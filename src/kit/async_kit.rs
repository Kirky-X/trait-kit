// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! `AsyncKit` — the async capability and configuration management center.
//!
//! Typestate `AsyncKit<Unbuilt>` → `AsyncKit<Ready>` with `Arc<RwLock>`
//! interior mutability (multi-threaded, `Send + Sync`). A parallel of the
//! synchronous [`super::kit::Kit`] — same typestate flow (register, config,
//! build, require, lifecycle, health, factory, decorator) with `RwLock`
//! instead of `RefCell` and async build functions returning
//! `Pin<Box<dyn Future + Send>>`. Sync-only surfaces (lazy, multi-binding,
//! interface, override, toggle, reload, encryption, snapshot) live on `Kit`.
//!
//! # Sync/Async 行为对照清单
//! 修改任一侧时同步核对另一侧（sync 侧为 `kit.rs`）：
//! 1. build：模块按拓扑序构建；`on_ready` 回调按拓扑序执行。
//! 2. `register_lifecycle`：幂等——重复注册同一模块为 no-op（未注册模块的
//!    钩子：sync 侧静默跳过 / async 侧垫底执行——两侧均应在注册后再
//!    `register_lifecycle`）。
//! 3. lazy require：n/a（async 侧没有 lazy 槽位，构建失败即返回
//!    `BuildFailed`；"builder 放回槽位可重试"仅存在于 sync 侧 `Kit`）。
//! 4. `shutdown_async()`：async `on_shutdown` 钩子 drain（one-shot，二次调用
//!    no-op）且按逆拓扑序执行（依赖者先于被依赖者）；`AsyncKit` 无 sync
//!    `shutdown()`——async 清理必须 await `shutdown_async()`。
//! 5. decorator：按 `decorator_module_to_cap` 映射应用（async 侧全部在
//!    `build()` 时应用，无 lazy 路径）。
//! 6. factory：typestate cast + 编译期 size/align 布局断言。

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use crate::core::AsyncAutoBuilder;
use crate::error::TraitKitError;

use super::AsyncTypeMap;

use super::{DependencyGraph, GraphError, ModuleEntry};

#[cfg(feature = "lifecycle")]
type AsyncShutdownHookFn =
    Box<dyn for<'a> Fn(&'a AsyncTypeMap) -> Pin<Box<dyn Future<Output = ()> + 'a>> + Send + Sync>;
#[cfg(feature = "lifecycle")]
type AsyncReadyCallback = Box<
    dyn for<'a> Fn(
            &'a AsyncKit<Ready>,
        ) -> Pin<Box<dyn Future<Output = Result<(), TraitKitError>> + Send + 'a>>
        + Send
        + Sync,
>;
#[cfg(feature = "health")]
type AsyncHealthCheckerFn =
    Arc<dyn Fn(&AsyncTypeMap) -> crate::core::health::HealthStatus + Send + Sync>;
#[cfg(feature = "observer")]
type AsyncObserverRef = Arc<dyn crate::core::observer::BuildObserver>;
#[cfg(feature = "decorator")]
type AsyncDecoratorFn =
    Box<dyn Fn(Box<dyn Any + Send + Sync>) -> Box<dyn Any + Send + Sync> + Send + Sync>;

// ─── Feature-gated field groups ────────────────────────────────────────────
//
// 按 feature 聚合的字段子结构：新增 feature 时扩展对应子结构即可，不要在
// `AsyncKit` 上平铺新字段（否则字段声明、`new()`、`build()` 搬移等多个位点
// 都要逐字段散弹式修改）。各子结构内部字段类型与聚合前完全一致，外部行为
// 零变化；`Arc<RwLock<T: Default>>` 的空态由 `derive(Default)` 提供，
// `AsyncKit::new()` 直接以 `XxxFields::default()` 构造。

/// Fields gated behind the `lifecycle` feature.
#[cfg(feature = "lifecycle")]
#[derive(Default)]
struct LifecycleFields {
    async_shutdown_callbacks: Arc<RwLock<Vec<(TypeId, AsyncShutdownHookFn)>>>,
    ready_callbacks: Arc<RwLock<Vec<(TypeId, AsyncReadyCallback)>>>,
}

/// Fields gated behind the `health` feature.
#[cfg(feature = "health")]
#[derive(Default)]
struct HealthFields {
    health_checkers: Arc<RwLock<HashMap<TypeId, (&'static str, AsyncHealthCheckerFn)>>>,
}

/// Fields gated behind the `observer` feature.
#[cfg(feature = "observer")]
#[derive(Default)]
struct ObserverFields {
    observers: Arc<RwLock<Vec<AsyncObserverRef>>>,
}

/// Fields gated behind the `decorator` feature.
#[cfg(feature = "decorator")]
#[derive(Default)]
struct DecoratorFields {
    decorators: Arc<RwLock<HashMap<TypeId, Vec<AsyncDecoratorFn>>>>,
    decorator_module_to_cap: Arc<RwLock<HashMap<TypeId, TypeId>>>,
}

/// Fields gated behind the `confers` feature.
#[cfg(feature = "confers")]
#[derive(Default)]
struct ConfersFields {
    /// Shared field overlay for cross-type config inheritance (async counterpart).
    /// Values are `serde_json::Value` to preserve type information.
    shared_fields: Arc<RwLock<serde_json::Map<String, serde_json::Value>>>,
    /// Config snapshots for save/restore (async counterpart of Kit's `RefCell<HashMap>`).
    config_snapshots: Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

/// 已登记的 reload 回调（线程安全闭包）
type ReloadSubscriber = Arc<dyn Fn() + Send + Sync>;

/// Fields gated behind the `reload` feature (async counterpart).
#[cfg(feature = "reload")]
#[derive(Default)]
struct ReloadFields {
    /// Thread-safe subscriber map: `Arc<dyn Fn() + Send + Sync>` instead of `Rc<dyn Fn()>`.
    subscribers: Arc<RwLock<HashMap<TypeId, Vec<ReloadSubscriber>>>>,
}

/// Fields gated behind the `encryption` feature (async counterpart).
#[cfg(feature = "encryption")]
#[derive(Default)]
struct EncryptionFields {
    encrypted_configs: Arc<RwLock<HashMap<TypeId, super::EncryptedBlob>>>,
}

/// Fields for observation ports (always present, no feature gate).
#[derive(Default)]
struct PortsFields {
    metrics_port: Arc<RwLock<super::ports::OptionalMetricsPort>>,
    log_port: Arc<RwLock<super::ports::OptionalLogPort>>,
    event_bus: Arc<RwLock<super::events::OptionalEventBus>>,
}

/// Marker type for the unbuilt state.
pub struct Unbuilt;

/// Marker type for the ready (built) state.
pub struct Ready;

/// Type-erased async build function.
///
/// Stored in the dependency graph and called during `AsyncKit::build()` to
/// produce a boxed capability. The returned future borrows the kit for
/// lifetime `'a` (higher-rank), allowing build callbacks to read configs /
/// require dependencies from the kit during async construction without forcing
/// a `'static` capture.
///
/// The future yields `Box<dyn Any + Send + Sync>` (not just `+ Send`) because
/// `AsyncTypeMap::insert_boxed` requires `Send + Sync` storage and the
/// capability trait bound `AsyncAutoBuilder::Capability: Send + Sync + 'static`
/// guarantees both.
///
/// The error variant is `Box<dyn Error + Send + 'static>` to match
/// `TraitKitError::BuildFailed::source` (which is `Send` so that `TraitKitError: Send`
/// and `tokio::spawn(async move { kit.build().await })` compiles on a
/// multi-threaded runtime). The future is `Send` because both
/// `Box<dyn Any + Send + Sync>` and `Box<dyn Error + Send + 'static>` are
/// `Send`.
#[allow(
    clippy::type_complexity,
    reason = "Pin<Box<dyn Future + Send>> is the canonical dyn-compatible async dispatch type; mirrors AsyncAutoBuilder::build"
)]
pub(crate) type AsyncBuildFn = Box<
    dyn for<'a> FnOnce(
            &'a AsyncKit,
        ) -> Pin<
            Box<
                dyn Future<
                        Output = Result<
                            Box<dyn Any + Send + Sync>,
                            Box<dyn std::error::Error + Send + 'static>,
                        >,
                    > + Send
                    + 'a,
            >,
        > + Send
        + Sync,
>;

/// A queued module-build future (borrowed from the kit for the layer's drive).
type AsyncBuildFut<'a> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    Box<dyn Any + Send + Sync>,
                    Box<dyn std::error::Error + Send + 'static>,
                >,
            > + Send
            + 'a,
    >,
>;

/// Dependency-free concurrency-limited join driver.
///
/// Polls at most `limit` queued futures at a time; as futures complete, new
/// ones are admitted from the queue. Children are polled with the *caller's*
/// context, so waking is correct on any executor. Output is in completion
/// order.
struct BatchJoin<T, F: Future> {
    queued: std::collections::VecDeque<(T, F)>,
    active: Vec<Option<(T, F)>>,
    limit: usize,
    completed: Vec<(T, F::Output)>,
}

impl<T: Copy + Unpin, F: Future + Unpin> Future for BatchJoin<T, F>
where
    F::Output: Unpin,
{
    type Output = Vec<(T, F::Output)>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        loop {
            // Admit queued futures into free active slots.
            while this.active.len() < this.limit {
                match this.queued.pop_front() {
                    Some((t, f)) => this.active.push(Some((t, f))),
                    None => break,
                }
            }
            if this.active.is_empty() {
                return Poll::Ready(std::mem::take(&mut this.completed));
            }

            let mut completed_any = false;
            let mut idx = 0;
            while idx < this.active.len() {
                let Some(slot) = this.active[idx].as_mut() else {
                    idx += 1;
                    continue;
                };
                let (_t, f) = slot;
                match Pin::new(f).poll(cx) {
                    Poll::Ready(out) => {
                        let (slot_t, _) = this.active.remove(idx).expect("slot occupied");
                        this.completed.push((slot_t, out));
                        completed_any = true;
                        // Do not advance: elements shifted left.
                    }
                    Poll::Pending => {
                        idx += 1;
                    }
                }
            }

            if !completed_any {
                return Poll::Pending;
            }
            // Some futures completed: loop to admit newly queued ones.
        }
    }
}

/// 单个待构建项：`(模块标识, 构建期起点)` 与其构建 future 的配对
type QueuedBuild<'a> = (
    (TypeId, &'static str, Option<std::time::Instant>),
    AsyncBuildFut<'a>,
);

/// Split a validated topological order into dependency levels.
///
/// Level 0 = modules without dependencies; level N = modules whose longest
/// dependency chain has N+1 nodes. Within a level no module depends on
/// another, so they can build concurrently.
fn topo_layers(graph: &DependencyGraph, sorted: &[TypeId]) -> Vec<Vec<TypeId>> {
    let mut level_of: HashMap<TypeId, usize> = HashMap::new();
    for id in sorted {
        let level = graph
            .entries()
            .iter()
            .find(|entry| entry.type_id == *id)
            .map_or(0, |entry| {
                entry
                    .dependencies
                    .iter()
                    .filter_map(|(_, dep)| level_of.get(dep).copied())
                    .max()
                    .unwrap_or(0)
                    + 1
            });
        level_of.insert(*id, level);
    }
    let mut buckets: Vec<Vec<TypeId>> = Vec::new();
    for id in sorted {
        let level = level_of[id];
        if buckets.len() <= level {
            buckets.resize(level + 1, Vec::new());
        }
        buckets[level].push(*id);
    }
    buckets
}

/// The async capability and configuration management center.
///
/// Multi-threaded (`Send + Sync`) counterpart to [`super::kit::Kit`]. Uses
/// `Arc<RwLock<...>>` for interior mutability (safe to share across threads,
/// poisoning-aware). Async module construction happens in `build()`.
pub struct AsyncKit<S = Unbuilt> {
    builders: Arc<RwLock<HashMap<TypeId, AsyncBuildFn>>>,
    graph: DependencyGraph,
    configs: AsyncTypeMap,
    capabilities: AsyncTypeMap,
    #[cfg(feature = "lifecycle")]
    lifecycle: LifecycleFields,
    #[cfg(feature = "health")]
    health: HealthFields,
    #[cfg(feature = "observer")]
    observer: ObserverFields,
    #[cfg(feature = "decorator")]
    decorator: DecoratorFields,
    #[cfg(feature = "confers")]
    confers: ConfersFields,
    #[cfg(feature = "reload")]
    reload: ReloadFields,
    #[cfg(feature = "encryption")]
    encryption: EncryptionFields,
    ports: PortsFields,
    /// Max concurrently-polled module builds per topological layer.
    max_concurrency: usize,
    _state: PhantomData<S>,
}

impl AsyncKit {
    /// Create a new empty `AsyncKit<Unbuilt>`.
    ///
    /// All containers (`builders`, `graph`, `configs`, `capabilities`) start
    /// empty; register modules and configs before calling `build()`.
    #[must_use]
    pub fn new() -> Self {
        AsyncKit {
            builders: Arc::new(RwLock::new(HashMap::new())),
            graph: DependencyGraph::new(),
            configs: AsyncTypeMap::new(),
            capabilities: AsyncTypeMap::new(),
            #[cfg(feature = "lifecycle")]
            lifecycle: LifecycleFields::default(),
            #[cfg(feature = "health")]
            health: HealthFields::default(),
            #[cfg(feature = "observer")]
            observer: ObserverFields::default(),
            #[cfg(feature = "decorator")]
            decorator: DecoratorFields::default(),
            #[cfg(feature = "confers")]
            confers: ConfersFields::default(),
            #[cfg(feature = "reload")]
            reload: ReloadFields::default(),
            #[cfg(feature = "encryption")]
            encryption: EncryptionFields::default(),
            ports: PortsFields::default(),
            max_concurrency: usize::MAX,
            _state: PhantomData,
        }
    }

