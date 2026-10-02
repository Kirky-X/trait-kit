// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! 优雅关闭协调器 — 分阶段有序关闭 + 超时强退
//!
//! 提供 [`ShutdownCoordinator`]（同步）和 [`AsyncShutdownCoordinator`]（异步），
//! 支持注册关闭钩子到三个阶段：
//!
//! 1. [`ShutdownPhase::StopRequests`] — 停止接收新请求
//! 2. [`ShutdownPhase::DrainQueue`] — 排空队列中的待处理任务
//! 3. [`ShutdownPhase::CloseConnections`] — 关闭连接池等底层资源
//!
//! 每个阶段可设置超时，超时后强制进入下一阶段，确保关闭流程不会无限阻塞。
//!
//! # 超时语义（软限制）
//!
//! 阶段超时与全局超时均为**软限制**：受同步模型限制，运行中的 hook 不可被
//! 中断（`shutdown()` 会等待当前 hook 返回），因此超时保证的粒度是
//! **"hook 之间"**——全局预算在阶段边界与每个 hook 启动前检查，超预算则
//! 跳过剩余 hooks；单个长 hook 仍可能使实际耗时超出预算。
//!
//! # 异步停机入口（3 → 4 条）定位
//!
//! 停机桥接落地后，异步消费者的停机入口从三条变为四条，按"谁驱动、
//! 清理谁"划分，选型只看**宿主是否已有协调器骨架**：
//!
//! 1. `AsyncKit<Ready>` drop——RAII 兜底，能力随能力表释放，无顺序保证；
//! 2. `AsyncKit<Ready>::shutdown_async()`——Kit 模块级 `on_shutdown`，
//!    逆拓扑序；适合**没有**协调器的单 Kit 自治清理；
//! 3. `AsyncShutdownCoordinator::shutdown()`——宿主自有 Send 钩子的
//!    三阶段编排（Send 快路径，不执行 local 钩子）；
//! 4. `AsyncShutdownCoordinator::shutdown_local()`——3 的全量版：Send +
//!    local（`!Send` future）钩子，返回 `!Send` future 须原地 await；配
//!    `register_local_hook` 注册 `!Send` future 钩子，配
//!    `AsyncKit<Ready>::register_shutdown_into` 把 Kit 组件清理桥接进
//!    指定阶段（典型落位 `CloseConnections`）。
//!
//! 认知负担的收敛点：2 与 4 都能执行模块级清理，但**桥接即所有权转移**
//! ——`register_shutdown_into` 后 `shutdown_async()` 变 no-op，同一组件
//! 的清理 exactly-once，不会双跑。因此 2 与 4 是互斥选项而非并存选项；
//! 有协调器骨架的宿主（daemon 三阶段停机）选 1+3+4，把组件清理汇入既有
//! 编排，不要在协调器之外另开一条 `shutdown_async` 调用。

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;
#[cfg(feature = "async")]
use std::task::Poll;
use std::time::{Duration, Instant};

use std::cell::RefCell;
#[cfg(feature = "async")]
use std::sync::{Arc, RwLock};

use crate::error::TraitKitError;
use crate::i18n::tr;

/// 关闭阶段，按枚举定义顺序依次执行。
///
/// 每个阶段代表关闭流程的一个逻辑步骤，协调器按
/// `StopRequests → DrainQueue → CloseConnections` 的顺序执行。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShutdownPhase {
    /// 阶段 1：停止接收新请求（如关闭监听端口、标记 draining）。
    StopRequests,
    /// 阶段 2：排空队列中的待处理任务（如消息队列、任务队列）。
    DrainQueue,
    /// 阶段 3：关闭连接池等底层资源（如数据库、缓存、RPC 通道）。
    CloseConnections,
}

impl ShutdownPhase {
    /// 返回所有阶段的有序切片（按执行顺序）。
    #[must_use]
    pub fn all_phases() -> &'static [ShutdownPhase] {
        &[
            ShutdownPhase::StopRequests,
            ShutdownPhase::DrainQueue,
            ShutdownPhase::CloseConnections,
        ]
    }

    /// 返回阶段的可读名称。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::StopRequests => "stop_requests",
            Self::DrainQueue => "drain_queue",
            Self::CloseConnections => "close_connections",
        }
    }

    /// 返回阶段在内部数组中的索引（由枚举定义顺序保证 0..=2）。
    #[must_use]
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// 同步关闭钩子。
type SyncShutdownHook = Box<dyn FnOnce()>;

/// 单个阶段的钩子集合 + 超时配置。
struct PhaseConfig {
    hooks: Vec<SyncShutdownHook>,
    timeout: Duration,
}

impl PhaseConfig {
    fn new(timeout: Duration) -> Self {
        Self {
            hooks: Vec::new(),
            timeout,
        }
    }
}

/// 单个 hook 启动前的超时判定结果（sync/async 协调器共享）。
pub(crate) enum TimeoutDecision {
    /// 预算未耗尽，执行该 hook。
    RunHook,
    /// 阶段超时或全局超时，跳过剩余 hooks。
    SkipRemaining,
}

/// 软限制超时判定（sync/async 协调器共享）。
///
/// 阶段耗时达到 `timeout`，或全局 `deadline`（若有）已过，则判定为
/// [`TimeoutDecision::SkipRemaining`]；粒度为"hook 之间"，见模块文档。
pub(crate) fn evaluate_timeout(
    start: Instant,
    timeout: Duration,
    deadline: Option<Instant>,
) -> TimeoutDecision {
    if start.elapsed() >= timeout || deadline.is_some_and(|d| Instant::now() >= d) {
        TimeoutDecision::SkipRemaining
    } else {
        TimeoutDecision::RunHook
    }
}

/// 同步优雅关闭协调器。
///
/// 管理分阶段关闭流程：注册钩子 → 按阶段顺序执行 → 超时强退。
///
/// 使用 `RefCell` 实现单线程内部可变性（与 `Kit` 一致）。
///
/// # 示例
///
/// ```
/// use trait_kit::kit::shutdown::{ShutdownCoordinator, ShutdownPhase};
/// use std::time::Duration;
///
/// let coord = ShutdownCoordinator::new();
///
/// // 注册各阶段钩子
/// coord.register_hook(ShutdownPhase::StopRequests, || {
///     // 停止接收新请求
/// });
/// coord.register_hook(ShutdownPhase::DrainQueue, || {
///     // 排空任务队列
/// });
/// coord.register_hook(ShutdownPhase::CloseConnections, || {
///     // 关闭连接池
/// });
///
/// // 设置阶段超时
/// coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(5));
///
/// // 执行关闭流程
/// coord.shutdown();
/// ```
pub struct ShutdownCoordinator {
    phases: RefCell<[PhaseConfig; 3]>,
    /// 全局超时（整个关闭流程）。`None` 表示无全局超时。
    global_timeout: RefCell<Option<Duration>>,
}

impl ShutdownCoordinator {
    /// 创建新的协调器，默认每阶段超时 30 秒。
    #[must_use]
    pub fn new() -> Self {
        const DEFAULT_PHASE_TIMEOUT: Duration = Duration::from_secs(30);
        Self {
            phases: RefCell::new([
                PhaseConfig::new(DEFAULT_PHASE_TIMEOUT),
                PhaseConfig::new(DEFAULT_PHASE_TIMEOUT),
                PhaseConfig::new(DEFAULT_PHASE_TIMEOUT),
            ]),
            global_timeout: RefCell::new(None),
        }
    }

    /// 设置全局关闭超时。超时后剩余阶段仍会尝试执行，但结果中会包含超时信息。
    pub fn set_global_timeout(&self, timeout: Duration) {
        *self.global_timeout.borrow_mut() = Some(timeout);
    }

    /// 设置指定阶段的超时。
    pub fn set_phase_timeout(&self, phase: ShutdownPhase, timeout: Duration) {
        let idx = phase.index();
        self.phases.borrow_mut()[idx].timeout = timeout;
    }

    /// 注册一个关闭钩子到指定阶段。
    ///
    /// 同一阶段可注册多个钩子，按注册顺序执行。
    pub fn register_hook<F>(&self, phase: ShutdownPhase, hook: F)
    where
        F: FnOnce() + 'static,
    {
        let idx = phase.index();
        self.phases.borrow_mut()[idx].hooks.push(Box::new(hook));
    }

