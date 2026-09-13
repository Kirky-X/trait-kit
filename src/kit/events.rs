// Copyright (c) 2026 Kirky.X
// SPDX-License-Identifier: MIT
//! Runtime event bus port.
//!
//! A single typed channel for the lifecycle events that previously lived in
//! disconnected mechanisms (build observers, health pull-model, reload
//! subscriptions): [`KitEvent`] covers module construction, health flips, and
//! configuration changes. The bus is a port (same pattern as
//! [`super::ports::MetricsPort`]): `Kit` publishes at lifecycle checkpoints
//! into the injected [`EventBus`], defaulting to a no-op so the zero-bus
//! build is zero-cost.
//!
//! Always available — no feature gate (zero-cost default via `NoOpEventBus`).

use std::sync::Mutex;

// ─── KitEvent ───────────────────────────────────────────────────────────────

/// Typed lifecycle event published by `Kit` / `AsyncKit`.
#[derive(Debug, Clone, PartialEq)]
pub enum KitEvent {
    /// A module's `build_fn` completed successfully during `build()`.
    ModuleBuilt {
        /// Module name (`ModuleMeta::NAME`).
        module: &'static str,
        /// Construction time in microseconds.
        elapsed_us: u64,
    },

    /// A module's health status was sampled.
    HealthChanged {
        /// Module name.
        module: &'static str,
        /// Flat status name (`healthy` / `degraded` / `unhealthy`).
        status: &'static str,
        /// Detail for degraded / unhealthy states.
        detail: Option<String>,
    },

    /// A configuration value changed (set / reload / restore).
    ConfigChanged {
        /// Configuration key (type name for typed configs).
        key: String,
        /// Short summary of the change (e.g. `"updated"`, `"restored snapshot"`).
        summary: String,
    },
}

impl KitEvent {
    /// Discriminant name of the event (`"module_built"` / `"health_changed"` /
    /// `"config_changed"`) — handy for logging and filtering.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::ModuleBuilt { .. } => "module_built",
            Self::HealthChanged { .. } => "health_changed",
            Self::ConfigChanged { .. } => "config_changed",
        }
    }
}

// ─── EventBus port ──────────────────────────────────────────────────────────

/// Runtime event bus port.
///
/// Implemented by [`MemoryEventBus`] (in-memory subscriber list) and
/// [`NoOpEventBus`] (the default — drops everything, no allocation). Custom
/// buses can forward to tracing/metrics/external systems.
pub trait EventBus: Send + Sync + 'static {
    /// Publish one event. Must not panic; must not block indefinitely.
    fn publish(&self, event: KitEvent);
}

/// In-memory [`EventBus`]: synchronous fan-out to registered subscribers.
///
/// A failing (panicking) subscriber is isolated: the panic is caught so other
/// subscribers still receive the event and `publish` never unwinds into the
/// Kit's build path.
/// 事件回调的克隆快照（`publish` 在锁外扇出，见其文档）
type SubscriberSlot = std::sync::Arc<dyn Fn(&KitEvent) + Send + Sync>;

#[derive(Default)]
pub struct MemoryEventBus {
    subscribers: Mutex<Vec<SubscriberSlot>>,
}

impl MemoryEventBus {
    /// Create an empty bus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a subscriber. Receives every published event, in publish
    /// order, synchronously inside `publish`.
    ///
    /// # Panics
    ///
    /// Panics if the subscriber mutex is poisoned (a subscriber panicked
    /// while holding it, e.g. inside another callback).
    pub fn subscribe(&self, subscriber: impl Fn(&KitEvent) + Send + Sync + 'static) {
        self.subscribers
            .lock()
            .expect("event bus lock")
            .push(std::sync::Arc::new(subscriber));
    }
}

impl EventBus for MemoryEventBus {
    fn publish(&self, event: KitEvent) {
        // Snapshot the subscriber list under the lock, then release it before
        // invoking any callback: a subscriber that re-enters `publish` or
        // `subscribe` on the same thread would otherwise deadlock on the
        // non-reentrant `Mutex`.
        let snapshot = self.subscribers.lock().expect("event bus lock").clone();
        for subscriber in &snapshot {
            // One misbehaving subscriber must not block the rest, and a
            // panicking subscriber must not unwind into Kit internals.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                subscriber(&event);
            }));
        }
    }
}

