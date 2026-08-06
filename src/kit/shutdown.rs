// Copyright (c) 2026 Kirky.X
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

#[cfg(feature = "async")]
use std::future::Future;
#[cfg(feature = "async")]
use std::pin::Pin;
use std::time::{Duration, Instant};

use std::cell::RefCell;
#[cfg(feature = "async")]
use std::sync::{Arc, RwLock};

use crate::error::TraitKitError;

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
        let idx = Self::phase_index(phase);
        self.phases.borrow_mut()[idx].timeout = timeout;
    }

    /// 注册一个关闭钩子到指定阶段。
    ///
    /// 同一阶段可注册多个钩子，按注册顺序执行。
    pub fn register_hook<F>(&self, phase: ShutdownPhase, hook: F)
    where
        F: FnOnce() + 'static,
    {
        let idx = Self::phase_index(phase);
        self.phases.borrow_mut()[idx].hooks.push(Box::new(hook));
    }

    /// 执行完整的分阶段关闭流程。
    ///
    /// 按 `StopRequests → DrainQueue → CloseConnections` 顺序执行。
    /// 每个阶段内的钩子按注册顺序执行。
    /// 单阶段超时后跳过剩余钩子，进入下一阶段。
    ///
    /// 返回每个阶段的执行结果（是否超时）。
    #[must_use = "shutdown returns phase results; ignoring it may hide timeout events"]
    pub fn shutdown(&self) -> Vec<ShutdownPhaseResult> {
        let global_start = Instant::now();
        let global_timeout = *self.global_timeout.borrow();
        let mut results = Vec::with_capacity(3);

        for phase in ShutdownPhase::all_phases() {
            // 检查全局超时
            if let Some(gt) = global_timeout
                && global_start.elapsed() >= gt
            {
                results.push(ShutdownPhaseResult {
                    phase: *phase,
                    timed_out: true,
                    elapsed: global_start.elapsed(),
                });
                continue;
            }

            let result = self.execute_phase(*phase);
            results.push(result);
        }

        results
    }

    /// 执行单个阶段的所有钩子。
    fn execute_phase(&self, phase: ShutdownPhase) -> ShutdownPhaseResult {
        let idx = Self::phase_index(phase);
        let start = Instant::now();

        // 取出钩子（drain 避免重复执行）
        let hooks: Vec<SyncShutdownHook> = {
            let mut phases = self.phases.borrow_mut();
            std::mem::take(&mut phases[idx].hooks)
        };
        let timeout = self.phases.borrow()[idx].timeout;

        for hook in hooks {
            if start.elapsed() >= timeout {
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

    /// 将阶段映射到数组索引。
    const fn phase_index(phase: ShutdownPhase) -> usize {
        match phase {
            ShutdownPhase::StopRequests => 0,
            ShutdownPhase::DrainQueue => 1,
            ShutdownPhase::CloseConnections => 2,
        }
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// 单个阶段的关闭结果。
#[derive(Debug, Clone)]
pub struct ShutdownPhaseResult {
    /// 执行的阶段。
    pub phase: ShutdownPhase,
    /// 是否因超时跳过。
    pub timed_out: bool,
    /// 该阶段实际耗时。
    pub elapsed: Duration,
}

impl ShutdownPhaseResult {
    /// 检查该阶段是否正常完成。
    #[must_use]
    pub fn is_ok(&self) -> bool {
        !self.timed_out
    }
}

/// 关闭流程整体结果。
#[derive(Debug, Clone)]
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
    /// # Panics
    ///
    /// 当内部 `RwLock` 中毒时 panic。
    pub fn set_global_timeout(&self, timeout: Duration) {
        *self.global_timeout.write().expect("lock poisoned") = Some(timeout);
    }

    /// 设置指定阶段的超时。
    ///
    /// # Panics
    ///
    /// 当内部 `RwLock` 中毒时 panic。
    pub fn set_phase_timeout(&self, phase: ShutdownPhase, timeout: Duration) {
        let idx = Self::phase_index(phase);
        self.phases.write().expect("lock poisoned")[idx].timeout = timeout;
    }

    /// 注册一个异步关闭钩子到指定阶段。
    ///
    /// # Errors
    ///
    /// 当内部锁中毒时返回 `TraitKitError::BuildFailed`。
    ///
    /// # Panics
    ///
    /// 当数组索引越界时 panic（不会发生，索引由枚举映射保证）。
    pub fn register_hook<F, Fut>(&self, phase: ShutdownPhase, hook: F) -> Result<(), TraitKitError>
    where
        F: FnOnce() -> Pin<Box<Fut>> + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let idx = Self::phase_index(phase);
        // 将 hook 包装为 AsyncShutdownHook（erased future）
        let boxed: AsyncShutdownHook =
            Box::new(move || -> Pin<Box<dyn Future<Output = ()> + Send>> {
                let fut = hook();
                Box::pin(fut) as Pin<Box<dyn Future<Output = ()> + Send>>
            });
        self.phases
            .write()
            .map_err(|_| TraitKitError::BuildFailed {
                context: format!("shutdown phase `{}`", phase.as_str()),
                source: Box::new(std::io::Error::other("RwLock poisoned")),
            })?
            .get_mut(idx)
            .expect("index in range")
            .hooks
            .push(boxed);
        Ok(())
    }

    /// 执行完整的异步分阶段关闭流程。
    ///
    /// # Panics
    ///
    /// 当内部 `RwLock` 中毒时 panic。
    #[must_use = "shutdown returns phase result; ignoring it may hide timeout events"]
    pub async fn shutdown(&self) -> ShutdownResult {
        let global_start = Instant::now();
        let global_timeout = *self.global_timeout.read().expect("lock poisoned");
        let mut results = Vec::with_capacity(3);

        for phase in ShutdownPhase::all_phases() {
            // 检查全局超时
            if let Some(gt) = global_timeout
                && global_start.elapsed() >= gt
            {
                results.push(ShutdownPhaseResult {
                    phase: *phase,
                    timed_out: true,
                    elapsed: global_start.elapsed(),
                });
                continue;
            }

            let result = self.execute_phase(*phase).await;
            results.push(result);
        }

        ShutdownResult { phases: results }
    }

    /// 执行单个异步阶段。
    async fn execute_phase(&self, phase: ShutdownPhase) -> ShutdownPhaseResult {
        let idx = Self::phase_index(phase);
        let start = Instant::now();

        // 取出钩子
        let hooks: Vec<AsyncShutdownHook> = {
            let mut phases = self.phases.write().expect("lock poisoned");
            std::mem::take(&mut phases[idx].hooks)
        };
        let timeout = self.phases.read().expect("lock poisoned")[idx].timeout;

        for hook in hooks {
            if start.elapsed() >= timeout {
                return ShutdownPhaseResult {
                    phase,
                    timed_out: true,
                    elapsed: start.elapsed(),
                };
            }
            hook().await;
        }

        ShutdownPhaseResult {
            phase,
            timed_out: false,
            elapsed: start.elapsed(),
        }
    }

    /// 将阶段映射到数组索引。
    const fn phase_index(phase: ShutdownPhase) -> usize {
        match phase {
            ShutdownPhase::StopRequests => 0,
            ShutdownPhase::DrainQueue => 1,
            ShutdownPhase::CloseConnections => 2,
        }
    }
}

#[cfg(feature = "async")]
impl Default for AsyncShutdownCoordinator {
    fn default() -> Self {
        Self::new()
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

        let results = coord.shutdown();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));
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

        coord.shutdown();

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

        let results = coord.shutdown();
        assert_eq!(results.len(), 3);

        // DrainQueue 应该超时
        assert!(results[0].is_ok()); // StopRequests
        assert!(results[1].timed_out); // DrainQueue
        assert!(results[2].is_ok()); // CloseConnections

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

        let results = coord.shutdown();
        assert_eq!(results.len(), 3);
        // StopRequests 执行完（phase timeout 30s 足够），但全局超时导致后续阶段被跳过
        // StopRequests 钩子执行后 CALLED=1，全局超时已超，后续阶段被跳过
        assert_eq!(CALLED.load(Ordering::SeqCst), 1);
        assert!(results[0].is_ok()); // StopRequests 正常完成
        assert!(results[1].timed_out); // DrainQueue 被全局超时跳过
        assert!(results[2].timed_out); // CloseConnections 被全局超时跳过
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
        let results = coord.shutdown();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn shutdown_coordinator_empty_phases_succeed() {
        let coord = ShutdownCoordinator::new();
        let results = coord.shutdown();
        assert_eq!(results.len(), 3);
        for r in &results {
            assert!(r.is_ok());
            assert!(
                r.elapsed.as_nanos() < 1_000_000,
                "empty phase should be near-instant"
            );
        }
    }

    #[test]
    fn shutdown_coordinator_hooks_not_reentrant() {
        static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

        let coord = ShutdownCoordinator::new();
        coord.register_hook(ShutdownPhase::StopRequests, || {
            CALL_COUNT.fetch_add(1, Ordering::SeqCst);
        });

        // 第一次 shutdown
        coord.shutdown();
        assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);

        // 第二次 shutdown — 钩子已被 drain，不应再执行
        coord.shutdown();
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

            let result = coord.shutdown().await;
            assert!(result.is_ok());
            assert_eq!(CALLED.load(Ordering::SeqCst), 3);
        });
    }

    #[test]
    fn async_shutdown_coordinator_timeout() {
        static CALLED: AtomicUsize = AtomicUsize::new(0);

        block_on(async {
            let coord = AsyncShutdownCoordinator::new();
            coord.set_phase_timeout(ShutdownPhase::DrainQueue, Duration::from_millis(10));

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

            let result = coord.shutdown().await;
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
            let result = coord.shutdown().await;
            assert!(result.is_ok());
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

            coord.shutdown().await;
            assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);

            coord.shutdown().await;
            assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
        });
    }
}