    /// Register a module for async construction.
    ///
    /// The module's [`AsyncAutoBuilder::build`] is stored as a type-erased
    /// `AsyncBuildFn` and invoked during `build()`. Registration order does
    /// not matter — `build()` resolves the construction order via the
    /// dependency graph's topological sort.
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::AlreadyRegistered`] if a module with the same
    /// `TypeId` was already registered.
    ///
    /// # Panics
    ///
    /// Panics if the `builders` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). Lock poisoning indicates a
    /// logic bug in the async build pipeline and should fail loudly.
    #[must_use = "ignoring the Result may hide AlreadyRegistered errors"]
    pub fn register<M: AsyncAutoBuilder>(&mut self) -> Result<(), TraitKitError> {
        let entry = ModuleEntry {
            type_id: TypeId::of::<M>(),
            name: M::NAME,
            dependencies: M::dependencies().iter().map(|(n, id)| (*n, *id)).collect(),
        };

        self.graph
            .add(entry)
            .map_err(|name| TraitKitError::AlreadyRegistered { module: name })?;

        let build_fn: AsyncBuildFn = Box::new(|kit| {
            Box::pin(async move {
                let cap = M::build(kit)
                    .await
                    .map_err(|e| -> Box<dyn std::error::Error + Send + 'static> { Box::new(e) })?;
                Ok(Box::new(cap) as Box<dyn Any + Send + Sync>)
            })
        });
        self.builders
            .write()
            .expect(
                "AsyncKit builders lock poisoned: another thread panicked while holding the lock",
            )
            .insert(TypeId::of::<M>(), build_fn);
        Ok(())
    }

    /// Set a configuration value.
    ///
    /// Overwrites any prior value of the same type. Configs are read during
    /// `build()` via [`AsyncKit::config`] inside module `build` callbacks.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). See [`register`](Self::register)
    /// for context on lock poisoning.
    pub fn set_config<C: Clone + Send + Sync + 'static>(&self, config: C) {
        let replaced = self.configs.contains::<C>();
        self.configs.insert(config);
        // config-change audit event to the injected bus (no-op default).
        if let Some(bus) = self.ports.event_bus.read().expect("lock poisoned").as_ref() {
            bus.publish(super::events::KitEvent::ConfigChanged {
                key: std::any::type_name::<C>().to_string(),
                summary: if replaced {
                    "set (replaced)".to_string()
                } else {
                    "set (new)".to_string()
                },
            });
        }
    }

    /// Store a configuration value behind an `Arc` for zero-clone reads.
    ///
    /// Stored under `TypeId::of::<Arc<C>>` — a distinct slot from plain
    /// `set_config`. Read back with `config_arc::<C>()`.
    ///
    /// Publishes the same `KitEvent::ConfigChanged` audit event as
    /// `set_config`, so event subscribers see Arc-slot updates too.
    ///
    /// # Panics
    ///
    /// Panics if the event bus lock is poisoned.
    pub fn set_config_arc<C: Clone + Send + Sync + 'static>(&self, config: C) {
        let replaced = self.configs.contains::<std::sync::Arc<C>>();
        self.configs.insert(std::sync::Arc::new(config));
        if let Some(bus) = self.ports.event_bus.read().expect("lock poisoned").as_ref() {
            bus.publish(super::events::KitEvent::ConfigChanged {
                key: std::any::type_name::<C>().to_string(),
                summary: if replaced {
                    "set_arc (replaced)".to_string()
                } else {
                    "set_arc (new)".to_string()
                },
            });
        }
    }

    /// Read a configuration value as an `Arc` snapshot — read-side zero clone.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingConfig` if no `Arc` snapshot of `C` was set.
    pub fn config_arc<C: Clone + Send + Sync + 'static>(
        &self,
    ) -> Result<std::sync::Arc<C>, TraitKitError> {
        self.configs
            .get_cloned::<std::sync::Arc<C>>()
            .ok_or(TraitKitError::MissingConfig {
                key: std::any::type_name::<C>().to_string(),
            })
    }

    /// Validate the dependency graph and build all modules in topological
    /// order, returning an `AsyncKit<Ready>` whose capabilities are available
    /// via `require` / `optional`.
    ///
    /// Async because each module's [`AsyncAutoBuilder::build`] returns a
    /// future. Modules are constructed one at a time in dependency order;
    /// the build callback receives a `&AsyncKit` reference and may read
    /// configs (and, once prior modules are built, capabilities) from it.
    ///
    /// # Cross-Module Dependency Injection
    ///
    /// Because modules are constructed in topological order and each
    /// capability is inserted into the shared [`AsyncTypeMap`] immediately
    /// after its `build` future resolves, a module's `build` callback may
    /// call `kit.require::<DepModule>()?` to pull in the capability of any
    /// already-built dependency. This is the canonical DI pattern and works
    /// transitively (A→B→C chains). The `require` method lives in
    /// `impl<S> AsyncKit<S>` so it is available on `&AsyncKit<Unbuilt>`
    /// during `build()` as well as on `&AsyncKit<Ready>` afterwards.
    ///
    /// ```text
    /// // Inside an AsyncAutoBuilder::build callback:
    /// let dep_cap = kit.require::<DepModule>()?;  // dep was built earlier
    /// ```
    ///
    /// The kit's `capabilities` map is backed by `Arc<RwLock<...>>`, so a
    /// write is visible to subsequent `require` calls without additional
    /// synchronization. The build callback must not hold a write guard
    /// across `.await` (the build pipeline never does this).
    ///
    /// # Errors
    ///
    /// - [`TraitKitError::DependencyMissing`] if a registered module declares a
    ///   dependency that was never registered.
    /// - [`TraitKitError::CycleDetected`] if the dependency graph contains a cycle.
    /// - [`TraitKitError::MissingCapability`] if a topologically-sorted module has
    ///   no stored build function (internal invariant violation).
    /// - [`TraitKitError::BuildFailed`] if a module's `build` callback returns `Err`.
    /// - [`TraitKitError::LifecycleFailed`] if any `on_ready` callback fails. The
    ///   built `AsyncKit<Ready>` is dropped wholesale: already-built capabilities
    ///   become unreachable, and the `on_shutdown` hooks registered so far are
    ///   **not** executed (mirroring the sync `Kit::build` behavior).
    ///
    /// # Panics
    ///
    /// Panics if the `builders` [`RwLock`] is poisoned (a worker thread
    /// panicked while holding the write lock). Lock poisoning indicates a
    /// logic bug in the async build pipeline and should fail loudly.
    #[must_use = "ignoring the built kit loses all capabilities and lifecycle callbacks"]
    #[allow(clippy::too_many_lines)]
    pub async fn build(self) -> Result<AsyncKit<Ready>, TraitKitError> {
        // 1. Validate the dependency graph: missing-dep check + Kahn topo sort.
        let sorted = match self.graph.validate() {
            Ok(sorted) => sorted,
            Err(GraphError::DependencyMissing { module, missing }) => {
                return Err(TraitKitError::DependencyMissing { module, missing });
            }
            Err(GraphError::CycleDetected { cycle }) => {
                return Err(TraitKitError::CycleDetected { cycle });
            }
        };

        // 2. Extract all builders from the Arc<RwLock<…>> in a single
        //    write-lock acquisition (instead of one lock per module in the
        //    loop). The drain empties the map inside the RwLock; the Arc
        //    itself remains held by the struct for subsequent operations.
        let mut builders: HashMap<TypeId, AsyncBuildFn> = {
            let mut guard = self
                .builders
                .write()
                .expect("AsyncKit builders lock poisoned");
            guard.drain().collect()
        };

        // 3. Build modules topological-layer by topological-layer:
        //    modules whose dependencies are all satisfied (same level) run
        //    concurrently, bounded by `max_concurrency`. Within a layer,
        //    results are processed in completion order; observer callbacks
        //    fire per module as its result is processed.
        let event_bus_present = {
            self.ports
                .event_bus
                .read()
                .expect("lock poisoned")
                .is_some()
        };
        // Per-module wall timing is taken only when a consumer exists.
        #[allow(unused_mut, unused_assignments)]
        let mut timing_enabled = event_bus_present;
        #[cfg(feature = "observer")]
        {
            timing_enabled = true;
        }
        let layers = topo_layers(&self.graph, &sorted);
        for layer in layers {
            // Materialize the layer's futures up front (lazy — nothing runs
            // until the driver polls them).
            let mut queued: std::collections::VecDeque<QueuedBuild> =
                std::collections::VecDeque::new();
            for type_id in layer {
                let module_name = self.graph.name_of(type_id).unwrap_or("<unknown>");
                let build_fn =
                    builders
                        .remove(&type_id)
                        .ok_or_else(|| TraitKitError::MissingCapability {
                            key: module_name.to_string(),
                        })?;
                // Observer: notify build start when the future is queued.
                #[cfg(feature = "observer")]
                {
                    let observers = self.observer.observers.read().expect("lock poisoned");
                    for obs in observers.iter() {
                        obs.on_module_start(module_name);
                    }
                }
                let started_at = if timing_enabled {
                    Some(std::time::Instant::now())
                } else {
                    None
                };
                queued.push_back(((type_id, module_name, started_at), build_fn(&self)));
            }

            let batch = BatchJoin {
                queued,
                active: Vec::new(),
                limit: self.max_concurrency.max(1),
                completed: Vec::new(),
            };
            let results = batch.await;

            for ((type_id, module_name, started_at), outcome) in results {
                match outcome {
                    Ok(boxed) => {
                        // Apply decorators (keyed by capability TypeId)
                        #[cfg(feature = "decorator")]
                        let boxed = {
                            let cap_type_id = self
                                .decorator
                                .decorator_module_to_cap
                                .read()
                                .expect("lock poisoned")
                                .get(&type_id)
                                .copied()
                                .unwrap_or(type_id);
                            self.apply_decorators(cap_type_id, boxed)
                        };
                        self.capabilities.insert_boxed(type_id, boxed);
                        #[cfg(feature = "observer")]
                        {
                            let observers = self.observer.observers.read().expect("lock poisoned");
                            for obs in observers.iter() {
                                // Completion-order callback within the layer.
                                obs.on_module_built(
                                    module_name,
                                    started_at.map_or(std::time::Duration::ZERO, |t| t.elapsed()),
                                );
                            }
                        }
                        if event_bus_present {
                            self.publish_event(super::events::KitEvent::ModuleBuilt {
                                module: module_name,
                                elapsed_us: started_at.map_or(0, |t| {
                                    u64::try_from(t.elapsed().as_micros()).unwrap_or(u64::MAX)
                                }),
                            });
                        }
                    }
                    Err(e) => {
                        let err = TraitKitError::BuildFailed {
                            context: module_name.to_string(),
                            source: e,
                        };
                        #[cfg(feature = "observer")]
                        {
                            let observers = self.observer.observers.read().expect("lock poisoned");
                            for obs in observers.iter() {
                                obs.on_build_error(module_name, &err);
                            }
                        }
                        return Err(err);
                    }
                }
            }
        }

        // 4. Transition to Ready: reuse all containers, swap the state marker.
        //    `builders` was drained (not moved) above; the empty map is reused.
        #[cfg(feature = "lifecycle")]
        let ready_callbacks: Vec<(TypeId, AsyncReadyCallback)> = {
            self.lifecycle
                .ready_callbacks
                .write()
                .expect("lock poisoned")
                .drain(..)
                .collect()
        };

        // Sort the async shutdown hooks by topological index (stable sort):
        // the reverse-order drain in `shutdown_async` then executes them in
        // reverse topological order — dependents shut down before the
        // modules they depend on, matching the documented contract. Modules
        // absent from the dependency graph keep registration order at the
        // tail (`usize::MAX`), mirroring the on_ready sort below.
        #[cfg(feature = "lifecycle")]
        {
            let topo_index: HashMap<TypeId, usize> = sorted
                .iter()
                .enumerate()
                .map(|(idx, id)| (*id, idx))
                .collect();
            self.lifecycle
                .async_shutdown_callbacks
                .write()
                .expect("lock poisoned")
                .sort_by_key(|(type_id, _)| topo_index.get(type_id).copied().unwrap_or(usize::MAX));
        }

        let kit = AsyncKit {
            builders: self.builders,
            graph: self.graph,
            configs: self.configs,
            capabilities: self.capabilities,
            #[cfg(feature = "lifecycle")]
            lifecycle: LifecycleFields {
                async_shutdown_callbacks: self.lifecycle.async_shutdown_callbacks,
                ready_callbacks: Arc::new(RwLock::new(Vec::new())),
            },
            #[cfg(feature = "health")]
            health: self.health,
            #[cfg(feature = "observer")]
            observer: self.observer,
            #[cfg(feature = "decorator")]
            decorator: self.decorator,
            #[cfg(feature = "confers")]
            confers: self.confers,
            #[cfg(feature = "reload")]
            reload: self.reload,
            #[cfg(feature = "encryption")]
            encryption: self.encryption,
            ports: self.ports,
            max_concurrency: self.max_concurrency,
            _state: PhantomData::<Ready>,
        };

        // Call lifecycle on_ready callbacks in topological order (matching the
        // synchronous `Kit::build`). Stable sort preserves registration order
        // for modules absent from the dependency graph.
        #[cfg(feature = "lifecycle")]
        {
            let topo_index: HashMap<TypeId, usize> = sorted
                .iter()
                .enumerate()
                .map(|(idx, id)| (*id, idx))
                .collect();
            let mut on_ready: Vec<(TypeId, AsyncReadyCallback)> = ready_callbacks;
            on_ready
                .sort_by_key(|(type_id, _)| topo_index.get(type_id).copied().unwrap_or(usize::MAX));
            for (_type_id, callback) in &on_ready {
                callback(&kit).await?;
            }
        }

        Ok(kit)
    }

    // ─── Lifecycle ─────────────────────────────────────────────────────

    /// Register lifecycle hooks for an async module.
    ///
    /// Requires the `lifecycle` feature. Idempotent: registering the same
    /// module twice is a no-op.
    ///
    /// The module must first be registered via [`AsyncKit::register`]: hooks of
    /// a module that never entered the dependency graph are not ordered by
    /// dependencies — they run last (stable registration order among
    /// themselves), after every graph module. Always call `register::<M>()`
    /// before `register_lifecycle::<M>()`.
    ///
    /// Both `AsyncLifecycle::on_ready` and `AsyncLifecycle::on_shutdown` are
    /// supported: `on_ready` runs during `build()` in topological order;
    /// `on_shutdown` is registered as an async hook and executed by
    /// [`AsyncKit::shutdown_async`] in reverse topological order (dependents
    /// shut down before their dependencies) — async cleanup must await
    /// `shutdown_async()`. If `on_ready` fails during `build()`, the built kit
    /// is dropped and no `on_shutdown` hook runs; see [`AsyncKit::build`]'s
    /// `# Errors` section for the failure semantics.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "lifecycle")]
    pub fn register_lifecycle<M>(&mut self)
    where
        M: crate::core::lifecycle::AsyncLifecycle + 'static,
        M::Capability: Send + Sync + 'static,
    {
        // Idempotent: re-registering the same module must not duplicate its
        // on_ready / on_shutdown hooks.
        let module_type_id = TypeId::of::<M>();
        if self
            .lifecycle
            .ready_callbacks
            .read()
            .expect("lock poisoned")
            .iter()
            .any(|(id, _)| *id == module_type_id)
        {
            return;
        }

        let async_shutdown_hook: AsyncShutdownHookFn = Box::new(|caps: &AsyncTypeMap| {
            let type_id = TypeId::of::<M>();
            Box::pin(async move {
                // The read guard and the capability reference are bundled in an
                // `AsyncTypeMapReadGuard`, so the read lock stays held (and the
                // capability stays valid) until `on_shutdown` completes.
                if let Some(cap_guard) = caps.read_by_type_id::<M::Capability>(type_id) {
                    M::on_shutdown(&cap_guard).await;
                }
            })
        });
        self.lifecycle
            .async_shutdown_callbacks
            .write()
            .expect("lock poisoned")
            .push((TypeId::of::<M>(), async_shutdown_hook));

        let ready_cb: AsyncReadyCallback = Box::new(|kit: &AsyncKit<Ready>| {
            let fut = M::on_ready(kit);
            Box::pin(async move {
                fut.await.map_err(|e| TraitKitError::LifecycleFailed {
                    context: M::NAME.to_string(),
                    source: Box::new(e),
                })
            })
        });
        self.lifecycle
            .ready_callbacks
            .write()
            .expect("lock poisoned")
            .push((TypeId::of::<M>(), ready_cb));
    }

    // ─── Health Check ──────────────────────────────────────────────────

    /// Register a health checker for an async module.
    ///
    /// Requires the `health` feature.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "health")]
    pub fn register_health_check<M>(&mut self)
    where
        M: crate::core::health::AsyncHealthCheck + 'static,
        M::Capability: Send + Sync + 'static,
    {
        let checker: AsyncHealthCheckerFn = Arc::new(|caps: &AsyncTypeMap| {
            let type_id = TypeId::of::<M>();
            match caps.read_by_type_id::<M::Capability>(type_id) {
                Some(cap_guard) => M::check(&cap_guard),
                None => crate::core::health::HealthStatus::Unhealthy {
                    detail: "capability not found".to_string(),
                },
            }
        });
        self.health
            .health_checkers
            .write()
            .expect("lock poisoned")
            .insert(TypeId::of::<M>(), (M::NAME, checker));
    }

    // ─── Conditional Registration ───────────────────────────────────────

    /// Conditionally register an async module based on a runtime predicate.
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::AlreadyRegistered`] if the module was already registered.
    #[must_use = "ignoring the Result may hide registration failures"]
    pub fn register_if<M: AsyncAutoBuilder>(
        &mut self,
        predicate: impl FnOnce(&AsyncKit) -> bool,
    ) -> Result<bool, TraitKitError> {
        if predicate(self) {
            self.register::<M>()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    // ─── Observability ─────────────────────────────────────────────────

    /// Register a build observer for the async build pipeline.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "observer")]
    pub fn with_observer(&mut self, observer: Arc<dyn crate::core::observer::BuildObserver>) {
        self.observer
            .observers
            .write()
            .expect("lock poisoned")
            .push(observer);
    }

    // ─── Observation Ports ─────────────────────────────────────────────

    /// Inject a [`MetricsPort`](crate::kit::ports::MetricsPort) for recording counters/gauges/histograms.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    pub fn with_metrics_port(&mut self, port: impl Into<super::ports::OptionalMetricsPort>) {
        *self.ports.metrics_port.write().expect("lock poisoned") = port.into();
    }

    /// Inject a [`LogPort`](crate::kit::ports::LogPort) for structured log recording.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    /// # Panics
    ///
    /// Panics if the log port lock is poisoned.
    pub fn with_log_port(&mut self, port: impl Into<super::ports::OptionalLogPort>) {
        *self.ports.log_port.write().expect("lock poisoned") = port.into();
    }

    /// Inject an [`EventBus`](super::events::EventBus) that receives runtime
    /// lifecycle events: module builds, health samples, config changes.
    ///
    /// Default is `None` (= no-op): publishing costs one `Option` check.
    ///
    /// # Panics
    ///
    /// Panics if the event bus lock is poisoned.
    pub fn with_event_bus(&mut self, bus: impl Into<super::events::OptionalEventBus>) {
        *self.ports.event_bus.write().expect("lock poisoned") = bus.into();
    }

    /// Configure the per-layer build concurrency limit.
    ///
    /// `AsyncKit::build()` groups modules into topological layers and drives
    /// the futures of each layer concurrently, with at most `limit` module
    /// builds in flight. Default: unlimited. Values are clamped to >= 1.
    ///
    /// # Panics
    ///
    /// This setter never panics; the limit is clamped to `>= 1` defensively.
    pub fn with_max_concurrency(&mut self, limit: usize) {
        self.max_concurrency = limit.max(1);
    }

    /// Retrieve the injected event bus, if any.
    ///
    /// # Panics
    ///
    /// Panics if the event bus lock is poisoned.
    #[must_use]
    pub fn event_bus(&self) -> super::events::OptionalEventBus {
        self.ports.event_bus.read().expect("lock poisoned").clone()
    }

    // ─── Decorator ─────────────────────────────────────────────────────

    /// Register a decorator for an async module's capability.
    ///
    /// Requires the `decorator` feature.
    ///
    /// # Panics
    ///
    /// Panics at runtime if the internal `downcast` fails due to a type
    /// mismatch (should never happen when used correctly), or if the
    /// internal `RwLock` is poisoned.
    #[cfg(feature = "decorator")]
    pub fn decorate<M: AsyncAutoBuilder>(
        &self,
        decorator: impl Fn(M::Capability) -> M::Capability + Send + Sync + 'static,
    ) where
        M::Capability: Send + Sync + 'static,
    {
        let wrapper: AsyncDecoratorFn = Box::new(move |boxed_cap| {
            let cap = boxed_cap
                .downcast::<M::Capability>()
                .expect("decorator type mismatch");
            let decorated = decorator(*cap);
            Box::new(decorated) as Box<dyn Any + Send + Sync>
        });
        self.decorator
            .decorators
            .write()
            .expect("lock poisoned")
            .entry(TypeId::of::<M::Capability>())
            .or_default()
            .push(wrapper);
        // Record module TypeId → capability TypeId mapping so
        // `build()` can look up decorators by module TypeId.
        self.decorator
            .decorator_module_to_cap
            .write()
            .expect("lock poisoned")
            .insert(TypeId::of::<M>(), TypeId::of::<M::Capability>());
    }
}

impl<S> AsyncKit<S> {
    /// Publish `event` to the injected bus (no-op when absent). Internal
    /// helper keeping the `Option` check in exactly one place.
    fn publish_event(&self, event: super::events::KitEvent) {
        if let Some(bus) = self.ports.event_bus.read().expect("lock poisoned").as_ref() {
            bus.publish(event);
        }
    }

    /// Publish a custom event to the injected bus (public escape hatch):
    /// lets module builders feed their own lifecycle events into the same
    /// channel. No-op when no bus is injected.
    pub fn emit_event(&self, event: super::events::KitEvent) {
        self.publish_event(event);
    }

    /// Apply registered decorators for a capability (keyed by capability `TypeId`).
    #[cfg(feature = "decorator")]
    fn apply_decorators(
        &self,
        cap_type_id: TypeId,
        boxed: Box<dyn Any + Send + Sync>,
    ) -> Box<dyn Any + Send + Sync> {
        let decorators = self.decorator.decorators.read().expect("lock poisoned");
        let Some(dec_list) = decorators.get(&cap_type_id) else {
            return boxed;
        };
        let mut current = boxed;
        for dec in dec_list {
            current = dec(current);
        }
        current
    }

    /// Get a configuration value.
    ///
    /// Available on both `AsyncKit<Unbuilt>` (inside `AsyncAutoBuilder::build`
    /// callbacks) and `AsyncKit<Ready>` (after `build()` completes).
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::MissingConfig`] if no value of type `C` was set.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    pub fn config<C: Clone + Send + Sync + 'static>(&self) -> Result<C, TraitKitError> {
        self.configs
            .get_cloned::<C>()
            .ok_or(TraitKitError::MissingConfig {
                key: std::any::type_name::<C>().to_string(),
            })
    }

    /// Retrieve a capability by its module type.
    ///
    /// Available on both `AsyncKit<Unbuilt>` (inside `AsyncAutoBuilder::build`
    /// callbacks, for cross-module dependency injection during `build()`) and
    /// `AsyncKit<Ready>` (after `build()` completes).
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::MissingCapability`] if the module has not been
    /// built yet (its `TypeId` is absent from the capabilities map).
    ///
    /// # Panics
    ///
    /// Panics if the `capabilities` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    pub fn require<M: AsyncAutoBuilder>(&self) -> Result<M::Capability, TraitKitError> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
            .ok_or(TraitKitError::MissingCapability {
                key: M::NAME.to_string(),
            })
    }
}

impl AsyncKit<Ready> {
    /// Retrieve an optional capability. Returns `None` if the module has not
    /// been built (its `TypeId` is absent from the capabilities map).
    ///
    /// # Panics
    ///
    /// Panics if the `capabilities` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    #[must_use]
    pub fn optional<M: AsyncAutoBuilder>(&self) -> Option<M::Capability> {
        let type_id = TypeId::of::<M>();
        self.capabilities
            .get_cloned_by_type_id::<M::Capability>(type_id)
    }

    /// Check if a capability has been built (its `TypeId` is present in the
    /// capabilities map).
    ///
    /// # Panics
    ///
    /// Panics if the `capabilities` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    #[must_use]
    pub fn contains<M: AsyncAutoBuilder>(&self) -> bool {
        self.capabilities.contains_by_type_id(TypeId::of::<M>())
    }

    /// Check if a config of type `C` has been registered.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned. See
    /// [`register`](Self::register) for context on lock poisoning.
    #[must_use]
    pub fn contains_config<C: Clone + Send + Sync + 'static>(&self) -> bool {
        self.configs.contains::<C>()
    }

    // ─── Lifecycle: shutdown ───────────────────────────────────────────

    /// Shut down all lifecycle modules, awaiting the async `on_shutdown` hooks.
    ///
    /// Requires the `lifecycle` feature.
    ///
    /// This method is **one-shot**: the hook registry is drained, so a second
    /// call finds no hooks and is a no-op. Hooks run in **reverse topological
    /// order** — dependent modules shut down before the modules they depend
    /// on — and each `AsyncLifecycle::on_shutdown` hook is awaited to
    /// completion one at a time, with no concurrent join, keeping the
    /// shutdown order predictable. Hooks of modules absent from the dependency
    /// graph keep registration order at the tail.
    ///
    /// The returned future is intentionally `!Send`: each hook holds the
    /// capability's `AsyncTypeMapReadGuard` across its `await` (required
    /// for soundness with the `std::sync::RwLock`-backed map, since that
    /// guard is `!Send`). Await `shutdown_async` in place (e.g. in your
    /// async `main` or via a `block_on` helper) instead of spawning it on a
    /// multi-threaded runtime.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "lifecycle")]
    pub async fn shutdown_async(&self) {
        let async_hooks: Vec<(TypeId, AsyncShutdownHookFn)> = {
            self.lifecycle
                .async_shutdown_callbacks
                .write()
                .expect("lock poisoned")
                .drain(..)
                .collect()
        };
        // Reverse order = reverse topological order (hooks were stable-sorted
        // by topological index in `build()`): dependents complete before the
        // modules they depend on. Each hook is awaited before the next starts.
        for (_type_id, hook) in async_hooks.iter().rev() {
            hook(&self.capabilities).await;
        }
    }

    // ─── Health Check ──────────────────────────────────────────────────

    /// Check the health of a specific async module.
    ///
    /// Requires the `health` feature.
    ///
    /// # Errors
    ///
    /// Returns [`TraitKitError::MissingConfig`] if no health checker is registered
    /// for the given module.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "health")]
    pub fn health_check<M: crate::core::health::AsyncHealthCheck>(
        &self,
    ) -> Result<crate::core::health::HealthStatus, TraitKitError> {
        let type_id = TypeId::of::<M>();
        // Collect an owned `Arc` clone of the checker inside the lock, then
        // release the lock before invoking it: the checker callback may run
        // arbitrary user code and must not run while holding the internal
        // `RwLock` read guard.
        let checker: AsyncHealthCheckerFn = {
            let checkers = self.health.health_checkers.read().expect("lock poisoned");
            let (_name, checker) = checkers.get(&type_id).ok_or(TraitKitError::MissingConfig {
                key: M::NAME.to_string(),
            })?;
            Arc::clone(checker)
        };
        Ok(checker(&self.capabilities))
    }

    /// Generate a health report for all registered async health checkers.
    ///
    /// Requires the `health` feature.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[cfg(feature = "health")]
    #[must_use]
    pub fn health_report(&self) -> Vec<(&'static str, crate::core::health::HealthStatus)> {
        // Same lock discipline as `health_check`: clone the checker `Arc`s out
        // of the lock, drop the guard, then run each checker outside the lock.
        let checkers: Vec<(&'static str, AsyncHealthCheckerFn)> = {
            let guard = self.health.health_checkers.read().expect("lock poisoned");
            guard
                .values()
                .map(|(name, checker)| (*name, Arc::clone(checker)))
                .collect()
        };
        let report: Vec<(&'static str, crate::core::health::HealthStatus)> = checkers
            .into_iter()
            .map(|(name, checker)| (name, checker(&self.capabilities)))
            .collect();
        // publish each sampled status to the injected event bus.
        if self
            .ports
            .event_bus
            .read()
            .expect("lock poisoned")
            .is_some()
        {
            for (name, status) in &report {
                self.publish_event(super::events::KitEvent::HealthChanged {
                    module: name,
                    status: status.as_status_name(),
                    detail: status.detail().map(str::to_owned),
                });
            }
        }
        report
    }

    // ─── Factory Pattern ───────────────────────────────────────────────

    /// Create a factory closure that produces new async instances on each call.
    ///
    /// The returned closure is `Send + Sync`, suitable for use in multi-threaded
    /// async runtimes.
    ///
    #[allow(clippy::type_complexity)]
    pub fn factory<M: AsyncAutoBuilder>(
        &self,
    ) -> impl Fn() -> Pin<Box<dyn Future<Output = Result<M::Capability, TraitKitError>> + Send>>
    + Send
    + Sync
    + '_ {
        // Compile-time layout assertions: if any field depending on `S` is
        // added to `AsyncKit`, these will fail at compile time.
        const _: () = assert!(
            std::mem::size_of::<AsyncKit<Ready>>() == std::mem::size_of::<AsyncKit>(),
            "AsyncKit size changed; unsafe cast is no longer sound"
        );
        const _: () = assert!(
            std::mem::align_of::<AsyncKit<Ready>>() == std::mem::align_of::<AsyncKit>(),
            "AsyncKit alignment changed; unsafe cast is no longer sound"
        );

        move || {
            // SAFETY: `AsyncKit<Ready>` and `AsyncKit` (= `AsyncKit<Unbuilt>`)
            // share identical memory layout — `S` appears only in
            // `PhantomData<S>`, guaranteed by the const size/align assertions
            // above. The closure is bounded by `'_`, keeping `self` (and the
            // kit it points to) alive for every call.
            #[allow(unsafe_code)]
            let kit_ref: &AsyncKit =
                unsafe { &*std::ptr::from_ref::<AsyncKit<Ready>>(self).cast::<AsyncKit>() };
            let fut = M::build(kit_ref);
            Box::pin(async move {
                fut.await.map_err(|e| TraitKitError::BuildFailed {
                    context: M::NAME.to_string(),
                    source: Box::new(e),
                })
            })
                as Pin<Box<dyn Future<Output = Result<M::Capability, TraitKitError>> + Send>>
        }
    }

    // ─── Scope ─────────────────────────────────────────────────────────

    /// Create a new empty async scope.
    ///
    /// Requires the `scope` feature.
    #[cfg(feature = "scope")]
    #[must_use]
    pub fn create_scope(&self) -> super::scope::AsyncScope {
        super::scope::AsyncScope::new()
    }

    // ─── Graph Visualization ───────────────────────────────────────────

    /// Export the dependency graph as a Graphviz DOT string.
    #[must_use]
    pub fn graph_dot(&self) -> String {
        self.graph.to_dot()
    }

    /// Export the dependency graph as a Mermaid flowchart string.
    #[must_use]
    pub fn graph_mermaid(&self) -> String {
        self.graph.to_mermaid()
    }
}