/// The default no-op bus: every publish is a no-op and the type is zero-sized.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoOpEventBus;

impl EventBus for NoOpEventBus {
    fn publish(&self, _event: KitEvent) {}
}

/// Optional bus handle stored in `Kit` / `AsyncKit` (`None` = no-op default).
pub type OptionalEventBus = Option<std::sync::Arc<dyn EventBus>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn memory_bus_fans_out_to_all_subscribers_in_order() {
        let bus = MemoryEventBus::new();
        let log: Arc<Mutex<Vec<&'static str>>> = Arc::new(Mutex::new(Vec::new()));
        let for_module_sub = Arc::clone(&log);
        let for_config_sub = Arc::clone(&log);

        bus.subscribe(move |event| {
            if let KitEvent::ModuleBuilt { module, .. } = event {
                for_module_sub.lock().unwrap().push(*module);
            }
        });
        bus.subscribe(move |event| {
            if let KitEvent::ConfigChanged { key, .. } = event {
                let _ = key;
                for_config_sub.lock().unwrap().push("config");
            }
        });

        bus.publish(KitEvent::ModuleBuilt {
            module: "a",
            elapsed_us: 1,
        });
        bus.publish(KitEvent::ConfigChanged {
            key: "k".into(),
            summary: "set".into(),
        });

        let seen = log.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(*seen, vec!["a", "config"]);
    }

    #[test]
    fn memory_bus_isolates_panicking_subscriber() {
        let bus = MemoryEventBus::new();
        let count = Arc::new(AtomicUsize::new(0));
        let count2 = Arc::clone(&count);

        bus.subscribe(move |_event| panic!("subscriber bug"));
        bus.subscribe(move |_event| {
            count2.fetch_add(1, Ordering::SeqCst);
        });

        bus.publish(KitEvent::ConfigChanged {
            key: "k".into(),
            summary: "set".into(),
        });
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "second subscriber still ran"
        );
    }

    #[test]
    fn memory_bus_survives_reentrant_publish_and_subscribe() {
        // 回归：publish 持锁回调时，同线程内再入 publish/subscribe 会在
        // 不可重入 Mutex 上死锁；现在先快照订阅者再释放锁。
        let bus = Arc::new(MemoryEventBus::new());
        let count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&count);

        let inner_bus = Arc::clone(&bus);
        bus.subscribe(move |event| {
            if let KitEvent::ModuleBuilt { .. } = event {
                // Re-entrant publish + subscribe on the same thread.
                inner_bus.publish(KitEvent::ConfigChanged {
                    key: "inner".into(),
                    summary: "re-entry".into(),
                });
                inner_bus.subscribe(|_event| {});
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });

        bus.publish(KitEvent::ModuleBuilt {
            module: "a",
            elapsed_us: 1,
        });
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "outer subscriber completed without deadlocking"
        );
    }

    #[test]
    fn noop_bus_publishes_without_effect() {
        // Compile-check: NoOpEventBus is Send + Sync + zero-sized.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoOpEventBus>();

        NoOpEventBus.publish(KitEvent::ModuleBuilt {
            module: "x",
            elapsed_us: 0,
        });
        assert_eq!(std::mem::size_of::<NoOpEventBus>(), 0);
    }

    #[test]
    fn event_kind_names_are_stable() {
        assert_eq!(
            KitEvent::ModuleBuilt {
                module: "m",
                elapsed_us: 1
            }
            .kind(),
            "module_built"
        );
        assert_eq!(
            KitEvent::HealthChanged {
                module: "m",
                status: "healthy",
                detail: None
            }
            .kind(),
            "health_changed"
        );
        assert_eq!(
            KitEvent::ConfigChanged {
                key: "k".into(),
                summary: "s".into()
            }
            .kind(),
            "config_changed"
        );
    }
}