    /// 执行完整的分阶段关闭流程。
    ///
    /// 按 `StopRequests → DrainQueue → CloseConnections` 顺序执行。
    /// 每个阶段内的钩子按注册顺序执行。
    /// 单阶段超时后跳过剩余钩子，进入下一阶段。
    /// 全局超时在阶段边界与每个钩子启动前检查（软限制，粒度为"hook 之间"，
    /// 见模块文档），超预算则跳过剩余钩子并标记该阶段 `timed_out`。
    ///
    /// 返回各阶段执行结果；通过 [`ShutdownResult::is_ok`] /
    /// [`ShutdownResult::timed_out_phases`] 检查整体状态，
    /// 或经 [`ShutdownResult::into_result`] 转换为 `Result`。
    #[must_use = "shutdown returns phase results; ignoring it may hide timeout events"]
    pub fn shutdown(&self) -> ShutdownResult {
        let global_start = Instant::now();
        let global_timeout = *self.global_timeout.borrow();
        // 全局截止时刻：None 表示无全局超时
        let deadline = global_timeout.map(|t| global_start + t);
        let mut phases = Vec::with_capacity(3);

        for phase in ShutdownPhase::all_phases() {
            // 检查全局超时（阶段边界）
            if deadline.is_some_and(|d| Instant::now() >= d) {
                phases.push(ShutdownPhaseResult {
                    phase: *phase,
                    timed_out: true,
                    // 阶段被整体跳过、未执行任何钩子：elapsed 语义为"该阶段
                    // 实际耗时"，故为零（而非全局已流逝时间）。
                    elapsed: Duration::ZERO,
                    hook_failures: 0,
                });
                continue;
            }

            let result = self.execute_phase(*phase, deadline);
            phases.push(result);
        }

        ShutdownResult { phases }
    }

    /// 执行单个阶段的所有钩子。
    ///
    /// 超时为软限制：在每个钩子**启动前**检查阶段超时与全局截止
    /// （`deadline`），超预算则停止本阶段剩余钩子并返回 `timed_out`；
    /// 运行中的钩子不可被中断（同步模型限制），保证粒度为"hook 之间"。
    ///
    /// 单个 hook 的 panic 被隔离（`catch_unwind`）：计数进 `hook_failures`
    /// 后继续执行剩余钩子，保证"关闭流程必须走完"（与 `Kit::shutdown` 的
    /// 生命周期回调隔离标准一致）。
    fn execute_phase(
        &self,
        phase: ShutdownPhase,
        deadline: Option<Instant>,
    ) -> ShutdownPhaseResult {
        let idx = phase.index();
        let start = Instant::now();

        // 单次借用同时取出钩子（drain 避免重复执行）与阶段超时
        let (hooks, timeout) = {
            let mut phases = self.phases.borrow_mut();
            let hooks = std::mem::take(&mut phases[idx].hooks);
            let timeout = phases[idx].timeout;
            (hooks, timeout)
        };

        let mut hook_failures = 0usize;
        for hook in hooks {
            // 钩子启动前的最后检查：阶段超时或全局超时（软限制，"hook 之间"粒度）
            if matches!(
                evaluate_timeout(start, timeout, deadline),
                TimeoutDecision::SkipRemaining
            ) {
                return ShutdownPhaseResult {
                    phase,
                    timed_out: true,
                    elapsed: start.elapsed(),
                    hook_failures,
                };
            }
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(hook)).is_err() {
                hook_failures += 1;
            }
        }

        ShutdownPhaseResult {
            phase,
            timed_out: false,
            elapsed: start.elapsed(),
            hook_failures,
        }
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ShutdownCoordinator {
    /// 手写 `Debug`：打印结构名、阶段数与各阶段/全局超时概要。
    /// 不打印 hooks——闭包不可 `Debug`。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phases = self.phases.borrow();
        let timeouts: [Duration; 3] = [phases[0].timeout, phases[1].timeout, phases[2].timeout];
        f.debug_struct("ShutdownCoordinator")
            .field("phase_count", &phases.len())
            .field("phase_timeouts", &timeouts)
            .field("global_timeout", &*self.global_timeout.borrow())
            .finish()
    }
}

/// 单个阶段的关闭结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShutdownPhaseResult {
    /// 执行的阶段。
    pub phase: ShutdownPhase,
    /// 是否因超时跳过。
    pub timed_out: bool,
    /// 该阶段实际耗时。
    pub elapsed: Duration,
    /// 该阶段内 panic 被隔离的 hook 数量（单个 hook panic 不再中断
    /// 整个关闭流程，只计数并继续）。
    pub hook_failures: usize,
}

impl Default for ShutdownPhaseResult {
    /// 默认值：首阶段、未超时、零耗时、零 hook 失败（`ShutdownPhase` 为
    /// 枚举，无 `Default`，故手写实现而非 derive）。
    fn default() -> Self {
        Self {
            phase: ShutdownPhase::StopRequests,
            timed_out: false,
            elapsed: Duration::ZERO,
            hook_failures: 0,
        }
    }
}

impl ShutdownPhaseResult {
    /// 检查该阶段是否正常完成：未超时 **且** 没有 hook panic。
    #[must_use]
    pub fn is_ok(&self) -> bool {
        !self.timed_out && self.hook_failures == 0
    }
}

/// 关闭流程整体结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShutdownResult {
    /// 各阶段的结果。
    pub phases: Vec<ShutdownPhaseResult>,
}

impl ShutdownResult {
    /// 检查所有阶段是否都正常完成（无超时）。
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.phases.iter().all(ShutdownPhaseResult::is_ok)
    }

    /// 返回超时的阶段列表。
    #[must_use]
    pub fn timed_out_phases(&self) -> Vec<ShutdownPhase> {
        self.phases
            .iter()
            .filter(|r| r.timed_out)
            .map(|r| r.phase)
            .collect()
    }

    /// 转换为 `TraitKitResult`。若有任何阶段超时或 hook panic，返回错误。
    ///
    /// # Errors
    ///
    /// 当任何阶段超时时返回 `TraitKitError::ShutdownTimedOut`；当只有
    /// hook panic（无超时阶段）时返回 `TraitKitError::BuildFailed`，
    /// 错误信息包含被隔离的 hook 数量。
    pub fn into_result(self) -> Result<Self, TraitKitError> {
        if self.is_ok() {
            return Ok(self);
        }
        let timed_out = self.timed_out_phases();
        if !timed_out.is_empty() {
            return Err(TraitKitError::ShutdownTimedOut { phases: timed_out });
        }
        let failures: usize = self.phases.iter().map(|p| p.hook_failures).sum();
        Err(TraitKitError::BuildFailed {
            context: "shutdown".into(),
            source: Box::new(std::io::Error::other(tr(
                "trait-kit-error-shutdown-hooks-panicked",
                &[("failures", &failures.to_string())],
            ))),
        })
    }
}

impl std::ops::Deref for ShutdownResult {
    type Target = [ShutdownPhaseResult];

    /// 透明借用内部的阶段结果切片，支持 `len()` / `iter()` / 索引等切片操作
    /// （如 `result[0].is_ok()`）；整体状态判断优先使用 `is_ok()` /
    /// `timed_out_phases()`。
    fn deref(&self) -> &Self::Target {
        &self.phases
    }
}

// ─── Async 版本 ─────────────────────────────────────────────────────────────

/// 异步关闭钩子。
#[cfg(feature = "async")]
type AsyncShutdownHook =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// 本地（`!Send` future）异步关闭钩子。
///
/// 与 [`AsyncShutdownHook`] 平行的存储槽：闭包仍要求 `Send + Sync`（协调器
/// 的共享状态因此保持 `Send + Sync`，公开类型 auto-trait 零漂移），仅
/// **返回的 future 不加 `Send`**——future 在调用点生成、只在执行线程上
/// 存活，可以持有 `std::sync::RwLock` 读卫、`Rc` 等单线程资源（停机桥接
/// 的 `on_shutdown` future 正属此类）。这类钩子只能由
/// [`AsyncShutdownCoordinator::shutdown_local`]（返回 `!Send` future，须
/// 原地 await）执行，[`AsyncShutdownCoordinator::shutdown`] 的 Send 路径
/// 不会触碰它们。
#[cfg(feature = "async")]
type LocalShutdownHook = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync>;

/// 异步阶段的配置。
#[cfg(feature = "async")]
struct AsyncPhaseConfig {
    hooks: Vec<AsyncShutdownHook>,
    /// `!Send` 钩子的平行存储。不能与 `hooks` 共用槽位：`AsyncShutdownHook`
    /// 的闭包与 future 都带 `Send` bound，放宽即破坏既有 `shutdown()` 的
    /// `Send` 保证；执行顺序约定为同阶段内先 `hooks` 后 `local_hooks`，
    /// 各自保持注册序。
    local_hooks: Vec<LocalShutdownHook>,
    timeout: Duration,
}

/// poll 级 panic 隔离包装：把 hook future 的 panic 转为 `Err(payload)`。
///
/// `Pin<Box<dyn Future>>` 本身 `Unpin`，包装器按值持有并直接轮询，
/// 无需 pin 投影、无 unsafe（`async` feature 保持零依赖，不引入
/// `futures-util::FutureExt::catch_unwind`）。
#[cfg(feature = "async")]
struct CatchUnwindFuture {
    inner: Pin<Box<dyn Future<Output = ()> + Send>>,
}