// ─── Config inheritance (confers feature, async) ────────────────────────

impl AsyncKit {
    /// Populate the config `TypeMap` with `C::default_value()` if no value of
    /// type `C` is present.
    ///
    /// Returns `true` if the default was populated, `false` if a value already
    /// existed (the existing value is not overridden).
    ///
    /// Requires the `confers` feature.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned.
    #[cfg(feature = "confers")]
    #[must_use]
    pub fn populate_defaults<C: super::ModuleConfig + Send + Sync>(&self) -> bool {
        if self.configs.contains::<C>() {
            return false;
        }
        self.set_config(C::default_value());
        true
    }

    /// Apply a compile-time safe field-level override to the config of type `C`.
    ///
    /// Reads the current config, applies non-`None` fields from `ovr`, and
    /// writes the result back. If no config of type `C` exists, this is a
    /// no-op (does not panic).
    ///
    /// Requires the `confers` feature.
    ///
    /// # Panics
    ///
    /// Panics if the `configs` [`RwLock`] is poisoned.
    #[cfg(feature = "confers")]
    pub fn merge_config<C: super::ConfigInherit + Send + Sync>(&self, ovr: C::Override)
    where
        C::Override: Send + Sync,
    {
        if let Ok(mut current) = self.config::<C>() {
            current.apply_override(&ovr);
            self.set_config(current);
        }
    }

