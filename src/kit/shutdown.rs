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

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;
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
                };
            }
            hook();
        }

        ShutdownPhaseResult {
            phase,
            timed_out: false,
            elapsed: start.elapsed(),
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
}

impl Default for ShutdownPhaseResult {
    /// 默认值：首阶段、未超时、零耗时（`ShutdownPhase` 为枚举，无 `Default`，
    /// 故手写实现而非 derive）。
    fn default() -> Self {
        Self {
            phase: ShutdownPhase::StopRequests,
            timed_out: false,
            elapsed: Duration::ZERO,
        }
    }
}

impl ShutdownPhaseResult {
    /// 检查该阶段是否正常完成。
    #[must_use]
    pub fn is_ok(&self) -> bool {
        !self.timed_out
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

    /// 转换为 `TraitKitResult`。若有任何阶段超时，返回 `ShutdownTimedOut` 错误。
    ///
    /// # Errors
    ///
    /// 当任何阶段超时时返回 `TraitKitError::ShutdownTimedOut`。
    pub fn into_result(self) -> Result<Self, TraitKitError> {
        if self.is_ok() {
            Ok(self)
        } else {
            Err(TraitKitError::ShutdownTimedOut {
                phases: self.timed_out_phases(),
            })
        }
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

/// 异步阶段的配置。
#[cfg(feature = "async")]
struct AsyncPhaseConfig {
    hooks: Vec<AsyncShutdownHook>,
    timeout: Duration,
}

#[cfg(feature = "async")]
impl AsyncPhaseConfig {
    fn new(timeout: Duration) -> Self {
        Self {
            hooks: Vec::new(),
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

    /// 执行完整的异步分阶段关闭流程。
    ///
    /// 全局超时在阶段边界与每个钩子启动前检查（软限制，粒度为"hook 之间"，
    /// 见模块文档），超预算则跳过剩余钩子并标记该阶段 `timed_out`。
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
                });
            }
            hook().await;
        }

        Ok(ShutdownPhaseResult {
            phase,
            timed_out: false,
            elapsed: start.elapsed(),
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
    /// 手写 `Debug`：打印结构名、阶段数与各阶段/全局超时概要。
    /// 不打印 hooks——闭包不可 `Debug`。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let phases = self.phases.read().expect("lock poisoned");
        let timeouts: [Duration; 3] = [phases[0].timeout, phases[1].timeout, phases[2].timeout];
        f.debug_struct("AsyncShutdownCoordinator")
            .field("phase_count", &phases.len())
            .field("phase_timeouts", &timeouts)
            .field(
                "global_timeout",
                &*self.global_timeout.read().expect("lock poisoned"),
            )
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
        };
        let b = ShutdownPhaseResult {
            phase: ShutdownPhase::DrainQueue,
            timed_out: true,
            elapsed: Duration::from_millis(5),
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
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::DrainQueue,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::CloseConnections,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
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
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::DrainQueue,
                    timed_out: true,
                    elapsed: Duration::from_secs(30),
                },
                ShutdownPhaseResult {
                    phase: ShutdownPhase::CloseConnections,
                    timed_out: false,
                    elapsed: Duration::from_millis(1),
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
}