#[cfg(feature = "async")]
impl Future for CatchUnwindFuture {
    type Output = Result<(), Box<dyn std::any::Any + Send>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        // Self: Unpin（唯一字段 `Pin<Box<..>>` 是 Unpin），可直接解引用。
        let this = &mut *self;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            this.inner.as_mut().poll(cx)
        })) {
            Ok(Poll::Ready(())) => Poll::Ready(Ok(())),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

/// [`CatchUnwindFuture`] 的 `!Send` 镜像：包装 [`LocalShutdownHook`] 产生
/// 的 future。panic 隔离语义与 Send 版完全一致，仅去掉 `Send` 约束。
#[cfg(feature = "async")]
struct LocalCatchUnwindFuture {
    inner: Pin<Box<dyn Future<Output = ()>>>,
}

#[cfg(feature = "async")]
impl Future for LocalCatchUnwindFuture {
    type Output = Result<(), Box<dyn std::any::Any + Send>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            this.inner.as_mut().poll(cx)
        })) {
            Ok(Poll::Ready(())) => Poll::Ready(Ok(())),
            Ok(Poll::Pending) => Poll::Pending,
            Err(payload) => Poll::Ready(Err(payload)),
        }
    }
}

#[cfg(feature = "async")]
impl AsyncPhaseConfig {
    fn new(timeout: Duration) -> Self {
        Self {
            hooks: Vec::new(),
            local_hooks: Vec::new(),
            timeout,
        }
    }
}

/// 异步协调器锁中毒 → `TraitKitError::BuildFailed` 的统一映射，
/// `context` 说明发生中毒的操作（经 [`tr`] 本地化的名词短语，嵌入
/// `failed to build {context}` 模板，其中 `{context}` 渲染时以反引号包裹）；
/// source 文案同样经 [`tr`] 输出。
#[cfg(feature = "async")]
fn lock_poisoned(context: String) -> TraitKitError {
    TraitKitError::BuildFailed {
        context,
        source: Box::new(std::io::Error::other(tr(
            "trait-kit-error-lock-poisoned-source",
            &[],
        ))),
    }
}

/// 异步优雅关闭协调器。
///
/// `ShutdownCoordinator` 的异步版本。使用 `Arc<RwLock>` 实现多线程安全的
/// 内部可变性，钩子返回 `Future`，支持 `await` 异步关闭操作。
///
/// 需要 `shutdown` + `async` features。
///
/// # 示例
///
/// ```ignore
/// use trait-kit::kit::shutdown::{AsyncShutdownCoordinator, ShutdownPhase};
/// use std::time::Duration;
///
/// let coord = AsyncShutdownCoordinator::new();
///
/// coord.register_hook(ShutdownPhase::StopRequests, || {
///     Box::pin(async {
///         // 异步停止接收请求
///     })
/// }).expect("register_hook failed");
///
/// let result = coord.shutdown().await;
/// ```
#[cfg(feature = "async")]
pub struct AsyncShutdownCoordinator {
    phases: Arc<RwLock<[AsyncPhaseConfig; 3]>>,
    global_timeout: Arc<RwLock<Option<Duration>>>,
}

#[cfg(feature = "async")]
impl AsyncShutdownCoordinator {
    /// 创建新的异步协调器，默认每阶段超时 30 秒。
    #[must_use]
    pub fn new() -> Self {
        const DEFAULT_PHASE_TIMEOUT: Duration = Duration::from_secs(30);
        Self {
            phases: Arc::new(RwLock::new([
                AsyncPhaseConfig::new(DEFAULT_PHASE_TIMEOUT),
                AsyncPhaseConfig::new(DEFAULT_PHASE_TIMEOUT),
                AsyncPhaseConfig::new(DEFAULT_PHASE_TIMEOUT),
            ])),
            global_timeout: Arc::new(RwLock::new(None)),
        }
    }

    /// 设置全局关闭超时。
    ///
    /// # Errors
    ///
    /// 当内部 `RwLock` 中毒时返回 `TraitKitError::BuildFailed`。
    pub fn set_global_timeout(&self, timeout: Duration) -> Result<(), TraitKitError> {
        let mut global = self.global_timeout.write().map_err(|_| {
            lock_poisoned(tr(
                "trait-kit-error-lock-poisoned-operation",
                &[("operation", "set_global_timeout")],
            ))
        })?;
        *global = Some(timeout);
        Ok(())
    }

    /// 设置指定阶段的超时。
    ///
    /// # Errors
    ///
    /// 当内部 `RwLock` 中毒时返回 `TraitKitError::BuildFailed`。
    pub fn set_phase_timeout(
        &self,
        phase: ShutdownPhase,
        timeout: Duration,
    ) -> Result<(), TraitKitError> {
        let idx = phase.index();
        let mut phases = self.phases.write().map_err(|_| {
            lock_poisoned(tr(
                "trait-kit-error-lock-poisoned-operation-phase",
                &[
                    ("operation", "set_phase_timeout"),
                    ("phase", phase.as_str()),
                ],
            ))
        })?;
        phases[idx].timeout = timeout;
        Ok(())
    }

    /// 注册一个异步关闭钩子到指定阶段。
    ///
    /// 钩子返回 `Pin<Box<dyn Future<Output = ()> + Send>>`；调用方闭包
    /// 通常写作 `|| Box::pin(async { ... })`，返回位置的 unsizing
    /// coercion 会自动完成擦除。
    ///
    /// # Errors
    ///
    /// 当内部锁中毒时返回 `TraitKitError::BuildFailed`。
    ///
    /// # Panics
    ///
    /// 当数组索引越界时 panic（不会发生，索引由枚举映射保证）。
    pub fn register_hook<F>(&self, phase: ShutdownPhase, hook: F) -> Result<(), TraitKitError>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync + 'static,
    {
        let idx = phase.index();
        // F 的返回类型已是擦除后的 trait object，直接 Box 一次即可
        self.phases
            .write()
            .map_err(|_| {
                lock_poisoned(tr(
                    "trait-kit-error-lock-poisoned-phase",
                    &[("phase", phase.as_str())],
                ))
            })?
            .get_mut(idx)
            .expect("index in range")
            .hooks
            .push(Box::new(hook));
        Ok(())
    }

    /// 注册一个本地（`!Send` future）异步关闭钩子到指定阶段。
    ///
    /// 与 [`register_hook`](Self::register_hook) 的区别仅在于**返回的
    /// future 不加 `Send`**：future 可以持有 `Rc`、`std::sync::RwLock`
    /// 读卫等单线程资源（闭包本身仍须 `Send + Sync`，见 `LocalShutdownHook`
    /// 的文档说明）。存储在平行槽位，与 Send 钩子互不干扰；执行只
    /// 发生在 [`shutdown_local`](Self::shutdown_local)，同阶段内先执行
    /// Send 钩子（注册序）、后执行 local 钩子（注册序）。
    ///
    /// # Errors
    ///
    /// 当内部锁中毒时返回 `TraitKitError::BuildFailed`（当前唯一的错误
    /// 变体；消费方如 `AsyncKit::register_shutdown_into` 的 stranded
    /// 包装对任意变体均成立，不得依赖此变体面收窄）。
    ///
    /// # Panics
    ///
    /// 当数组索引越界时 panic（不会发生，索引由枚举映射保证）。
    pub fn register_local_hook<F>(&self, phase: ShutdownPhase, hook: F) -> Result<(), TraitKitError>
    where
        F: FnOnce() -> Pin<Box<dyn Future<Output = ()>>> + Send + Sync + 'static,
    {
        let idx = phase.index();
        self.phases
            .write()
            .map_err(|_| {
                lock_poisoned(tr(
                    "trait-kit-error-lock-poisoned-phase",
                    &[("phase", phase.as_str())],
                ))
            })?
            .get_mut(idx)
            .expect("index in range")
            .local_hooks
            .push(Box::new(hook));
        Ok(())
    }

    /// 返回尚未执行的 local（`!Send` future）钩子总数（三阶段合计）。
    ///
    /// [`shutdown`](Self::shutdown) 不执行也不 drain local 槽位，只调
    /// Send 快路径的宿主可用本计数察觉"仍有组件清理待执行"——这类遗漏
    /// 在 [`ShutdownResult`] 里不可见（结果照常成功），必须显式探测。
    /// `shutdown_local` 按阶段 drain local 槽位（含超时丢弃），正常走完
    /// 后计数归零。
    ///
    /// # Errors
    ///
    /// 当内部 `RwLock` 中毒时返回 `TraitKitError::BuildFailed`，与同
    /// impl 块的 [`register_local_hook`](Self::register_local_hook) /
    /// [`shutdown_local`](Self::shutdown_local) 错误纪律一致——本方法
    /// 常被宿主的监控/健康线程调用，不得以 panic 击穿调用方。
    pub fn pending_local_hook_count(&self) -> Result<usize, TraitKitError> {
        self.phases
            .read()
            .map(|phases| phases.iter().map(|phase| phase.local_hooks.len()).sum())
            .map_err(|_| {
                lock_poisoned(tr(
                    "trait-kit-error-lock-poisoned-operation",
                    &[("operation", "pending_local_hook_count")],
                ))
            })
    }