    /// Extract shared fields from config `C` into the `AsyncKit`'s shared overlay.
    ///
    /// Calls `C::extract_shared()` and merges the result into
    /// `self.confers.shared_fields`. New values override same-named keys from
    /// previous extractions. If no config of type `C` exists, this is a
    /// no-op.
    ///
    /// Requires the `confers` feature.
    ///
    /// # Panics
    ///
    /// Panics if the `shared_fields` [`RwLock`] is poisoned.
    #[cfg(feature = "confers")]
    pub fn extract_shared<C: super::SharedConfig + Send + Sync>(&self) {
        if let Ok(config) = self.config::<C>() {
            let fields = config.extract_shared();
            self.confers
                .shared_fields
                .write()
                .expect("shared_fields lock poisoned")
                .extend(fields);
        }
    }

    /// Inject shared fields from the `AsyncKit`'s overlay into config `C`.
    ///
    /// Reads the current shared overlay and calls `C::inject_shared()`.
    /// The updated config is written back to the `TypeMap`. If no config of
    /// type `C` exists, this is a no-op.
    ///
    /// Requires the `confers` feature.
    ///
    /// # Panics
    ///
    /// Panics if the `shared_fields` [`RwLock`] is poisoned.
    #[cfg(feature = "confers")]
    pub fn inject_shared<C: super::SharedConfig + Send + Sync>(&self) {
        if let Ok(mut config) = self.config::<C>() {
            let overlay = self
                .confers
                .shared_fields
                .read()
                .expect("shared_fields lock poisoned")
                .clone();
            config.inject_shared(&overlay);
            self.set_config(config);
        }
    }
}

impl AsyncKit {
    /// Load a configuration via its `Configurable` implementation and store it.
    ///
    /// Requires the `confers` feature. Async counterpart of `Kit::load_config`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if `Configurable::load` fails.
    #[cfg(feature = "confers")]
    pub fn load_config<C: super::Configurable + Send + Sync + 'static>(
        &self,
    ) -> Result<(), TraitKitError> {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "load_config".into(),
            source: e,
        })?;
        self.set_config(config);
        Ok(())
    }

    /// Load a configuration and validate it before storing.
    ///
    /// Requires the `confers` feature. Async counterpart of `Kit::load_and_validate`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if loading or validation fails.
    #[cfg(feature = "confers")]
    pub fn load_and_validate<C>(&self) -> Result<(), TraitKitError>
    where
        C: super::Configurable + super::Validatable + Send + Sync + 'static,
    {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "load_and_validate".into(),
            source: e,
        })?;
        match config.validate() {
            Ok(()) => {
                self.set_config(config);
                Ok(())
            }
            Err(errors) => Err(TraitKitError::BuildFailed {
                context: "load_and_validate".into(),
                source: Box::new(super::ValidationError { errors }),
            }),
        }
    }

    /// Snapshot the current configuration of type `C`.
    ///
    /// Requires the `confers` feature. Returns `false` if no config of type `C` is present.
    ///
    /// # Panics
    ///
    /// Panics if the config snapshots lock is poisoned.
    #[cfg(feature = "confers")]
    #[must_use]
    pub fn snapshot_config<C: Clone + Send + Sync + 'static>(&self) -> bool {
        if let Some(config) = self.configs.get_cloned::<C>() {
            self.confers
                .config_snapshots
                .write()
                .expect("config_snapshots lock poisoned")
                .insert(TypeId::of::<C>(), Box::new(config));
            true
        } else {
            false
        }
    }

    /// Restore a configuration from its snapshot.
    ///
    /// Requires the `confers` feature.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingConfig` if no snapshot exists for `C`.
    ///
    /// # Panics
    ///
    /// Panics if the config snapshots lock is poisoned.
    #[cfg(feature = "confers")]
    pub fn restore_config<C: Clone + Send + Sync + 'static>(&self) -> Result<(), TraitKitError> {
        let snapshots = self
            .confers
            .config_snapshots
            .read()
            .expect("config_snapshots lock poisoned");
        let boxed =
            snapshots
                .get(&TypeId::of::<C>())
                .ok_or_else(|| TraitKitError::MissingConfig {
                    key: format!("{} (snapshot)", std::any::type_name::<C>()),
                })?;
        let config =
            boxed
                .downcast_ref::<C>()
                .cloned()
                .ok_or_else(|| TraitKitError::MissingConfig {
                    key: format!("{} (snapshot downcast)", std::any::type_name::<C>()),
                })?;
        drop(snapshots);
        self.set_config(config);
        Ok(())
    }

    /// Check if a snapshot exists for configuration type `C`.
    ///
    /// # Panics
    ///
    /// Panics if the config snapshots lock is poisoned.
    #[cfg(feature = "confers")]
    #[must_use]
    pub fn has_snapshot<C: 'static>(&self) -> bool {
        self.confers
            .config_snapshots
            .read()
            .expect("config_snapshots lock poisoned")
            .contains_key(&TypeId::of::<C>())
    }

    /// Load a configuration with variable interpolation.
    ///
    /// Requires the `confers` feature. Async counterpart of `Kit::load_config_with`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if loading, serialization, or
    /// deserialization fails.
    #[cfg(feature = "confers")]
    pub fn load_config_with<C, S: std::hash::BuildHasher>(
        &self,
        vars: &std::collections::HashMap<String, String, S>,
    ) -> Result<(), TraitKitError>
    where
        C: super::Configurable + serde::Serialize + serde::de::DeserializeOwned + Send + Sync,
    {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "load_config_with".into(),
            source: e,
        })?;
        let mut json_value =
            serde_json::to_value(&config).map_err(|e| TraitKitError::BuildFailed {
                context: "load_config_with (serialize)".into(),
                source: Box::new(e),
            })?;
        super::config::interpolate_json_value(&mut json_value, vars);
        let interpolated: C =
            serde_json::from_value(json_value).map_err(|e| TraitKitError::BuildFailed {
                context: "load_config_with (deserialize)".into(),
                source: Box::new(e),
            })?;
        self.set_config(interpolated);
        Ok(())
    }

    /// Load a configuration via `Configurable::load`, falling back to
    /// `ModuleConfig::default_value` if loading fails.
    ///
    /// Requires the `confers` feature. Returns `true` if `C::load()` succeeded.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if storing the loaded config fails
    /// (e.g. the config channel is closed).
    #[cfg(feature = "confers")]
    pub fn load_config_or_default<C>(&self) -> Result<bool, TraitKitError>
    where
        C: super::Configurable + super::ModuleConfig + Send + Sync,
    {
        match C::load() {
            Ok(value) => {
                self.set_config(value);
                Ok(true)
            }
            Err(_e) => {
                self.set_config(C::default_value());
                Ok(false)
            }
        }
    }

    /// Subscribe a callback to be invoked when config of type `C` is reloaded.
    ///
    /// Requires the `reload` feature. Async counterpart of `Kit::subscribe`.
    /// Callbacks use `Arc<dyn Fn() + Send + Sync>` (thread-safe).
    ///
    /// # Panics
    ///
    /// Panics if the reload subscribers lock is poisoned.
    #[cfg(feature = "reload")]
    pub fn subscribe<C: 'static>(&self, callback: impl Fn() + Send + Sync + 'static) {
        let callback: Arc<dyn Fn() + Send + Sync> = Arc::new(callback);
        self.reload
            .subscribers
            .write()
            .expect("reload subscribers lock poisoned")
            .entry(TypeId::of::<C>())
            .or_default()
            .push(callback);
    }

    /// Reload a configuration via its `Configurable` implementation and
    /// notify all subscribers of type `C`.
    ///
    /// Requires the `reload` feature. Async counterpart of `Kit::reload_config`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if `Configurable::load` fails.
    ///
    /// # Panics
    ///
    /// Panics if the reload subscribers lock is poisoned.
    #[cfg(feature = "reload")]
    pub fn reload_config<C: super::Configurable + Send + Sync>(&self) -> Result<(), TraitKitError> {
        let config = C::load().map_err(|e| TraitKitError::BuildFailed {
            context: "reload_config".into(),
            source: e,
        })?;
        self.configs.insert(config);
        let callbacks: Vec<Arc<dyn Fn() + Send + Sync>> = match self
            .reload
            .subscribers
            .read()
            .expect("reload subscribers lock poisoned")
            .get(&TypeId::of::<C>())
        {
            Some(subs) => subs.iter().map(Arc::clone).collect(),
            None => Vec::new(),
        };
        for cb in &callbacks {
            cb();
        }
        Ok(())
    }

    /// Encrypt and store a configuration value.
    ///
    /// Requires the `encryption` feature. Async counterpart of `Kit::set_encrypted`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::BuildFailed` if serialization, key derivation, or
    /// encryption fails.
    ///
    /// # Panics
    ///
    /// Panics if the encrypted-config lock is poisoned.
    #[cfg(feature = "encryption")]
    pub fn set_encrypted<C>(&self, value: &C, master_key: &[u8]) -> Result<(), TraitKitError>
    where
        C: super::ModuleConfig + serde::Serialize + Send + Sync,
    {
        use super::XChaCha20Crypto;

        if master_key.len() < 16 {
            return Err(TraitKitError::BuildFailed {
                context: "set_encrypted".into(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "master_key must be at least 16 bytes, got {}",
                        master_key.len()
                    ),
                )),
            });
        }

        let mut field_key = super::kit::derive_kit_field_key(master_key, C::PATH, "set_encrypted")?;

        let mut plaintext = match serde_json::to_vec(value) {
            Ok(vec) => vec,
            Err(e) => {
                super::kit::zeroize_bytes(&mut field_key);
                return Err(TraitKitError::BuildFailed {
                    context: "set_encrypted".into(),
                    source: Box::new(e),
                });
            }
        };

        let encrypted = XChaCha20Crypto::new().encrypt(&plaintext, &field_key);
        super::kit::zeroize_bytes(&mut field_key);
        super::kit::zeroize_bytes(&mut plaintext);
        let (nonce, ciphertext) = encrypted.map_err(|e| TraitKitError::BuildFailed {
            context: "set_encrypted".into(),
            source: Box::new(e),
        })?;

        self.encryption
            .encrypted_configs
            .write()
            .expect("encrypted_configs lock poisoned")
            .insert(
                TypeId::of::<C>(),
                super::EncryptedBlob::new(nonce, ciphertext),
            );
        Ok(())
    }

    /// Check if an encrypted config of type `C` is registered.
    ///
    /// # Panics
    ///
    /// Panics if the encrypted-config lock is poisoned.
    #[cfg(feature = "encryption")]
    #[must_use]
    pub fn contains_encrypted<C: super::ModuleConfig>(&self) -> bool {
        self.encryption
            .encrypted_configs
            .read()
            .expect("encrypted_configs lock poisoned")
            .contains_key(&TypeId::of::<C>())
    }

    /// Retrieve and decrypt a configuration value.
    ///
    /// Requires the `encryption` feature. Async counterpart of `Kit::get_encrypted`.
    ///
    /// # Errors
    ///
    /// Returns `TraitKitError::MissingConfig` if no encrypted blob for `C` exists.
    /// Returns `TraitKitError::BuildFailed` if key derivation, decryption, or
    /// deserialization fails.
    ///
    /// # Panics
    ///
    /// Panics if the encrypted-config lock is poisoned.
    #[cfg(feature = "encryption")]
    pub fn get_encrypted<C>(&self, master_key: &[u8]) -> Result<C, TraitKitError>
    where
        C: super::ModuleConfig + serde::de::DeserializeOwned + Send + Sync,
    {
        use super::XChaCha20Crypto;

        if master_key.len() < 16 {
            return Err(TraitKitError::BuildFailed {
                context: "get_encrypted".into(),
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "master_key must be at least 16 bytes, got {}",
                        master_key.len()
                    ),
                )),
            });
        }

        let blob = self
            .encryption
            .encrypted_configs
            .read()
            .expect("encrypted_configs lock poisoned")
            .get(&TypeId::of::<C>())
            .cloned()
            .ok_or(TraitKitError::MissingConfig {
                key: std::any::type_name::<C>().to_string(),
            })?;

        let mut field_key = super::kit::derive_kit_field_key(master_key, C::PATH, "get_encrypted")?;

        let decrypted = XChaCha20Crypto::new().decrypt(blob.nonce(), blob.ciphertext(), &field_key);
        super::kit::zeroize_bytes(&mut field_key);
        let mut plaintext = decrypted.map_err(|e| TraitKitError::BuildFailed {
            context: "get_encrypted".into(),
            source: Box::new(e),
        })?;

        let parsed: Result<C, _> = serde_json::from_slice(&plaintext);
        super::kit::zeroize_bytes(&mut plaintext);
        parsed.map_err(|e| TraitKitError::BuildFailed {
            context: "get_encrypted".into(),
            source: Box::new(e),
        })
    }

    // ─── Observation Port Accessors ────────────────────────────────────

    /// Retrieve the injected [`MetricsPort`](crate::kit::ports::MetricsPort), if any.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[must_use]
    pub fn metrics_port(&self) -> super::ports::OptionalMetricsPort {
        self.ports
            .metrics_port
            .read()
            .expect("lock poisoned")
            .clone()
    }

    /// Retrieve the injected [`LogPort`](crate::kit::ports::LogPort), if any.
    ///
    /// # Panics
    ///
    /// Panics if the internal `RwLock` is poisoned.
    #[must_use]
    pub fn log_port(&self) -> super::ports::OptionalLogPort {
        self.ports.log_port.read().expect("lock poisoned").clone()
    }
}