    /// 执行完整的异步分阶段关闭流程。
    ///
    /// 全局超时在阶段边界与每个钩子启动前检查（软限制，粒度为"hook 之间"，
    /// 见模块文档），超预算则跳过剩余钩子并标记该阶段 `timed_out`。
    ///
    /// **不执行也不 drain local（`!Send`）槽位**：本方法的 `Send` 保证与
    /// `!Send` 钩子天然互斥。若协调器同时承载了经
    /// [`register_local_hook`](Self::register_local_hook) /
    /// `AsyncKit<Ready>::register_shutdown_into` 注册的清理钩子，只调本
    /// 方法会使其**静默遗留**——钩子不执行，结果照常报告成功。遗留可用
    /// [`pending_local_hook_count`](Self::pending_local_hook_count) 探测，
    /// 或改用全量版 [`shutdown_local`](Self::shutdown_local)。
    ///
    /// # Errors
    ///
    /// 当内部 `RwLock` 中毒时返回 `TraitKitError::BuildFailed`。
    #[must_use = "shutdown returns phase result; ignoring it may hide timeout events"]
    pub async fn shutdown(&self) -> Result<ShutdownResult, TraitKitError> {
        let global_start = Instant::now();
        let global_timeout = *self.global_timeout.read().map_err(|_| {
            lock_poisoned(tr(
                "trait-kit-error-lock-poisoned-operation",
                &[("operation", "shutdown")],
            ))
        })?;
        // 全局截止时刻：None 表示无全局超时
        let deadline = global_timeout.map(|t| global_start + t);
        let mut phases = Vec::with_capacity(3);

        for phase in ShutdownPhase::all_phases() {
            // 检查全局超时（阶段边界）
            if deadline.is_some_and(|d| Instant::now() >= d) {
                phases.push(ShutdownPhaseResult {
                    phase: *phase,
                    timed_out: true,
                    // 阶段被整体跳过、未执行任何钩子：elapsed 语义为"该阶段
                    // 实际耗时"，故为零（而非全局已流逝时间）。
                    elapsed: Duration::ZERO,
                    hook_failures: 0,
                });
                continue;
            }

            let result = self.execute_phase(*phase, deadline).await?;
            phases.push(result);
        }

        Ok(ShutdownResult { phases })
    }

    /// 执行单个异步阶段。
    ///
    /// 钩子集合与阶段超时在**一次写锁**内同时取出（避免 TOCTOU：两次锁
    ///  acquisition 之间超时可能被并发修改）。
    ///
    /// 超时为软限制：在每个钩子**启动前**检查阶段超时与全局截止
    /// （`deadline`），超预算则停止本阶段剩余钩子并返回 `timed_out`；
    /// 运行中的钩子不可被中断，保证粒度为"hook 之间"。
    ///
    /// 单个 hook 的 panic 被隔离（`CatchUnwindFuture`，poll 级
    /// `catch_unwind`）：计数进 `hook_failures` 后继续执行剩余钩子。
    ///
    /// # Errors
    ///
    /// 当内部 `RwLock` 中毒时返回 `TraitKitError::BuildFailed`。
    async fn execute_phase(
        &self,
        phase: ShutdownPhase,
        deadline: Option<Instant>,
    ) -> Result<ShutdownPhaseResult, TraitKitError> {
        let idx = phase.index();
        let start = Instant::now();

        // 单次写锁同时取出钩子与阶段超时
        let (hooks, timeout) = {
            let mut phases = self.phases.write().map_err(|_| {
                lock_poisoned(tr(
                    "trait-kit-error-lock-poisoned-operation-phase",
                    &[("operation", "execute_phase"), ("phase", phase.as_str())],
                ))
            })?;
            let hooks = std::mem::take(&mut phases[idx].hooks);
            let timeout = phases[idx].timeout;
            (hooks, timeout)
        };

        let mut hook_failures = 0usize;
        for hook in hooks {
            // 钩子启动前的最后检查：阶段超时或全局超时（软限制，"hook 之间"粒度）
            if matches!(
                evaluate_timeout(start, timeout, deadline),
                TimeoutDecision::SkipRemaining
            ) {
                return Ok(ShutdownPhaseResult {
                    phase,
                    timed_out: true,
                    elapsed: start.elapsed(),
                    hook_failures,
                });
            }
            let outcome = (CatchUnwindFuture { inner: hook() }).await;
            if outcome.is_err() {
                hook_failures += 1;
            }
        }

        Ok(ShutdownPhaseResult {
            phase,
            timed_out: false,
            elapsed: start.elapsed(),
            hook_failures,
        })
    }

    /// 执行完整的分阶段关闭流程（Send + local 全量版）。
    ///
    /// 流程骨架与 [`shutdown`](Self::shutdown) 一致；差异在于每个阶段
    /// 在 Send 钩子执行完后，继续执行 local（`!Send`）钩子（注册序，
    /// 由 `execute_local_phase` 驱动），并把
    /// 两段的超时/panic 计数合并为**单个** [`ShutdownPhaseResult`]，
    /// 保持 `ShutdownResult::phases` 每阶段一条的形状不变。
    ///
    /// 超时语义仍是软限制：阶段超时窗口对 Send 段与 local 段**各自生效**
    /// ——Send 段用满窗口（`timed_out`）后，local 段以全新 `start` 独立
    /// 重置窗口，因此**单阶段最坏耗时 ≈ 2×阶段超时**（两段各用满一个
    /// 窗口）；全局 `deadline` 则贯穿两段不受影响，宿主设定阶段/全局
    /// 超时时应把这一预算翻倍后果计入。返回的 future 是
    /// **`!Send`**（local 钩子 future 不加 `Send`）：必须原地 await，
    /// 不能抛给多线程运行时 spawn。
    ///
    /// [`shutdown`](Self::shutdown) 不会执行 local 钩子——它的 `Send`
    /// 保证与 local 槽位天然互斥，两条路径各自 drain、互不越界。
    ///
    /// # Errors
    ///
    /// 当内部 `RwLock` 中毒时返回 `TraitKitError::BuildFailed`。
    #[must_use = "shutdown returns phase result; ignoring it may hide timeout events"]
    pub async fn shutdown_local(&self) -> Result<ShutdownResult, TraitKitError> {
        let global_start = Instant::now();
        let global_timeout = *self.global_timeout.read().map_err(|_| {
            lock_poisoned(tr(
                "trait-kit-error-lock-poisoned-operation",
                &[("operation", "shutdown_local")],
            ))
        })?;
        // 全局截止时刻：None 表示无全局超时
        let deadline = global_timeout.map(|t| global_start + t);
        let mut phases = Vec::with_capacity(3);

        for phase in ShutdownPhase::all_phases() {
            // 检查全局超时（阶段边界）
            if deadline.is_some_and(|d| Instant::now() >= d) {
                phases.push(ShutdownPhaseResult {
                    phase: *phase,
                    timed_out: true,
                    // 阶段被整体跳过、未执行任何钩子：elapsed 语义为"该阶段
                    // 实际耗时"，故为零（而非全局已流逝时间）。
                    elapsed: Duration::ZERO,
                    hook_failures: 0,
                });
                continue;
            }

            // 先 Send 钩子后 local 钩子（各自注册序），结果合并为一条。
            let send_result = self.execute_phase(*phase, deadline).await?;
            let local_result = self.execute_local_phase(*phase, deadline).await?;
            phases.push(ShutdownPhaseResult {
                phase: *phase,
                timed_out: send_result.timed_out || local_result.timed_out,
                elapsed: send_result.elapsed + local_result.elapsed,
                hook_failures: send_result.hook_failures + local_result.hook_failures,
            });
        }

        Ok(ShutdownResult { phases })
    }

    /// 执行单个阶段的 local（`!Send`）钩子，[`execute_phase`](Self::execute_phase)
    /// 的镜像：同一次写锁取出 `local_hooks` 与阶段超时，超时软限制与
    /// poll 级 panic 隔离（`LocalCatchUnwindFuture`）语义一致。
    ///
    /// # Errors
    ///
    /// 当内部 `RwLock` 中毒时返回 `TraitKitError::BuildFailed`。
    async fn execute_local_phase(
        &self,
        phase: ShutdownPhase,
        deadline: Option<Instant>,
    ) -> Result<ShutdownPhaseResult, TraitKitError> {
        let idx = phase.index();
        let start = Instant::now();

        // 单次写锁同时取出 local 钩子与阶段超时（与 execute_phase 同纪律）
        let (hooks, timeout) = {
            let mut phases = self.phases.write().map_err(|_| {
                lock_poisoned(tr(
                    "trait-kit-error-lock-poisoned-operation-phase",
                    &[
                        ("operation", "execute_local_phase"),
                        ("phase", phase.as_str()),
                    ],
                ))
            })?;
            let hooks = std::mem::take(&mut phases[idx].local_hooks);
            let timeout = phases[idx].timeout;
            (hooks, timeout)
        };

        let mut hook_failures = 0usize;
        for hook in hooks {
            // 钩子启动前的最后检查：阶段超时或全局超时（软限制，"hook 之间"粒度）
            if matches!(
                evaluate_timeout(start, timeout, deadline),
                TimeoutDecision::SkipRemaining
            ) {
                return Ok(ShutdownPhaseResult {
                    phase,
                    timed_out: true,
                    elapsed: start.elapsed(),
                    hook_failures,
                });
            }
            let outcome = (LocalCatchUnwindFuture { inner: hook() }).await;
            if outcome.is_err() {
                hook_failures += 1;
            }
        }

        Ok(ShutdownPhaseResult {
            phase,
            timed_out: false,
            elapsed: start.elapsed(),
            hook_failures,
        })
    }
}