impl Default for AsyncKit {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for AsyncKit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncKit<Unbuilt>")
            .field("modules", &self.graph.entries().len())
            .field("configs", &self.configs.len())
            .finish()
    }
}

impl std::fmt::Debug for AsyncKit<Ready> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncKit<Ready>")
            .field("modules", &self.graph.entries().len())
            .field("configs", &self.configs.len())
            .finish()
    }
}

#[cfg(all(test, feature = "async"))]
mod tests {
    use super::{AsyncKit, Ready};
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::error::TraitKitError;
    use crate::test_helpers::{MockError, block_on};
    use std::any::TypeId;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug, Clone, PartialEq)]
    struct MockCap {
        value: i32,
    }

    struct MockModule;

    impl ModuleMeta for MockModule {
        const NAME: &'static str = "mock-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockModule {
        type Capability = Arc<MockCap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(MockCap { value: 42 })) })
        }
    }

    // --- mock modules for build() tests ---

    /// Build callback returns `Err`, exercising `TraitKitError::BuildFailed`.
    struct MockErrModule;

    impl ModuleMeta for MockErrModule {
        const NAME: &'static str = "mock-err-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockErrModule {
        type Capability = Arc<MockCap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Err(MockError::Failed("intentional build failure".to_string())) })
        }
    }

    /// Build callback reads an `Arc<AtomicUsize>` config and increments it,
    /// proving the async body actually executed.
    struct MockCounterModule;

    impl ModuleMeta for MockCounterModule {
        const NAME: &'static str = "mock-counter-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockCounterModule {
        type Capability = Arc<()>;

        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            Box::pin(async move {
                let counter = kit
                    .config::<Arc<AtomicUsize>>()
                    .map_err(|e| MockError::Failed(e.to_string()))?;
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(()))
            })
        }
    }

    /// Phantom module that is never registered; used as a declared-but-missing
    /// dependency to trigger `TraitKitError::DependencyMissing`.
    struct MissingDep;

    /// Declares a dependency on `MissingDep` (unregistered) to trigger
    /// `TraitKitError::DependencyMissing` during `graph.validate()`.
    struct MockMissingDepModule;

    impl ModuleMeta for MockMissingDepModule {
        const NAME: &'static str = "mock-missing-dep-module";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("missing-dep", TypeId::of::<MissingDep>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockMissingDepModule {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    /// First half of a 2-node dependency cycle.
    struct MockCycleA;

    impl ModuleMeta for MockCycleA {
        const NAME: &'static str = "mock-cycle-a";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-b", TypeId::of::<MockCycleB>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleA {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    /// Second half of a 2-node dependency cycle.
    struct MockCycleB;

    impl ModuleMeta for MockCycleB {
        const NAME: &'static str = "mock-cycle-b";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-a", TypeId::of::<MockCycleA>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleB {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_kit_new_returns_unbuilt_state() {
        let kit = AsyncKit::new();
        assert!(kit.builders.read().expect("lock poisoned").is_empty());
        assert!(kit.graph.entries().is_empty());
        assert_eq!(kit.configs.len(), 0);
        assert_eq!(kit.capabilities.len(), 0);
    }

    #[test]
    fn async_kit_register_stores_builder() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        assert_eq!(kit.builders.read().expect("lock poisoned").len(), 1);
        assert_eq!(kit.graph.entries().len(), 1);
    }

    #[test]
    fn async_kit_register_duplicate_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("first register should succeed");
        let err = kit
            .register::<MockModule>()
            .expect_err("duplicate register should error");
        assert!(
            matches!(
                err,
                TraitKitError::AlreadyRegistered {
                    module: "mock-module"
                }
            ),
            "expected AlreadyRegistered, got {err:?}"
        );
    }

    #[test]
    fn async_kit_concurrent_registration() {
        // 多线程并发注册不同模块：AsyncKit 内部为 Arc<RwLock>，
        // 并发 register 不得丢注册、不得 panic、不得产生重复条目。
        use std::sync::Mutex;

        let kit = Arc::new(Mutex::new(AsyncKit::new()));
        let mut handles = Vec::new();
        for (i, name) in ["mock-b", "mock-c"].iter().enumerate() {
            let kit = Arc::clone(&kit);
            let name = *name;
            handles.push(std::thread::spawn(move || {
                let mut kit = kit.lock().expect("lock poisoned");
                match name {
                    "mock-b" => kit.register::<MockBModule>().expect("register B"),
                    _ => kit.register::<MockCModule>().expect("register C"),
                }
                let _ = i;
            }));
        }
        for h in handles {
            h.join().expect("worker thread panicked");
        }

        // 并发后两个模块都注册成功，且无重复。
        let kit = kit.lock().expect("lock poisoned");
        assert_eq!(
            kit.builders.read().expect("lock poisoned").len(),
            2,
            "both modules must be registered exactly once"
        );
        assert_eq!(kit.graph.entries().len(), 2);
    }

    #[test]
    fn async_kit_set_config_stores_value() {
        let kit = AsyncKit::new();
        kit.set_config(42i32);
        assert_eq!(kit.config::<i32>().expect("config should exist"), 42);
    }

    #[test]
    fn async_kit_set_config_overwrite() {
        let kit = AsyncKit::new();
        kit.set_config(1i32);
        kit.set_config(2i32);
        assert_eq!(kit.config::<i32>().expect("config should exist"), 2);
    }

    #[test]
    fn async_kit_config_missing_returns_error() {
        let kit = AsyncKit::new();
        let err = kit
            .config::<u64>()
            .expect_err("missing config should error");
        assert!(
            matches!(err, TraitKitError::MissingConfig { .. }),
            "expected MissingConfig, got {err:?}"
        );
    }

    #[test]
    fn async_kit_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AsyncKit>();
    }

    // --- MED-002: Send-ness assertions for TraitKitError and build() result ---

    /// Verifies HIGH-001: `TraitKitError` is `Send` (so it can cross
    /// `tokio::spawn` boundaries). Before HIGH-001, `TraitKitError::BuildFailed::source`
    /// was `Box<dyn Error>` (without `+ Send`), which made the entire enum
    /// `!Send` and blocked `tokio::spawn(async move { kit.build().await })`.
    #[test]
    fn kit_error_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<TraitKitError>();
    }

    /// Verifies HIGH-001: `AsyncKit::build()`'s return type is `Send`, so the
    /// spawned future's output satisfies `tokio::spawn`'s `Send` requirement
    /// on a multi-threaded runtime.
    #[test]
    fn async_kit_build_result_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<Result<AsyncKit<Ready>, TraitKitError>>();
    }

    // --- tests for AsyncKit::build() ---

    #[test]
    fn async_kit_build_returns_ready_state() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built: AsyncKit<Ready> = block_on(kit.build()).expect("build should succeed");
        // Type assertion via let binding: built must be AsyncKit<Ready>.
        let _ = built;
    }

    #[test]
    fn async_kit_build_constructs_capability() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        let cap = built
            .capabilities
            .get_cloned_by_type_id::<Arc<MockCap>>(TypeId::of::<MockModule>())
            .expect("capability should be stored after build");
        assert_eq!(cap.value, 42);
    }

    #[test]
    fn async_kit_build_multiple_modules_in_topo_order() {
        let mut kit = AsyncKit::new();
        kit.set_config(Arc::new(AtomicUsize::new(0)));
        kit.register::<MockModule>().expect("register module A");
        kit.register::<MockCounterModule>()
            .expect("register module B");
        let built = block_on(kit.build()).expect("build should succeed");
        assert_eq!(
            built.capabilities.len(),
            2,
            "capabilities should contain both modules"
        );
    }

    #[test]
    fn async_kit_build_missing_dependency_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockMissingDepModule>()
            .expect("register should succeed (declares missing dep)");
        let err =
            block_on(kit.build()).expect_err("build should fail when a dependency is unregistered");
        assert!(
            matches!(
                err,
                TraitKitError::DependencyMissing {
                    module: "mock-missing-dep-module",
                    missing: "missing-dep"
                }
            ),
            "expected DependencyMissing, got {err:?}"
        );
    }

    #[test]
    fn async_kit_build_cycle_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockCycleA>().expect("register cycle A");
        kit.register::<MockCycleB>().expect("register cycle B");
        let err = block_on(kit.build()).expect_err("build should fail on cyclic dependency graph");
        assert!(
            matches!(err, TraitKitError::CycleDetected { .. }),
            "expected CycleDetected, got {err:?}"
        );
    }

    #[test]
    fn async_kit_build_calls_async_build_fn() {
        let counter = Arc::new(AtomicUsize::new(0));
        let mut kit = AsyncKit::new();
        kit.set_config(Arc::clone(&counter));
        kit.register::<MockCounterModule>()
            .expect("register should succeed");
        let _built = block_on(kit.build()).expect("build should succeed");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "async build callback should have executed exactly once"
        );
    }

    #[test]
    fn async_kit_build_propagates_build_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockErrModule>()
            .expect("register should succeed");
        let err =
            block_on(kit.build()).expect_err("build should fail when module build returns Err");
        match &err {
            TraitKitError::BuildFailed { context, .. } => {
                assert_eq!(
                    context.as_str(),
                    "mock-err-module",
                    "expected BuildFailed for mock-err-module, got {err:?}"
                );
            }
            _ => panic!("expected BuildFailed for mock-err-module, got {err:?}"),
        }
    }

    // --- tests for AsyncKit<Ready> retrieval API (require/optional/contains/contains_config) ---

    #[test]
    fn async_kit_ready_require_returns_capability() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        let cap = built
            .require::<MockModule>()
            .expect("require on built module should succeed");
        assert_eq!(cap.value, 42);
    }

    #[test]
    fn async_kit_ready_require_missing_returns_error() {
        // Empty kit: MockModule is never registered/built, so its TypeId is
        // absent from the capabilities map. `require` must return MissingCapability.
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("empty build should succeed");
        let err = built
            .require::<MockModule>()
            .expect_err("require on unbuilt module should error");
        assert!(
            matches!(err, TraitKitError::MissingCapability { ref key } if key == "mock-module"),
            "expected MissingCapability for mock-module, got {err:?}"
        );
    }

    #[test]
    fn async_kit_ready_optional_returns_some_for_built() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        let cap = built
            .optional::<MockModule>()
            .expect("optional on built module should return Some");
        assert_eq!(cap.value, 42);
    }

    #[test]
    fn async_kit_ready_optional_returns_none_for_unbuilt() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("empty build should succeed");
        assert!(
            built.optional::<MockModule>().is_none(),
            "optional on unbuilt module should return None"
        );
    }

    #[test]
    fn async_kit_ready_contains_returns_true_for_built() {
        let mut kit = AsyncKit::new();
        kit.register::<MockModule>()
            .expect("register should succeed");
        let built = block_on(kit.build()).expect("build should succeed");
        assert!(
            built.contains::<MockModule>(),
            "contains should return true for built module"
        );
    }

    #[test]
    fn async_kit_ready_contains_returns_false_for_unbuilt() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).expect("empty build should succeed");
        assert!(
            !built.contains::<MockModule>(),
            "contains should return false for unbuilt module"
        );
    }

    #[test]
    fn async_kit_ready_contains_config_returns_true() {
        let kit = AsyncKit::new();
        kit.set_config(42i32);
        let built = block_on(kit.build()).expect("build should succeed");
        assert!(
            built.contains_config::<i32>(),
            "contains_config should return true for stored i32 config"
        );
    }

    #[test]
    fn async_kit_ready_contains_config_returns_false() {
        let kit = AsyncKit::new();
        kit.set_config(42i32);
        let built = block_on(kit.build()).expect("build should succeed");
        assert!(
            !built.contains_config::<u64>(),
            "contains_config should return false for absent u64 config"
        );
    }

    //
    // MockBModule: no deps, cap = Arc<Bcap{n:42}>.
    // MockAModule: declares dep on MockBModule; build() calls
    //   `kit.require::<MockBModule>()?` and embeds B's n into A's cap.
    //   This is the canonical DI pattern from design.md Decision 3.
    // MockCModule / MockChainBModule / MockChainAModule: transitive
    //   A→B→C chain; each build callback calls require on its direct dep.
    // MockCycleA3/B3/C3: 3-node cycle A→B→C→A for cycle detection.
    //
    // `From<TraitKitError> for MockError` lets `?` convert require errors
    // (matches the production pattern in design.md where DbNexusModule
    // uses `kit.require::<OxcacheModule>()?` with `OxcacheError: From<TraitKitError>`).

    impl From<TraitKitError> for MockError {
        fn from(e: TraitKitError) -> Self {
            MockError::Failed(e.to_string())
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Bcap {
        n: i32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Acap {
        b_val: i32,
    }

    struct MockBModule;

    impl ModuleMeta for MockBModule {
        const NAME: &'static str = "mock-b";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockBModule {
        type Capability = Arc<Bcap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(Bcap { n: 42 })) })
        }
    }

    struct MockAModule;

    impl ModuleMeta for MockAModule {
        const NAME: &'static str = "mock-a";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-b", TypeId::of::<MockBModule>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockAModule {
        type Capability = Arc<Acap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            Box::pin(async move {
                // DI happens here: pull B's cap from the kit during A's build.
                let b_cap: Arc<Bcap> = kit.require::<MockBModule>()?;
                Ok(Arc::new(Acap { b_val: b_cap.n }))
            })
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Ccap {
        v: i32,
        build_order: usize,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ChainBcap {
        c_val: i32,
        build_order: usize,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct ChainAcap {
        b_val: i32,
        build_order: usize,
    }

    struct MockCModule;

    impl ModuleMeta for MockCModule {
        const NAME: &'static str = "mock-c";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }

    impl AsyncAutoBuilder for MockCModule {
        type Capability = Arc<Ccap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            Box::pin(async move {
                let counter = kit.config::<Arc<AtomicUsize>>()?;
                let order = counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(Ccap {
                    v: 100,
                    build_order: order + 1,
                }))
            })
        }
    }

    struct MockChainBModule;

    impl ModuleMeta for MockChainBModule {
        const NAME: &'static str = "mock-chain-b";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-c", TypeId::of::<MockCModule>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockChainBModule {
        type Capability = Arc<ChainBcap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            Box::pin(async move {
                // DI: pull C's cap during B's build.
                let c_cap: Arc<Ccap> = kit.require::<MockCModule>()?;
                let counter = kit.config::<Arc<AtomicUsize>>()?;
                let order = counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(ChainBcap {
                    c_val: c_cap.v,
                    build_order: order + 1,
                }))
            })
        }
    }

    struct MockChainAModule;

    impl ModuleMeta for MockChainAModule {
        const NAME: &'static str = "mock-chain-a";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-chain-b", TypeId::of::<MockChainBModule>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockChainAModule {
        type Capability = Arc<ChainAcap>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            Box::pin(async move {
                // DI: pull chain-B's cap during A's build (transitive).
                let b_cap: Arc<ChainBcap> = kit.require::<MockChainBModule>()?;
                let counter = kit.config::<Arc<AtomicUsize>>()?;
                let order = counter.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(ChainAcap {
                    b_val: b_cap.c_val,
                    build_order: order + 1,
                }))
            })
        }
    }

    // 3-node cycle: MockCycleA3 → MockCycleB3 → MockCycleC3 → MockCycleA3.
    // Build callbacks are trivial because graph.validate() rejects the cycle
    // before any build_fn is invoked.
    struct MockCycleA3;

    impl ModuleMeta for MockCycleA3 {
        const NAME: &'static str = "mock-cycle-a3";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-b3", TypeId::of::<MockCycleB3>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleA3 {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    struct MockCycleB3;

    impl ModuleMeta for MockCycleB3 {
        const NAME: &'static str = "mock-cycle-b3";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-c3", TypeId::of::<MockCycleC3>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleB3 {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    struct MockCycleC3;

    impl ModuleMeta for MockCycleC3 {
        const NAME: &'static str = "mock-cycle-c3";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            static DEPS: &[(&str, TypeId)] = &[("mock-cycle-a3", TypeId::of::<MockCycleA3>())];
            DEPS
        }
    }

    impl AsyncAutoBuilder for MockCycleC3 {
        type Capability = Arc<()>;
        type Error = MockError;

        fn build<'a>(
            kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>
        {
            let _ = kit;
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    // --- tests: cross-module dependency injection (R-004) ---

    /// R-004 #1: A declares dep on B; B is built before A (topo order).
    /// A's cap embeds B's n=42, proving B was ready when A's build ran.
    #[test]
    fn async_kit_di_dependency_built_before_dependent() {
        let mut kit = AsyncKit::new();
        kit.register::<MockBModule>().expect("register B");
        kit.register::<MockAModule>().expect("register A");
        let built = block_on(kit.build()).expect("build should succeed");
        let a_cap = built
            .require::<MockAModule>()
            .expect("A's cap should be built");
        assert_eq!(
            a_cap.b_val, 42,
            "A's cap must contain B's n=42 — proves B built before A"
        );
    }

    /// R-004 #2: A's build callback calls `kit.require::<MockBModule>()`
    /// and receives B's capability. Both caps are retrievable post-build.
    #[test]
    fn async_kit_di_require_returns_dependency_capability() {
        let mut kit = AsyncKit::new();
        kit.register::<MockBModule>().expect("register B");
        kit.register::<MockAModule>().expect("register A");
        let built = block_on(kit.build()).expect("build should succeed");
        let b_cap = built.require::<MockBModule>().expect("B's cap");
        let a_cap = built.require::<MockAModule>().expect("A's cap");
        assert_eq!(b_cap.n, 42);
        assert_eq!(
            a_cap.b_val, 42,
            "A's cap must contain B's n=42 — require worked inside build callback"
        );
    }

    /// R-004 #3: Missing dependency → `TraitKitError::DependencyMissing`.
    /// Register only `MockAModule` (declares dep on `MockBModule`); `MockBModule`
    /// is intentionally unregistered. `graph.validate()` must reject before
    /// any `build_fn` runs.
    #[test]
    fn async_kit_di_missing_dependency_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockAModule>()
            .expect("register A only (B missing)");
        let err =
            block_on(kit.build()).expect_err("build must fail when declared dep is unregistered");
        assert!(
            matches!(
                err,
                TraitKitError::DependencyMissing {
                    module: "mock-a",
                    missing: "mock-b"
                }
            ),
            "expected DependencyMissing {{ module: \"mock-a\", missing: \"mock-b\" }}, got {err:?}"
        );
    }

    /// R-004 #4: 3-node cycle A→B→C→A → `TraitKitError::CycleDetected`.
    /// Distinct from the 2-node cycle test — exercises DFS cycle
    /// extraction on a longer ring.
    #[test]
    fn async_kit_di_three_node_cycle_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<MockCycleA3>().expect("register cycle A3");
        kit.register::<MockCycleB3>().expect("register cycle B3");
        kit.register::<MockCycleC3>().expect("register cycle C3");
        let err = block_on(kit.build()).expect_err("build must fail on 3-node cycle");
        assert!(
            matches!(err, TraitKitError::CycleDetected { .. }),
            "expected CycleDetected for 3-node cycle, got {err:?}"
        );
    }

    /// R-004 #5: Transitive chain A→B→C. C built first (order=1), B second
    /// (order=2), A third (order=3). A's `require::<MockChainBModule>()`
    /// succeeds, B's `require::<MockCModule>()` succeeds. A's cap contains
    /// C's v=100 transitively — proves DI propagates through the chain.
    #[test]
    fn async_kit_di_transitive_dependency_chain() {
        let mut kit = AsyncKit::new();
        kit.set_config(Arc::new(AtomicUsize::new(0)));
        kit.register::<MockCModule>().expect("register C");
        kit.register::<MockChainBModule>()
            .expect("register chain-B");
        kit.register::<MockChainAModule>()
            .expect("register chain-A");
        let built = block_on(kit.build()).expect("build should succeed");

        let c_cap = built.require::<MockCModule>().expect("C's cap");
        let b_cap = built.require::<MockChainBModule>().expect("chain-B's cap");
        let a_cap = built.require::<MockChainAModule>().expect("chain-A's cap");

        // Topological order: C=1, B=2, A=3.
        assert_eq!(c_cap.build_order, 1, "C should be built first");
        assert_eq!(b_cap.build_order, 2, "B should be built second");
        assert_eq!(a_cap.build_order, 3, "A should be built third");

        // DI propagation: A.b_val ← B.c_val ← C.v.
        assert_eq!(c_cap.v, 100);
        assert_eq!(
            b_cap.c_val, 100,
            "B's cap must contain C's v=100 — require::<MockCModule>() worked in B's build"
        );
        assert_eq!(
            a_cap.b_val, 100,
            "A's cap must transitively contain C's v=100 — transitive DI worked"
        );
    }

    /// Trigger `From<TraitKitError> for MockError` by having a module
    /// require an unregistered module during build.
    #[test]
    fn async_from_trait_kit_error_for_mock_error() {
        struct RequireMissingModule;
        impl ModuleMeta for RequireMissingModule {
            const NAME: &'static str = "require-missing";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for RequireMissingModule {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move {
                    // This require will fail — MockBModule is not registered.
                    // The `?` triggers From<TraitKitError> for MockError.
                    let _b: Arc<Bcap> = kit.require::<MockBModule>()?;
                    Ok(Arc::new(()))
                })
            }
        }

        let mut kit = AsyncKit::new();
        kit.register::<RequireMissingModule>().unwrap();
        let result = block_on(kit.build());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::BuildFailed { .. }
        ));
    }

    // Direct build calls for error-path fixtures to cover their build fns.

    #[test]
    fn async_mock_missing_dep_module_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockMissingDepModule::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_a_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleA::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_b_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleB::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_a3_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleA3::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_b3_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleB3::build(&kit));
        assert!(result.is_ok());
    }

    #[test]
    fn async_mock_cycle_c3_build_succeeds_directly() {
        let kit = AsyncKit::new();
        let result = block_on(MockCycleC3::build(&kit));
        assert!(result.is_ok());
    }
}

// ─── Feature-gated async integration tests ────────────────────────────────

#[cfg(all(test, feature = "lifecycle"))]
mod async_lifecycle_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::core::lifecycle::AsyncLifecycle;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ASYNC_LC_READY: AtomicUsize = AtomicUsize::new(0);

    struct AsyncLcModule;
    impl ModuleMeta for AsyncLcModule {
        const NAME: &'static str = "async-lc";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncLcModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }
    impl AsyncLifecycle for AsyncLcModule {
        fn on_ready<'a>(
            _kit: &'a AsyncKit<Ready>,
        ) -> Pin<Box<dyn Future<Output = Result<(), MockError>> + Send + 'a>> {
            Box::pin(async {
                ASYNC_LC_READY.fetch_add(1, Ordering::SeqCst);
                Ok(())
            })
        }
    }

    #[test]
    fn async_lifecycle_on_ready_called() {
        let before = ASYNC_LC_READY.load(Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<AsyncLcModule>().unwrap();
        kit.register_lifecycle::<AsyncLcModule>();
        let _built = block_on(kit.build()).unwrap();
        let after = ASYNC_LC_READY.load(Ordering::SeqCst);
        assert!(
            after > before,
            "on_ready should have been called at least once: before={before}, after={after}"
        );
    }

    // --- async shutdown: `shutdown_async` runs `AsyncLifecycle::on_shutdown` ---

    static ASYNC_LC_SHUTDOWN: AtomicUsize = AtomicUsize::new(0);

    struct AsyncShutdownLcModule;
    impl ModuleMeta for AsyncShutdownLcModule {
        const NAME: &'static str = "async-shutdown-lc";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncShutdownLcModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }
    impl AsyncLifecycle for AsyncShutdownLcModule {
        fn on_shutdown<'a>(_cap: &'a Arc<()>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            Box::pin(async {
                ASYNC_LC_SHUTDOWN.fetch_add(1, Ordering::SeqCst);
            })
        }
    }

    /// FIX 回归钉子：此前注册的 shutdown 回调是同步闭包，`on_shutdown`
    /// 返回的 Future 从未被执行（async 清理被静默跳过），且全库无 async
    /// shutdown API。现在 `shutdown_async()` 逐个 await 异步钩子（计数恰好
    /// +1）。
    #[test]
    fn async_shutdown_async_runs_on_shutdown_hook() {
        let before = ASYNC_LC_SHUTDOWN.load(Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<AsyncShutdownLcModule>().unwrap();
        kit.register_lifecycle::<AsyncShutdownLcModule>();
        let built = block_on(kit.build()).unwrap();

        block_on(built.shutdown_async());
        assert_eq!(
            ASYNC_LC_SHUTDOWN.load(Ordering::SeqCst),
            before + 1,
            "shutdown_async() 应恰好执行一次 async on_shutdown"
        );
    }

    // ── 逆拓扑关闭顺序的回归钉子见文件后部 `async_shutdown_async_runs_in_
    //    reverse_topological_order`（MED-004）。──

    /// `shutdown_async()` 是 one-shot：async hook registry 被 drain，二次调用
    /// 为 no-op，`on_shutdown` 恰好执行一次。（原测试直插私有字段
    /// `shutdown_callbacks` 钉 sync `shutdown()` 的 one-shot 行为；sync 入口
    /// 已随 MED-003 删除——async 清理唯一入口是 `shutdown_async()`。）
    #[test]
    fn async_shutdown_async_is_one_shot_second_call_is_noop() {
        // 测试内专用计数器：避免共享 static 计数器受其他测试干扰，
        // "恰好一次"断言才能成立。
        static ONE_SHOT_COUNT: AtomicUsize = AtomicUsize::new(0);

        struct OneShotModule;
        impl ModuleMeta for OneShotModule {
            const NAME: &'static str = "async-shutdown-one-shot";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for OneShotModule {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move { Ok(Arc::new(())) })
            }
        }
        impl AsyncLifecycle for OneShotModule {
            fn on_shutdown<'a>(_cap: &'a Arc<()>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
                Box::pin(async {
                    ONE_SHOT_COUNT.fetch_add(1, Ordering::SeqCst);
                })
            }
        }

        let mut kit = AsyncKit::new();
        kit.register::<OneShotModule>().unwrap();
        kit.register_lifecycle::<OneShotModule>();
        let built = block_on(kit.build()).unwrap();

        block_on(built.shutdown_async());
        block_on(built.shutdown_async());
        assert_eq!(
            ONE_SHOT_COUNT.load(Ordering::SeqCst),
            1,
            "第二次 shutdown_async() 必须 no-op（钩子已 drain），on_shutdown 恰好执行一次"
        );
    }

    /// MED-004 回归钉子：钩子在 `build()` 时按依赖图拓扑索引稳定排序，
    /// `shutdown_async()` 逆序 drain 后依赖者（dependent）先于被依赖者
    /// （dep）关闭——文档承诺的 "reverse topological order" 由此为真。
    #[test]
    fn async_shutdown_async_runs_in_reverse_topological_order() {
        use std::sync::Mutex;

        static SHUTDOWN_ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

        struct OrderDepModule;
        impl ModuleMeta for OrderDepModule {
            const NAME: &'static str = "shutdown-order-dep";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for OrderDepModule {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move { Ok(Arc::new(())) })
            }
        }
        impl AsyncLifecycle for OrderDepModule {
            fn on_shutdown<'a>(_cap: &'a Arc<()>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
                Box::pin(async {
                    SHUTDOWN_ORDER.lock().expect("lock poisoned").push("dep");
                })
            }
        }

        /// 依赖 `OrderDepModule` 的模块（依赖者）。
        struct OrderDependentModule;
        impl ModuleMeta for OrderDependentModule {
            const NAME: &'static str = "shutdown-order-dependent";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] =
                    &[("shutdown-order-dep", TypeId::of::<OrderDepModule>())];
                DEPS
            }
        }
        impl AsyncAutoBuilder for OrderDependentModule {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move { Ok(Arc::new(())) })
            }
        }
        impl AsyncLifecycle for OrderDependentModule {
            fn on_shutdown<'a>(_cap: &'a Arc<()>) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
                Box::pin(async {
                    SHUTDOWN_ORDER
                        .lock()
                        .expect("lock poisoned")
                        .push("dependent");
                })
            }
        }

        SHUTDOWN_ORDER.lock().expect("lock poisoned").clear();
        let mut kit = AsyncKit::new();
        kit.register::<OrderDepModule>().unwrap();
        kit.register::<OrderDependentModule>().unwrap();
        // 注册顺序故意与拓扑序相反（dependent 先、dep 后）：若实现仍是
        // 旧的"逆注册序"，结果为 [dep, dependent]，本测试失败。
        kit.register_lifecycle::<OrderDependentModule>();
        kit.register_lifecycle::<OrderDepModule>();
        let built = block_on(kit.build()).unwrap();
        block_on(built.shutdown_async());

        let order = SHUTDOWN_ORDER.lock().expect("lock poisoned").clone();
        assert_eq!(
            order,
            vec!["dependent", "dep"],
            "依赖者（dependent）必须先于被依赖者（dep）执行 on_shutdown（逆拓扑序）"
        );
    }
}