#[cfg(feature = "async")]
impl Default for AsyncShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "async")]
impl std::fmt::Debug for AsyncShutdownCoordinator {
    /// 手写 `Debug`：打印结构名、阶段数、各阶段/全局超时与待执行 local
    /// 钩子计数。不打印 hooks——闭包不可 `Debug`；计数使"只调 Send 快
    /// 路径导致 local 清理遗留"在调试输出中可被察觉。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phases = self.phases.read().expect("lock poisoned");
        let timeouts: [Duration; 3] = [phases[0].timeout, phases[1].timeout, phases[2].timeout];
        let pending_local: usize = phases.iter().map(|phase| phase.local_hooks.len()).sum();
        f.debug_struct("AsyncShutdownCoordinator")
            .field("phase_count", &phases.len())
            .field("phase_timeouts", &timeouts)
            .field(
                "global_timeout",
                &*self.global_timeout.read().expect("lock poisoned"),
            )
            .field("pending_local_hooks", &pending_local)
            .finish()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn shutdown_phase_all_phases_returns_three() {
        assert_eq!(ShutdownPhase::all_phases().len(), 3);
    }

    #[test]
    fn shutdown_phase_as_str_returns_readable_name() {
        assert_eq!(ShutdownPhase::StopRequests.as_str(), "stop_requests");
        assert_eq!(ShutdownPhase::DrainQueue.as_str(), "drain_queue");
        assert_eq!(
            ShutdownPhase::CloseConnections.as_str(),
            "close_connections"
        );
    }

    #[test]
    fn shutdown_coordinator_executes_hooks_in_order() {
        static ORDER: AtomicUsize = AtomicUsize::new(0);

        let coord = ShutdownCoordinator::new();
        coord.register_hook(ShutdownPhase::StopRequests, || {
            assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 0);
        });
        coord.register_hook(ShutdownPhase::StopRequests, || {
            assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 1);
        });
        coord.register_hook(ShutdownPhase::DrainQueue, || {
            assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 2);
        });
        coord.register_hook(ShutdownPhase::CloseConnections, || {
            assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 3);
        });

        let result = coord.shutdown();
        assert_eq!(result.phases.len(), 3);
        assert!(result.is_ok());
        assert_eq!(ORDER.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn shutdown_coordinator_phase_order_is_correct() {
        static PHASE_ORDER: std::sync::Mutex<Vec<ShutdownPhase>> =
            std::sync::Mutex::new(Vec::new());

        let coord = ShutdownCoordinator::new();
        coord.register_hook(ShutdownPhase::CloseConnections, || {
            PHASE_ORDER
                .lock()
                .unwrap()
                .push(ShutdownPhase::CloseConnections);
        });
        coord.register_hook(ShutdownPhase::StopRequests, || {
            PHASE_ORDER
                .lock()
                .unwrap()
                .push(ShutdownPhase::StopRequests);
        });
        coord.register_hook(ShutdownPhase::DrainQueue, || {
            PHASE_ORDER.lock().unwrap().push(ShutdownPhase::DrainQueue);
        });

        let _ = coord.shutdown();

        let order = PHASE_ORDER.lock().unwrap();
        assert_eq!(
            *order,
            vec![
                ShutdownPhase::StopRequests,
                ShutdownPhase::DrainQueue,
                ShutdownPhase::CloseConnections,
            ]
        );
    }

    #[test]
    fn shutdown_coordinator_timeout_skips_remaining_hooks() {
        static CALLED: AtomicUsize = AtomicUsize::new(0);

        let coord = ShutdownCoordinator::new();
        // 设置 10ms 超时：足够第一个钩子执行，但不够 50ms 的 sleep
        coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_millis(10));

        // 第一阶段正常
        coord.register_hook(ShutdownPhase::StopRequests, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });

        // 第二阶段：先注册一个 sleep 超过超时的钩子
        coord.register_hook(ShutdownPhase::DrainQueue, || {
            std::thread::sleep(Duration::from_millis(50));
            CALLED.fetch_add(1, Ordering::SeqCst);
        });
        // 这个钩子不应该被执行（超时跳过）
        coord.register_hook(ShutdownPhase::DrainQueue, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });

        // 第三阶段正常
        coord.register_hook(ShutdownPhase::CloseConnections, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });

        let result = coord.shutdown();
        assert_eq!(result.phases.len(), 3);

        // DrainQueue 应该超时
        assert!(result.phases[0].is_ok()); // StopRequests
        assert!(result.phases[1].timed_out); // DrainQueue
        assert!(result.phases[2].is_ok()); // CloseConnections

        // StopRequests(1) + DrainQueue[0](1) + CloseConnections(1) = 3
        // DrainQueue[1] 被超时跳过
        assert_eq!(CALLED.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn shutdown_coordinator_global_timeout() {
        static CALLED: AtomicUsize = AtomicUsize::new(0);

        let coord = ShutdownCoordinator::new();
        // 全局超时 10ms：StopRequests 的 sleep 50ms 会超过全局超时
        coord.set_global_timeout(Duration::from_millis(10));

        coord.register_hook(ShutdownPhase::StopRequests, || {
            std::thread::sleep(Duration::from_millis(50));
            CALLED.fetch_add(1, Ordering::SeqCst);
        });
        coord.register_hook(ShutdownPhase::DrainQueue, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });
        coord.register_hook(ShutdownPhase::CloseConnections, || {
            CALLED.fetch_add(1, Ordering::SeqCst);
        });

        let result = coord.shutdown();
        assert_eq!(result.phases.len(), 3);
        // StopRequests 执行完（phase timeout 30s 足够），但全局超时导致后续阶段被跳过
        // StopRequests 钩子执行后 CALLED=1，全局超时已超，后续阶段被跳过
        assert_eq!(CALLED.load(Ordering::SeqCst), 1);
        assert!(result.phases[0].is_ok()); // StopRequests 正常完成
        assert!(result.phases[1].timed_out); // DrainQueue 被全局超时跳过
        assert!(result.phases[2].timed_out); // CloseConnections 被全局超时跳过
    }

    #[test]
    fn shutdown_coordinator_global_timeout_checked_between_hooks_in_phase() {
        // 新语义：全局超时不仅在阶段之间检查，也在同一阶段内的每个 hook
        // 启动前检查（软限制："hook 之间"粒度，运行中的 hook 不可中断）。
        static CALLED: AtomicUsize = AtomicUsize::new(0);

        let coord = ShutdownCoordinator::new();
        // 全局预算 20ms：第一个 hook sleep 50ms 耗尽预算后，第二个 hook 必须跳过
        coord.set_global_timeout(Duration::from_millis(20));

        coord.register_hook(ShutdownPhase::StopRequests, || {
            std::thread::sleep(Duration::from_millis(50));
            CALLED.fetch_add(1, Ordering::SeqCst); // 执行（启动时预算未超）
        });
        coord.register_hook(ShutdownPhase::StopRequests, || {
            CALLED.fetch_add(1, Ordering::SeqCst); // 不应执行：全局预算已耗尽
        });
        coord.register_hook(ShutdownPhase::DrainQueue, || {
            CALLED.fetch_add(1, Ordering::SeqCst); // 不应执行
        });

        let result = coord.shutdown();
        assert_eq!(CALLED.load(Ordering::SeqCst), 1, "只有第一个 hook 执行");
        // StopRequests 启动了第一个 hook 但跳过了剩余 hooks → timed_out
        assert!(
            result.phases[0].timed_out,
            "超全局预算后本阶段剩余 hooks 应被跳过: {result:?}"
        );
        assert!(result.phases[1].timed_out); // DrainQueue 阶段边界被跳过
        assert!(result.phases[2].timed_out); // CloseConnections 阶段边界被跳过
    }

    #[test]
    fn shutdown_results_equality_and_default() {
        let a = ShutdownPhaseResult {
            phase: ShutdownPhase::DrainQueue,
            timed_out: true,
            elapsed: Duration::from_millis(5),
            hook_failures: 0,
        };
        let b = ShutdownPhaseResult {
            phase: ShutdownPhase::DrainQueue,
            timed_out: true,
            elapsed: Duration::from_millis(5),
            hook_failures: 0,
        };
        let c = ShutdownPhaseResult {
            timed_out: false,
            ..b.clone()
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(
            ShutdownPhaseResult::default(),
            ShutdownPhaseResult {
                phase: ShutdownPhase::StopRequests,
                timed_out: false,
                elapsed: Duration::ZERO,
                hook_failures: 0,
            }
        );

        let r1 = ShutdownResult { phases: vec![a] };
        let r2 = ShutdownResult { phases: vec![b] };
        assert_eq!(r1, r2);
        assert_eq!(
            ShutdownResult::default(),
            ShutdownResult { phases: Vec::new() }
        );
    }

    #[test]
    fn shutdown_coordinator_debug_omits_hooks() {
        let coord = ShutdownCoordinator::new();
        coord.set_global_timeout(Duration::from_secs(7));
        coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(3));
        let dbg = format!("{coord:?}");
        assert!(dbg.contains("ShutdownCoordinator"), "got: {dbg}");
        assert!(dbg.contains("3s"), "phase timeout missing: {dbg}");
        assert!(dbg.contains("Some(7s)"), "global timeout missing: {dbg}");
    }

    #[test]
    fn shutdown_result_into_result_ok() {
        let result = ShutdownResult {
            phases: vec![
                ShutdownPhaseResult {
                    phase: ShutdownPhase::StopRequests,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                    hook_failures: 0,
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::DrainQueue,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                    hook_failures: 0,
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::CloseConnections,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                    hook_failures: 0,
                },
            ],
        };
        assert!(result.is_ok());
        assert!(result.timed_out_phases().is_empty());
        assert!(result.into_result().is_ok());
    }

    #[test]
    fn shutdown_result_into_result_timeout() {
        let result = ShutdownResult {
            phases: vec![
                ShutdownPhaseResult {
                    phase: ShutdownPhase::StopRequests,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                    hook_failures: 0,
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::DrainQueue,
                    timed_out: true,
                    elapsed: Duration::from_secs(30),
                    hook_failures: 0,
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::CloseConnections,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                    hook_failures: 0,
                },
            ],
        };
        assert!(!result.is_ok());
        assert_eq!(result.timed_out_phases(), vec![ShutdownPhase::DrainQueue]);
        let err = result.into_result().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("drain_queue"),
            "error should mention timed out phase: {msg}"
        );
    }

    #[test]
    fn shutdown_coordinator_default_works() {
        let coord = ShutdownCoordinator::default();
        let result = coord.shutdown();
        assert_eq!(result.phases.len(), 3);
        assert!(result.is_ok());
    }

    #[test]
    fn shutdown_coordinator_empty_phases_succeed() {
        let coord = ShutdownCoordinator::new();
        let result = coord.shutdown();
        assert_eq!(result.phases.len(), 3);
        for r in &result.phases {
            assert!(r.is_ok());
            assert!(
                r.elapsed.as_nanos() < 1_000_000,
                "empty phase should be near-instant"
            );
        }
    }

    #[test]
    fn shutdown_coordinator_normal_path_returns_ok() {
        // 正常路径：set_* 与 shutdown / execute_phase 均成功
        let coord = ShutdownCoordinator::new();
        coord.set_global_timeout(Duration::from_secs(1));
        coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(1));
        coord.register_hook(ShutdownPhase::StopRequests, || {});

        let phase = coord.execute_phase(ShutdownPhase::StopRequests, None);
        assert!(phase.is_ok());
        assert!(coord.shutdown().is_ok());
    }

    #[test]
    fn shutdown_coordinator_hooks_not_reentrant() {
        static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

        let coord = ShutdownCoordinator::new();
        coord.register_hook(ShutdownPhase::StopRequests, || {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        });

        // 第一次 shutdown
        let _ = coord.shutdown();
        assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);

        // 第二次 shutdown — 钩子已被 drain，不应再执行
        let _ = coord.shutdown();
        assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
    }

    /// 一个 hook panic 不得中断整个关闭流程：后续 hook 与阶段照常执行，
    /// 失败计数记入该阶段结果。
    #[test]
    fn shutdown_coordinator_isolates_panicking_hook() {
        use std::sync::Arc;

        let coord = ShutdownCoordinator::new();
        let ran_after = Arc::new(AtomicUsize::new(0));
        let ran_after2 = Arc::clone(&ran_after);
        let ran_late_phase = Arc::new(AtomicUsize::new(0));
        let ran_late_phase2 = Arc::clone(&ran_late_phase);

        // 第一阶段：panic hook → 后续阶段必须照常
        coord.register_hook(ShutdownPhase::StopRequests, || {
            panic!("hook boom");
        });
        // 同阶段后续 hook 必须继续执行
        coord.register_hook(ShutdownPhase::StopRequests, move || {
            ran_after2.fetch_add(1, Ordering::SeqCst);
        });
        // 下一阶段必须照常执行
        coord.register_hook(ShutdownPhase::DrainQueue, move || {
            ran_late_phase2.fetch_add(1, Ordering::SeqCst);
        });

        let result = coord.shutdown();
        assert_eq!(
            ran_after.load(Ordering::SeqCst),
            1,
            "同阶段 panic 之后的 hook 仍须执行"
        );
        assert_eq!(
            ran_late_phase.load(Ordering::SeqCst),
            1,
            "panic 之后的阶段仍须执行"
        );
        let stop = result
            .phases
            .iter()
            .find(|p| p.phase == ShutdownPhase::StopRequests)
            .expect("StopRequests phase result");
        assert_eq!(stop.hook_failures, 1, "恰好一个 panic hook 被计数");
        assert!(!stop.is_ok(), "hook 失败后该阶段 is_ok 必须为 false");
        assert!(!result.is_ok(), "整体 is_ok 必须反映 hook 失败");
    }
}