#[cfg(all(test, feature = "health"))]
mod async_health_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::core::health::{AsyncHealthCheck, HealthStatus};
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct AsyncHcCap {
        val: i32,
    }

    struct AsyncHcModule;
    impl ModuleMeta for AsyncHcModule {
        const NAME: &'static str = "async-hc";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncHcModule {
        type Capability = Arc<AsyncHcCap>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncHcCap>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(AsyncHcCap { val: 42 })) })
        }
    }
    impl AsyncHealthCheck for AsyncHcModule {
        fn check(cap: &Arc<AsyncHcCap>) -> HealthStatus {
            if cap.val > 0 {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy {
                    detail: "zero".into(),
                }
            }
        }
    }

    #[test]
    fn async_health_check_queryable() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHcModule>().unwrap();
        kit.register_health_check::<AsyncHcModule>();
        let built = block_on(kit.build()).unwrap();
        let status = built.health_check::<AsyncHcModule>().unwrap();
        assert_eq!(status, HealthStatus::Healthy);
    }

    #[test]
    fn async_health_report_returns_all() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHcModule>().unwrap();
        kit.register_health_check::<AsyncHcModule>();
        let built = block_on(kit.build()).unwrap();
        let report = built.health_report();
        assert_eq!(report.len(), 1);
        assert_eq!(report[0].0, "async-hc");
    }

    #[test]
    fn async_health_check_unregistered_returns_error() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncHcModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let err = built.health_check::<AsyncHcModule>().unwrap_err();
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }
}

#[cfg(all(test, feature = "observer"))]
mod async_observability_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::core::observer::BuildObserver;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    struct AsyncObs {
        start: Arc<AtomicUsize>,
        built: Arc<AtomicUsize>,
    }
    impl BuildObserver for AsyncObs {
        fn on_module_start(&self, _: &'static str) {
            self.start.fetch_add(1, Ordering::SeqCst);
        }
        fn on_module_built(&self, _: &'static str, _: Duration) {
            self.built.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct AsyncObsModule;
    impl ModuleMeta for AsyncObsModule {
        const NAME: &'static str = "async-obs";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncObsModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_observer_callbacks_fired() {
        let start = Arc::new(AtomicUsize::new(0));
        let built_count = Arc::new(AtomicUsize::new(0));
        let obs = Arc::new(AsyncObs {
            start: Arc::clone(&start),
            built: Arc::clone(&built_count),
        });
        let mut kit = AsyncKit::new();
        kit.with_observer(obs);
        kit.register::<AsyncObsModule>().unwrap();
        block_on(kit.build()).unwrap();
        assert_eq!(start.load(Ordering::SeqCst), 1);
        assert_eq!(built_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn async_observer_on_build_error_called() {
        struct AsyncFailObs {
            errors: Arc<AtomicUsize>,
        }
        impl BuildObserver for AsyncFailObs {
            fn on_build_error(&self, _: &'static str, _: &TraitKitError) {
                self.errors.fetch_add(1, Ordering::SeqCst);
            }
        }

        struct AsyncFailBuildModule;
        impl ModuleMeta for AsyncFailBuildModule {
            const NAME: &'static str = "async-fail-build";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                &[]
            }
        }
        impl AsyncAutoBuilder for AsyncFailBuildModule {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move { Err(MockError::Failed("intentional".into())) })
            }
        }

        let errors = Arc::new(AtomicUsize::new(0));
        let obs = Arc::new(AsyncFailObs {
            errors: Arc::clone(&errors),
        });
        let mut kit = AsyncKit::new();
        kit.with_observer(obs);
        kit.register::<AsyncFailBuildModule>().unwrap();
        let result = block_on(kit.build());
        assert!(result.is_err());
        assert_eq!(
            errors.load(Ordering::SeqCst),
            1,
            "on_build_error should fire"
        );
    }
}

#[cfg(test)]
mod async_factory_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static ASYNC_FACTORY_COUNT: AtomicUsize = AtomicUsize::new(0);

    struct AsyncFactoryModule;
    impl ModuleMeta for AsyncFactoryModule {
        const NAME: &'static str = "async-factory";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncFactoryModule {
        type Capability = Arc<AtomicUsize>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AtomicUsize>, MockError>> + Send + 'a>>
        {
            Box::pin(async move {
                let n = ASYNC_FACTORY_COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(Arc::new(AtomicUsize::new(n)))
            })
        }
    }

    #[test]
    fn async_factory_creates_new_instances() {
        ASYNC_FACTORY_COUNT.store(0, Ordering::SeqCst);
        let mut kit = AsyncKit::new();
        kit.register::<AsyncFactoryModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let factory = built.factory::<AsyncFactoryModule>();
        let cap1 = block_on(factory());
        let cap2 = block_on(factory());
        assert!(cap1.is_ok());
        assert!(cap2.is_ok());
        assert_ne!(
            cap1.unwrap().load(Ordering::SeqCst),
            cap2.unwrap().load(Ordering::SeqCst)
        );
    }
}

#[cfg(all(test, feature = "scope"))]
mod async_scope_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    struct AsyncScopeMockModule;
    impl ModuleMeta for AsyncScopeMockModule {
        const NAME: &'static str = "async-scope-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncScopeMockModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_create_scope_returns_empty() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncScopeMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let scope = built.create_scope();
        assert!(!scope.contains::<AsyncScopeMockModule>());
    }
}

#[cfg(test)]
mod async_conditional_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    struct AsyncCondMockModule;
    impl ModuleMeta for AsyncCondMockModule {
        const NAME: &'static str = "async-cond-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncCondMockModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_register_if_true() {
        let mut kit = AsyncKit::new();
        let registered = kit.register_if::<AsyncCondMockModule>(|_| true).unwrap();
        assert!(registered);
        let built = block_on(kit.build()).unwrap();
        assert!(built.contains::<AsyncCondMockModule>());
    }

    #[test]
    fn async_register_if_false() {
        let mut kit = AsyncKit::new();
        let registered = kit.register_if::<AsyncCondMockModule>(|_| false).unwrap();
        assert!(!registered);
        let built = block_on(kit.build()).unwrap();
        assert!(!built.contains::<AsyncCondMockModule>());
    }
}

#[cfg(all(test, feature = "decorator"))]
mod async_decorator_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    struct AsyncDecCap {
        val: String,
    }

    struct AsyncDecModule;
    impl ModuleMeta for AsyncDecModule {
        const NAME: &'static str = "async-dec";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncDecModule {
        type Capability = Arc<AsyncDecCap>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<AsyncDecCap>, MockError>> + Send + 'a>>
        {
            Box::pin(async move { Ok(Arc::new(AsyncDecCap { val: "base".into() })) })
        }
    }

    #[test]
    fn async_decorate_registers() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncDecModule>().unwrap();
        kit.decorate::<AsyncDecModule>(|cap| {
            Arc::new(AsyncDecCap {
                val: format!("{}+wrapped", cap.val),
            })
        });
        let built = block_on(kit.build()).unwrap();
        let cap = built.require::<AsyncDecModule>().unwrap();
        assert!(!cap.val.is_empty());
    }
}

// ─── AsyncKit<Ready> surface tests ─────────────────────────────────────

#[cfg(all(test, feature = "async"))]
mod async_ready_tests {
    use super::*;
    use crate::core::ModuleMeta;
    use crate::test_helpers::{MockError, block_on};
    use std::sync::Arc;

    struct AsyncReadyMockModule;
    impl ModuleMeta for AsyncReadyMockModule {
        const NAME: &'static str = "async-ready-mock";
        fn dependencies() -> &'static [(&'static str, TypeId)] {
            &[]
        }
    }
    impl AsyncAutoBuilder for AsyncReadyMockModule {
        type Capability = Arc<()>;
        type Error = MockError;
        fn build<'a>(
            _kit: &'a AsyncKit,
        ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
            Box::pin(async move { Ok(Arc::new(())) })
        }
    }

    #[test]
    fn async_debug_unbuilt_format() {
        let kit = AsyncKit::new();
        let debug = format!("{kit:?}");
        assert!(debug.contains("AsyncKit<Unbuilt>"));
    }

    #[test]
    fn async_debug_ready_format() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncReadyMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let debug = format!("{built:?}");
        assert!(debug.contains("AsyncKit<Ready>"));
    }

    #[test]
    fn async_default_creates_empty() {
        let kit = AsyncKit::default();
        let built = block_on(kit.build()).unwrap();
        assert_eq!(built.graph.entries().len(), 0);
    }

    #[test]
    fn async_graph_dot_works() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncReadyMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let dot = built.graph_dot();
        assert!(dot.contains("digraph"));
    }

    #[test]
    fn async_graph_mermaid_works() {
        let mut kit = AsyncKit::new();
        kit.register::<AsyncReadyMockModule>().unwrap();
        let built = block_on(kit.build()).unwrap();
        let mermaid = built.graph_mermaid();
        assert!(mermaid.contains("graph TD"));
    }

    #[test]
    fn async_config_missing_returns_error() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).unwrap();
        let err = built.config::<i32>().unwrap_err();
        assert!(matches!(err, TraitKitError::MissingConfig { .. }));
    }

    #[test]
    fn async_build_missing_dep_returns_error() {
        struct AsyncNeedsDep;
        impl ModuleMeta for AsyncNeedsDep {
            const NAME: &'static str = "async-needs-dep";
            fn dependencies() -> &'static [(&'static str, TypeId)] {
                static DEPS: &[(&str, TypeId)] = &[("dep", TypeId::of::<AsyncReadyMockModule>())];
                DEPS
            }
        }
        impl AsyncAutoBuilder for AsyncNeedsDep {
            type Capability = Arc<()>;
            type Error = MockError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Arc<()>, MockError>> + Send + 'a>> {
                Box::pin(async move { Ok(Arc::new(())) })
            }
        }

        let mut kit = AsyncKit::new();
        kit.register::<AsyncNeedsDep>().unwrap();
        let result = block_on(kit.build());
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            TraitKitError::DependencyMissing { .. }
        ));
    }

    #[test]
    fn async_optional_returns_none_for_unbuilt() {
        let kit = AsyncKit::new();
        let built = block_on(kit.build()).unwrap();
        assert!(built.optional::<AsyncReadyMockModule>().is_none());
    }
}