#[cfg(all(test, feature = "async"))]
mod async_tests {
    use super::*;
    use crate::test_helpers::block_on;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn async_shutdown_coordinator_executes_hooks() {
        static CALLED: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .register_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        CALLED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        CALLED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::CloseConnections, || {
                    Box::pin(async {
                        CALLED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            let result = coord.shutdown().await.unwrap();
            assert!(result.is_ok());
            assert_eq!(CALLED.load(Ordering::SeqCst), 3);
        });
    }

    #[test]
    fn async_shutdown_coordinator_normal_path_returns_ok() {
        // 正常路径：set_* / execute_phase / shutdown 均返回 Ok
        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            assert!(coord.set_global_timeout(Duration::from_secs(1)).is_ok());
            assert!(
                coord
                    .set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(1))
                    .is_ok()
            );
            coord
                .register_hook(ShutdownPhase::StopRequests, || Box::pin(async {}))
                .unwrap();

            assert!(
                coord
                    .execute_phase(ShutdownPhase::StopRequests, None)
                    .await
                    .is_ok()
            );
            assert!(coord.shutdown().await.is_ok());
        });
    }

    #[test]
    fn async_shutdown_coordinator_timeout() {
        static CALLED: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_millis(10))
                .unwrap();

            coord
                .register_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        CALLED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        // 模拟长时间异步操作
                        std::thread::sleep(Duration::from_millis(50));
                        CALLED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        // 应被超时跳过
                        CALLED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::CloseConnections, || {
                    Box::pin(async {
                        CALLED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            let result = coord.shutdown().await.unwrap();
            assert!(!result.is_ok());
            assert_eq!(result.timed_out_phases(), vec![ShutdownPhase::DrainQueue]);
            // StopRequests(1) + DrainQueue first hook(1) + CloseConnections(1) = 3
            // DrainQueue second hook skipped due to timeout
            assert_eq!(CALLED.load(Ordering::SeqCst), 3);
        });
    }

    #[test]
    fn async_shutdown_coordinator_default() {
        block_on(async {
            let coord = AsyncShutdownCoordinator::default();
            let result = coord.shutdown().await.unwrap();
            assert!(result.is_ok());
        });
    }

    #[test]
    fn async_shutdown_coordinator_global_timeout_checked_between_hooks_in_phase() {
        // 新语义（与同步版一致）：全局超时在同一阶段内的每个 hook 启动前
        // 也检查（软限制："hook 之间"粒度）。
        static CALLED: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            // 全局预算 20ms：第一个 hook 阻塞 50ms 耗尽预算，第二个 hook 必须跳过
            coord.set_global_timeout(Duration::from_millis(20)).unwrap();

            coord
                .register_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        std::thread::sleep(Duration::from_millis(50));
                        CALLED.fetch_add(1, Ordering::SeqCst); // 执行（启动时预算未超）
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        CALLED.fetch_add(1, Ordering::SeqCst); // 不应执行：预算已耗尽
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        CALLED.fetch_add(1, Ordering::SeqCst); // 不应执行
                    })
                })
                .unwrap();

            let result = coord.shutdown().await.unwrap();
            assert_eq!(CALLED.load(Ordering::SeqCst), 1, "只有第一个 hook 执行");
            assert!(
                result.phases[0].timed_out,
                "超全局预算后本阶段剩余 hooks 应被跳过: {result:?}"
            );
            assert!(result.phases[1].timed_out);
            assert!(result.phases[2].timed_out);
        });
    }

    #[test]
    fn async_shutdown_coordinator_debug_omits_hooks() {
        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord.set_global_timeout(Duration::from_secs(7)).unwrap();
            coord
                .set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_secs(3))
                .unwrap();
            let dbg = format!("{coord:?}");
            assert!(dbg.contains("AsyncShutdownCoordinator"), "got: {dbg}");
            assert!(dbg.contains("3s"), "phase timeout missing: {dbg}");
            assert!(dbg.contains("Some(7s)"), "global timeout missing: {dbg}");
        });
    }

    #[test]
    fn async_shutdown_coordinator_hooks_not_reentrant() {
        static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .register_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        CALL_COUNT.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            coord.shutdown().await.unwrap();
            assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);

            // 第二次 shutdown — 钩子已被 drain，不应再执行
            coord.shutdown().await.unwrap();
            assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
        });
    }

    /// async 侧 hook panic 同样被隔离：后续 hook 与阶段照常执行，
    /// 失败计数进 `hook_failures`。
    #[test]
    fn async_shutdown_isolates_panicking_hook() {
        use std::sync::Arc;

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            let ran_after = Arc::new(AtomicUsize::new(0));
            let ran_after2 = Arc::clone(&ran_after);

            coord
                .register_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        panic!("async hook boom");
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::StopRequests, move || {
                    let r = Arc::clone(&ran_after2);
                    Box::pin(async move {
                        r.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            let result = coord.shutdown().await.unwrap();
            assert_eq!(
                ran_after.load(Ordering::SeqCst),
                1,
                "同阶段 panic 之后的 hook 仍须执行"
            );
            let stop = result
                .phases
                .iter()
                .find(|p| p.phase == ShutdownPhase::StopRequests)
                .expect("StopRequests phase result");
            assert_eq!(stop.hook_failures, 1, "恰好一个 panic hook 被计数");
            assert!(!stop.is_ok());
            assert!(!result.is_ok());
        });
    }

    // ─── local(!Send)钩子:平行槽位与 shutdown_local 执行语义 ───

    #[test]
    fn local_hook_future_may_hold_non_send_state_and_runs() {
        static TOUCHED: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .register_local_hook(ShutdownPhase::CloseConnections, || {
                    // Rc 在闭包体内创建并移入 future：future 持有 Rc（!Send），
                    // 而闭包本身无捕获、保持 Send+Sync（平行槽位的约束面）。
                    // 若槽位重新要求 future: Send，此钩子将无法编译。
                    let hits = std::rc::Rc::new(std::cell::Cell::new(0usize));
                    let probe = std::rc::Rc::clone(&hits);
                    Box::pin(async move {
                        probe.set(probe.get() + 1);
                        TOUCHED.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            let result = coord.shutdown_local().await.unwrap();
            assert!(result.is_ok());
            assert_eq!(
                TOUCHED.load(Ordering::SeqCst),
                1,
                "local 钩子须由 shutdown_local() 恰好执行一次"
            );
        });
    }

    #[test]
    fn shutdown_send_path_skips_local_hooks() {
        static LOCAL_RUNS: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .register_local_hook(ShutdownPhase::CloseConnections, || {
                    Box::pin(async {
                        LOCAL_RUNS.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            // Send 快路径不得触碰 local 槽位（其 future 的 Send 保证与
            // !Send 钩子天然互斥），两条路径各自 drain。
            let send_result = coord.shutdown().await.unwrap();
            assert!(send_result.is_ok());
            assert_eq!(
                LOCAL_RUNS.load(Ordering::SeqCst),
                0,
                "shutdown() 不得执行 local 钩子"
            );

            let local_result = coord.shutdown_local().await.unwrap();
            assert!(local_result.is_ok());
            assert_eq!(
                LOCAL_RUNS.load(Ordering::SeqCst),
                1,
                "shutdown_local() 执行 local 钩子"
            );
        });
    }

    #[test]
    fn shutdown_local_runs_send_hooks_before_local_hooks_in_phase() {
        static ORDER: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            // local 钩子先注册，仍须排在 Send 钩子之后：同阶段顺序约定为
            // "先 hooks 后 local_hooks"，与注册先后无关，各自保持注册序。
            coord
                .register_local_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        assert_eq!(
                            ORDER.fetch_add(1, Ordering::SeqCst),
                            1,
                            "local 钩子在 Send 钩子之后执行"
                        );
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        assert_eq!(ORDER.fetch_add(1, Ordering::SeqCst), 0, "Send 钩子先执行");
                    })
                })
                .unwrap();

            let result = coord.shutdown_local().await.unwrap();
            assert!(result.is_ok());
            // Send 段与 local 段合并为每阶段一条结果，形状与 shutdown() 一致。
            assert_eq!(result.phases.len(), 3);
        });
    }

    #[test]
    fn local_hook_panic_is_isolated_and_counted() {
        static RAN_AFTER: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .register_local_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        panic!("local hook boom");
                    })
                })
                .unwrap();
            coord
                .register_local_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        RAN_AFTER.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            let result = coord.shutdown_local().await.unwrap();
            assert_eq!(
                RAN_AFTER.load(Ordering::SeqCst),
                1,
                "同阶段 panic 之后的 local 钩子仍须执行"
            );
            let stop = result
                .phases
                .iter()
                .find(|p| p.phase == ShutdownPhase::StopRequests)
                .expect("StopRequests phase result");
            assert_eq!(stop.hook_failures, 1, "恰好一个 panic 的 local 钩子被计数");
            assert!(!result.is_ok());
        });
    }

    #[test]
    fn shutdown_send_path_leaves_pending_local_hooks_detectable() {
        static LOCAL_RUNS: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .register_local_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        LOCAL_RUNS.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_local_hook(ShutdownPhase::CloseConnections, || {
                    Box::pin(async {
                        LOCAL_RUNS.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            // Send 快路径：结果照常成功，local 钩子静默遗留——
            // 遗留必须能被 pending_local_hook_count() 探测。
            let send_result = coord.shutdown().await.unwrap();
            assert!(send_result.is_ok(), "Send 快路径结果不含 local 信息");
            assert_eq!(LOCAL_RUNS.load(Ordering::SeqCst), 0);
            assert_eq!(
                coord.pending_local_hook_count().unwrap(),
                2,
                "遗留的 local 钩子必须可被探测，否则清理丢失不可察觉"
            );

            let local_result = coord.shutdown_local().await.unwrap();
            assert!(local_result.is_ok());
            assert_eq!(LOCAL_RUNS.load(Ordering::SeqCst), 2);
            assert_eq!(
                coord.pending_local_hook_count().unwrap(),
                0,
                "shutdown_local() 正常走完后计数归零"
            );
        });
    }

    #[test]
    fn shutdown_local_phase_timeout_skips_remaining_local_hooks() {
        static RAN: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .set_phase_timeout(ShutdownPhase::StopRequests, Duration::from_millis(30))
                .unwrap();
            coord
                .register_local_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        // 耗尽本阶段窗口
                        std::thread::sleep(Duration::from_millis(60));
                        RAN.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_local_hook(ShutdownPhase::StopRequests, || {
                    Box::pin(async {
                        // 应被超时跳过
                        RAN.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();

            let result = coord.shutdown_local().await.unwrap();
            assert!(!result.is_ok());
            assert_eq!(
                result.timed_out_phases(),
                vec![ShutdownPhase::StopRequests],
                "local 段超时须合并进该阶段结果"
            );
            assert_eq!(
                RAN.load(Ordering::SeqCst),
                1,
                "仅窗口耗尽前的首个 local 钩子执行"
            );
            // 软限制语义：超时跳过的剩余 local 钩子随 drain 丢弃，不遗留——
            // 与 Send 段超时的处置一致。
            assert_eq!(
                coord.pending_local_hook_count().unwrap(),
                0,
                "超时丢弃的 local 钩子不应遗留"
            );
        });
    }

    #[test]
    fn shutdown_local_local_segment_gets_fresh_window_after_send_timeout() {
        static RAN: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord
                .set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_millis(30))
                .unwrap();
            coord
                .register_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        // Send 段：耗尽第一份窗口
                        std::thread::sleep(Duration::from_millis(60));
                        RAN.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        RAN.fetch_add(1, Ordering::SeqCst); // Send 段被超时跳过
                    })
                })
                .unwrap();
            coord
                .register_local_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        // local 段：窗口已独立重置，首钩子启动前检查通过
                        std::thread::sleep(Duration::from_millis(60));
                        RAN.fetch_add(1, Ordering::SeqCst);
                    })
                })
                .unwrap();
            coord
                .register_local_hook(ShutdownPhase::DrainQueue, || {
                    Box::pin(async {
                        RAN.fetch_add(1, Ordering::SeqCst); // local 段在其窗口内被跳过
                    })
                })
                .unwrap();

            let result = coord.shutdown_local().await.unwrap();
            assert!(!result.is_ok());
            assert_eq!(
                result.timed_out_phases(),
                vec![ShutdownPhase::DrainQueue],
                "两段均超时，合并后仍标记该阶段"
            );
            assert_eq!(
                RAN.load(Ordering::SeqCst),
                2,
                "Send 段与 local 段各执行首个钩子：local 段拿到全新窗口"
            );
            // 单阶段最坏耗时 ≈ 2×阶段超时（两段各用满一个窗口）——
            // 锁定"独立重置窗口"的声明，防止未来实现无意收紧或放宽。
            let drain = result
                .phases
                .iter()
                .find(|p| p.phase == ShutdownPhase::DrainQueue)
                .expect("DrainQueue phase result");
            assert!(
                drain.elapsed >= Duration::from_millis(100),
                "两段各耗满窗口（60ms+60ms），elapsed 应 ≥100ms，实际 {:?}",
                drain.elapsed
            );
        });
    }

    #[test]
    fn pending_local_hook_count_returns_err_on_poisoned_lock() {
        let coord = AsyncShutdownCoordinator::new();
        let phases = Arc::clone(&coord.phases);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = phases.write().unwrap();
            panic!("poison the phases lock");
        }));

        let err = coord
            .pending_local_hook_count()
            .expect_err("锁中毒时探测必须返回 Err，不得击穿宿主监控线程");
        assert!(
            err.to_string().contains("pending_local_hook_count"),
            "错误须可定位到探测操作本身，实际: {err}"
        );
    }

    // 毒化协调器锁需要持有私有 `phases` 写锁后 panic——只能在本模块
    // （而非 async_kit.rs 的测试）完成，故桥接失败语义的测试也在此。
    #[test]
    #[cfg(feature = "lifecycle")]
    fn register_shutdown_into_reports_stranded_count_on_bridge_failure() {
        use crate::core::lifecycle::AsyncLifecycle;
        use crate::core::{AsyncAutoBuilder, ModuleMeta};
        use crate::kit::AsyncKit;
        use std::any::TypeId;
        use std::error::Error as _;

        macro_rules! bridge_module {
            ($name:ident) => {
                struct $name;
                impl ModuleMeta for $name {
                    const NAME: &'static str = stringify!($name);
                    fn dependencies() -> &'static [(&'static str, TypeId)] {
                        &[]
                    }
                }
                impl AsyncAutoBuilder for $name {
                    type Capability = Arc<()>;
                    type Error = TraitKitError;

                    fn build<'a>(
                        _kit: &'a AsyncKit,
                    ) -> Pin<
                        Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>,
                    > {
                        Box::pin(async { Ok(Arc::new(())) })
                    }
                }
                impl AsyncLifecycle for $name {}
            };
        }
        bridge_module!(BridgeFirst);
        bridge_module!(BridgeSecond);

        let mut kit = AsyncKit::new();
        kit.register::<BridgeFirst>().unwrap();
        kit.register_lifecycle::<BridgeFirst>();
        kit.register::<BridgeSecond>().unwrap();
        kit.register_lifecycle::<BridgeSecond>();
        let built = block_on(kit.build()).expect("build should succeed");

        let coord = AsyncShutdownCoordinator::new();
        let phases = Arc::clone(&coord.phases);
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let _guard = phases.write().unwrap();
            panic!("poison the phases lock");
        }));

        let err = built
            .register_shutdown_into(&coord, ShutdownPhase::CloseConnections)
            .expect_err("桥接失败必须返回 Err");
        let msg = err.to_string();
        assert!(
            msg.contains("1 remaining lifecycle hook(s) stranded"),
            "错误须携带 stranded 计数，使程序化检测不依赖 log subscriber，实际: {msg}"
        );
        let source_msg = err
            .source()
            .map(std::string::ToString::to_string)
            .expect("包装错误必须保留协调器拒绝的原始错误为 source");
        assert!(
            source_msg.contains("RwLock poisoned"),
            "source 链须可追溯到锁中毒根因，实际: {source_msg}"
        );
    }
}