// ─── Config inheritance tests (async) ───────────────────────────────────

#[cfg(all(test, feature = "confers"))]
mod async_config_inheritance_tests {
    use super::*;
    use crate::kit::{ConfigInherit, ModuleConfig, SharedConfig};
    use crate::test_helpers::block_on;

    // ── Test fixtures ──

    #[derive(Clone, Debug, PartialEq)]
    struct AsyncDbConfig {
        host: String,
        port: u16,
        max_connections: u32,
    }

    #[derive(Clone, Default)]
    struct AsyncDbConfigOverride {
        host: Option<String>,
        port: Option<u16>,
        max_connections: Option<u32>,
    }

    impl ConfigInherit for AsyncDbConfig {
        type Override = AsyncDbConfigOverride;
        fn apply_override(&mut self, ovr: &Self::Override) {
            if let Some(ref h) = ovr.host {
                self.host.clone_from(h);
            }
            if let Some(ref p) = ovr.port {
                self.port = *p;
            }
            if let Some(ref m) = ovr.max_connections {
                self.max_connections = *m;
            }
        }
    }

    impl ModuleConfig for AsyncDbConfig {
        const PATH: &'static str = "config/db.toml";
        fn default_value() -> Self {
            Self {
                host: "localhost".into(),
                port: 3306,
                max_connections: 10,
            }
        }
    }

    impl SharedConfig for AsyncDbConfig {
        fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value> {
            let mut map = serde_json::Map::new();
            map.insert("host".into(), serde_json::Value::String(self.host.clone()));
            map.insert("port".into(), serde_json::json!(self.port));
            map
        }
        fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>) {
            if let Some(v) = shared.get("host")
                && let Ok(s) = serde_json::from_value(v.clone())
            {
                self.host = s;
            }
            if let Some(v) = shared.get("port")
                && let Ok(n) = serde_json::from_value(v.clone())
            {
                self.port = n;
            }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    struct AsyncAppConfig {
        host: String,
        port: u16,
        app_name: String,
    }

    impl SharedConfig for AsyncAppConfig {
        fn extract_shared(&self) -> serde_json::Map<String, serde_json::Value> {
            let mut map = serde_json::Map::new();
            map.insert("host".into(), serde_json::Value::String(self.host.clone()));
            map.insert("port".into(), serde_json::json!(self.port));
            map
        }
        fn inject_shared(&mut self, shared: &serde_json::Map<String, serde_json::Value>) {
            if let Some(v) = shared.get("host")
                && let Ok(s) = serde_json::from_value(v.clone())
            {
                self.host = s;
            }
            if let Some(v) = shared.get("port")
                && let Ok(n) = serde_json::from_value(v.clone())
            {
                self.port = n;
            }
        }
    }

    // ── Tests ──

    #[test]
    fn async_populate_defaults_fills_and_skips() {
        let kit = AsyncKit::new();
        assert!(kit.populate_defaults::<AsyncDbConfig>());
        assert!(!kit.populate_defaults::<AsyncDbConfig>());

        let cfg: AsyncDbConfig = kit.config().unwrap();
        assert_eq!(cfg.host, "localhost");
        assert_eq!(cfg.port, 3306);
        assert_eq!(cfg.max_connections, 10);
    }

    #[test]
    fn async_merge_config_applies_override() {
        let kit = AsyncKit::new();
        kit.set_config(AsyncDbConfig {
            host: "db.example.com".into(),
            port: 5432,
            max_connections: 50,
        });

        kit.merge_config::<AsyncDbConfig>(AsyncDbConfigOverride {
            host: Some("override.example.com".into()),
            port: None,
            max_connections: Some(100),
        });

        let cfg: AsyncDbConfig = kit.config().unwrap();
        assert_eq!(cfg.host, "override.example.com");
        assert_eq!(cfg.port, 5432); // unchanged
        assert_eq!(cfg.max_connections, 100);
    }

    #[test]
    fn async_extract_inject_shared_flow() {
        let kit = AsyncKit::new();

        // Set DbConfig with specific host/port
        kit.set_config(AsyncDbConfig {
            host: "shared-host".into(),
            port: 9999,
            max_connections: 20,
        });

        // Set AppConfig with different host/port
        kit.set_config(AsyncAppConfig {
            host: "app-host".into(),
            port: 8080,
            app_name: "my-app".into(),
        });

        // Extract from DbConfig → overlay
        kit.extract_shared::<AsyncDbConfig>();

        // Inject overlay → AppConfig (overrides host/port)
        kit.inject_shared::<AsyncAppConfig>();

        let app_cfg: AsyncAppConfig = kit.config().unwrap();
        assert_eq!(app_cfg.host, "shared-host");
        assert_eq!(app_cfg.port, 9999);
        assert_eq!(app_cfg.app_name, "my-app"); // unchanged
    }

    #[test]
    fn async_config_inheritance_survives_build() {
        let kit = AsyncKit::new();
        kit.set_config(AsyncDbConfig {
            host: "prod-db".into(),
            port: 5432,
            max_connections: 100,
        });
        kit.extract_shared::<AsyncDbConfig>();

        let built = block_on(kit.build()).unwrap();

        // shared_fields should be accessible on Ready kit
        let overlay = built.confers.shared_fields.read().unwrap().clone();
        assert_eq!(
            overlay.get("host"),
            Some(&serde_json::Value::String("prod-db".into()))
        );
    }

    // ── no-op boundary tests ──

    #[test]
    fn async_merge_config_noop_when_missing() {
        let kit = AsyncKit::new();
        kit.merge_config::<AsyncDbConfig>(AsyncDbConfigOverride {
            host: Some("x".into()),
            ..Default::default()
        });
        assert!(!kit.configs.contains::<AsyncDbConfig>());
    }

    #[test]
    fn async_extract_shared_noop_when_config_missing() {
        let kit = AsyncKit::new();
        kit.extract_shared::<AsyncDbConfig>();
        let overlay = kit.confers.shared_fields.read().unwrap();
        assert!(overlay.is_empty());
    }

    #[test]
    fn async_inject_shared_noop_when_config_missing() {
        let kit = AsyncKit::new();
        kit.confers
            .shared_fields
            .write()
            .unwrap()
            .insert("host".into(), serde_json::json!("some-host"));
        kit.inject_shared::<AsyncDbConfig>();
        assert_eq!(kit.confers.shared_fields.read().unwrap().len(), 1);
    }

    // ── concurrent safety test ──

    #[test]
    fn async_shared_fields_concurrent_access() {
        use std::thread;

        let kit = Arc::new(AsyncKit::new());
        kit.set_config(AsyncDbConfig {
            host: "concurrent-host".into(),
            port: 1234,
            max_connections: 5,
        });

        let kit2 = Arc::clone(&kit);
        let kit3 = Arc::clone(&kit);

        // Thread 1: extract_shared
        let h1 = thread::spawn(move || {
            kit2.extract_shared::<AsyncDbConfig>();
        });

        // Thread 2: extract_shared (concurrent write to shared_fields)
        let h2 = thread::spawn(move || {
            kit3.set_config(AsyncAppConfig {
                host: "thread3-host".into(),
                port: 5678,
                app_name: "concurrent-app".into(),
            });
            kit3.extract_shared::<AsyncAppConfig>();
        });

        h1.join().unwrap();
        h2.join().unwrap();

        // Both extractions should have completed without panic
        let overlay = kit.confers.shared_fields.read().unwrap();
        assert!(overlay.contains_key("host"));
        assert!(overlay.contains_key("port"));
    }
}

#[cfg(all(test, feature = "async"))]
mod concurrency_tests {
    use super::{AsyncKit, BatchJoin};
    use crate::core::{AsyncAutoBuilder, ModuleMeta};
    use crate::error::TraitKitError;
    use crate::test_helpers::block_on;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// Future that stays Pending once (simulating an async I/O step), so the
    /// driver must interleave other futures around it.
    struct YieldOnce {
        yielded: bool,
    }
    impl Future for YieldOnce {
        type Output = ();
        fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<()> {
            if self.yielded {
                std::task::Poll::Ready(())
            } else {
                self.yielded = true;
                cx.waker().wake_by_ref();
                std::task::Poll::Pending
            }
        }
    }

    #[test]
    fn batch_join_respects_concurrency_limit() {
        // Drive 4 pending-once futures with limit 2: max observed in-flight = 2.
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut queued = std::collections::VecDeque::new();
        for i in 0u64..4 {
            let a = Arc::clone(&active);
            let m = Arc::clone(&max_active);
            queued.push_back((
                i,
                Box::pin(async move {
                    let now = a.fetch_add(1, Ordering::SeqCst) + 1;
                    m.fetch_max(now, Ordering::SeqCst);
                    YieldOnce { yielded: false }.await;
                    a.fetch_sub(1, Ordering::SeqCst);
                }) as Pin<Box<dyn Future<Output = ()> + Send>>,
            ));
        }
        let batch = BatchJoin {
            queued,
            active: Vec::new(),
            limit: 2,
            completed: Vec::new(),
        };
        let results = block_on(batch);
        assert_eq!(results.len(), 4);
        assert_eq!(
            max_active.load(Ordering::SeqCst),
            2,
            "no more than `limit` futures in flight"
        );
    }

    #[test]
    fn batch_join_unlimited_runs_all_concurrently() {
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut queued = std::collections::VecDeque::new();
        for i in 0u64..4 {
            let a = Arc::clone(&active);
            let m = Arc::clone(&max_active);
            queued.push_back((
                i,
                Box::pin(async move {
                    let now = a.fetch_add(1, Ordering::SeqCst) + 1;
                    m.fetch_max(now, Ordering::SeqCst);
                    YieldOnce { yielded: false }.await;
                    a.fetch_sub(1, Ordering::SeqCst);
                }) as Pin<Box<dyn Future<Output = ()> + Send>>,
            ));
        }
        let batch = BatchJoin {
            queued,
            active: Vec::new(),
            limit: usize::MAX,
            completed: Vec::new(),
        };
        let results = block_on(batch);
        assert_eq!(results.len(), 4);
        assert_eq!(max_active.load(Ordering::SeqCst), 4, "all four in flight");
    }

    #[test]
    fn async_kit_builds_independency_modules_concurrently() {
        // Three dependency-free modules with real (pending-once) build
        // futures sharing a global max-in-flight counter. With the default
        // unlimited limit all three overlap.
        static ACTIVE: AtomicUsize = AtomicUsize::new(0);
        static MAX_ACTIVE: AtomicUsize = AtomicUsize::new(0);

        macro_rules! make_concurrent_module {
            ($modname:ident) => {
                struct $modname;
                impl ModuleMeta for $modname {
                    const NAME: &'static str = stringify!($modname);
                }
                impl AsyncAutoBuilder for $modname {
                    type Capability = Arc<SlowCap>;
                    type Error = TraitKitError;
                    fn build<'a>(
                        _kit: &'a AsyncKit,
                    ) -> Pin<
                        Box<
                            dyn Future<Output = Result<Self::Capability, TraitKitError>>
                                + Send
                                + 'a,
                        >,
                    > {
                        Box::pin(async move {
                            let now = ACTIVE.fetch_add(1, Ordering::SeqCst) + 1;
                            MAX_ACTIVE.fetch_max(now, Ordering::SeqCst);
                            YieldOnce { yielded: false }.await;
                            ACTIVE.fetch_sub(1, Ordering::SeqCst);
                            Ok(Arc::new(SlowCap))
                        })
                    }
                }
            };
        }
        make_concurrent_module!(ConcA);
        make_concurrent_module!(ConcB);
        make_concurrent_module!(ConcC);

        let mut kit = AsyncKit::new();
        kit.register::<ConcA>().expect("a");
        kit.register::<ConcB>().expect("b");
        kit.register::<ConcC>().expect("c");
        let ready = block_on(kit.build()).expect("build ok");
        assert!(
            ready.contains::<ConcA>() && ready.contains::<ConcB>() && ready.contains::<ConcC>()
        );
        assert_eq!(
            MAX_ACTIVE.load(Ordering::SeqCst),
            3,
            "dependency-free modules build concurrently (all 3 overlapped)"
        );
    }

    #[derive(Debug)]
    struct SlowCap;

    #[test]
    fn dependent_modules_still_build_in_dependency_order() {
        static ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

        struct DepLeaf;
        impl ModuleMeta for DepLeaf {
            const NAME: &'static str = "dep-leaf";
        }
        impl AsyncAutoBuilder for DepLeaf {
            type Capability = Arc<SlowCap>;
            type Error = TraitKitError;
            fn build<'a>(
                _kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
            {
                Box::pin(async move {
                    ORDER.lock().unwrap().push("dep-leaf");
                    Ok(Arc::new(SlowCap))
                })
            }
        }
        struct DepTop;
        impl ModuleMeta for DepTop {
            const NAME: &'static str = "dep-top";
            fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
                static DEPS: &[(&str, std::any::TypeId)] = &[(
                    <DepLeaf as ModuleMeta>::NAME,
                    std::any::TypeId::of::<DepLeaf>(),
                )];
                DEPS
            }
        }
        impl AsyncAutoBuilder for DepTop {
            type Capability = Arc<SlowCap>;
            type Error = TraitKitError;
            fn build<'a>(
                kit: &'a AsyncKit,
            ) -> Pin<Box<dyn Future<Output = Result<Self::Capability, TraitKitError>> + Send + 'a>>
            {
                Box::pin(async move {
                    kit.require::<DepLeaf>()
                        .map_err(|e| TraitKitError::BuildFailed {
                            context: "dep-top".into(),
                            source: Box::new(e),
                        })?;
                    ORDER.lock().unwrap().push("dep-top");
                    Ok(Arc::new(SlowCap))
                })
            }
        }

        let mut kit = AsyncKit::new();
        kit.register::<DepTop>().expect("top");
        kit.register::<DepLeaf>().expect("leaf");
        let ready = block_on(kit.build()).expect("build ok");
        assert!(ready.contains::<DepTop>());
        let order = ORDER.lock().unwrap();
        let leaf_pos = order
            .iter()
            .position(|n| *n == "dep-leaf")
            .expect("leaf ran");
        let top_pos = order.iter().position(|n| *n == "dep-top").expect("top ran");
        assert!(
            leaf_pos < top_pos,
            "dependency must complete first: {order:?}"
        );
    }
}