// ─── into_result hook-failure branch ───────────────────────────────────────

#[cfg(test)]
mod into_result_failures_tests {
    use super::*;
    use std::time::Duration;

    /// 只有 hook panic（无超时阶段）时 `into_result` 返回 `BuildFailed`，
    /// 错误信息携带被隔离的 hook 数量。
    #[test]
    fn shutdown_result_into_result_reports_isolated_hook_failures() {
        let result = ShutdownResult {
            phases: vec![
                ShutdownPhaseResult {
                    phase: ShutdownPhase::StopRequests,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                    hook_failures: 2,
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::DrainQueue,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                    hook_failures: 1,
                },
            ],
        };
        assert!(!result.is_ok());
        assert!(result.timed_out_phases().is_empty());
        let err = result.into_result().unwrap_err();
        let TraitKitError::BuildFailed { context, source } = &err else {
            panic!("expected BuildFailed, got {err:?}")
        };
        assert_eq!(context, "shutdown");
        let msg = format!("{source}");
        assert!(
            msg.contains('3'),
            "failure count should appear in the message: {msg}"
        );
    }
}

// ─── async global deadline skips remaining phases at the boundary ─────────

#[cfg(all(test, feature = "async"))]
mod async_deadline_tests {
    use super::*;
    use crate::test_helpers::block_on;

    /// 全局预算在阶段边界耗尽时，剩余阶段整体跳过（`timed_out`、
    /// 零耗时、零 hook 执行）。
    #[test]
    fn async_zero_global_timeout_skips_all_phases() {
        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord.set_global_timeout(Duration::ZERO).unwrap();

            let result = coord.shutdown().await.unwrap();
            assert_eq!(
                result.timed_out_phases(),
                vec![
                    ShutdownPhase::StopRequests,
                    ShutdownPhase::DrainQueue,
                    ShutdownPhase::CloseConnections
                ],
                "every phase past the exhausted budget is skipped"
            );
            for phase in &result.phases {
                assert_eq!(phase.elapsed, Duration::ZERO, "skipped phases cost 0");
                assert_eq!(phase.hook_failures, 0);
            }
        });
    }
}

// ─── shutdown_local: local hooks + exhausted global budget ──────────────

#[cfg(all(test, feature = "async"))]
mod async_shutdown_local_tests {
    use super::*;
    use crate::test_helpers::block_on;
    use std::sync::{Arc, Mutex, RwLock};

    /// 持有 `RwLock` 读卫的 `!Send` future：证明 local 槽位不要求 `Send`。
    struct LocalGuardFuture {
        _guard: Arc<RwLock<u32>>,
        log: Arc<Mutex<Vec<&'static str>>>,
    }
    impl Future for LocalGuardFuture {
        type Output = ();
        fn poll(self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>) -> Poll<()> {
            self.log.lock().unwrap().push("local");
            Poll::Ready(())
        }
    }

    #[test]
    fn local_hooks_run_after_send_hooks_in_registration_order() {
        let order: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let guard = Arc::new(RwLock::new(7u32));

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            let first = Arc::clone(&order);
            coord
                .register_hook(ShutdownPhase::DrainQueue, move || {
                    let log = Arc::clone(&first);
                    Box::pin(async move {
                        log.lock().unwrap().push("send-1");
                    })
                })
                .unwrap();
            let second = Arc::clone(&order);
            coord
                .register_hook(ShutdownPhase::DrainQueue, move || {
                    let log = Arc::clone(&second);
                    Box::pin(async move {
                        log.lock().unwrap().push("send-2");
                    })
                })
                .unwrap();
            let g = Arc::clone(&guard);
            let third = Arc::clone(&order);
            coord
                .register_local_hook(ShutdownPhase::DrainQueue, move || {
                    Box::pin(LocalGuardFuture {
                        _guard: g,
                        log: third,
                    })
                })
                .unwrap();
            assert_eq!(coord.pending_local_hook_count().unwrap(), 1);

            let result = coord.shutdown_local().await.unwrap();
            assert!(result.is_ok(), "no timeouts, no failures: {result:?}");
            assert_eq!(coord.pending_local_hook_count().unwrap(), 0, "drained");
        });

        assert_eq!(
            order.lock().unwrap().clone(),
            vec!["send-1", "send-2", "local"],
            "send hooks in registration order, then the local slot"
        );
    }

    #[test]
    fn shutdown_local_with_exhausted_budget_skips_every_phase() {
        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord.set_global_timeout(Duration::ZERO).unwrap();

            let result = coord.shutdown_local().await.unwrap();
            assert!(
                result.phases.iter().all(|p| p.timed_out),
                "global budget exhausted at every phase boundary: {result:?}"
            );
            assert!(result.phases.iter().all(|p| p.elapsed == Duration::ZERO));
        });
    }
}
