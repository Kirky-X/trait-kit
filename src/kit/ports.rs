// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Observation ports — injectable metrics and logging abstractions.
//!
//! `MetricsPort` and `LogPort` are trait-based extension points that allow
//! downstream crates to plug in their own metrics/log backends. Both ship
//! with `NoOp` default implementations that compile to zero overhead when
//! not injected (`Option<Arc<dyn XPort>>` defaults to `None`).

use std::sync::Arc;

/// Metrics observation port.
///
/// Implementations record counter/gauge/histogram values. The trait is
/// object-safe (`Send + Sync`) so it can be stored as `Arc<dyn MetricsPort>`.
///
/// # `NoOp` default
///
/// [`NoOpMetricsPort`] discards all recordings. Use it as the default when
/// no metrics backend is configured.
pub trait MetricsPort: Send + Sync + 'static {
    /// Increment a counter by `value`.
    fn record_counter(&self, name: &str, value: u64);
    /// Set a gauge to `value`.
    fn record_gauge(&self, name: &str, value: f64);
    /// Record a histogram observation of `value`.
    fn record_histogram(&self, name: &str, value: f64);
}

/// Structured log observation port.
///
/// Implementations receive structured log records from Kit/AsyncKit
/// lifecycle events. The trait is object-safe (`Send + Sync`).
///
/// # `NoOp` default
///
/// [`NoOpLogPort`] discards all records.
pub trait LogPort: Send + Sync + 'static {
    /// Emit a structured log record.
    fn record_log(&self, level: LogLevel, message: &str);
}

/// Log severity levels for [`LogPort::record_log`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    /// Trace-level diagnostic.
    Trace,
    /// Debug-level diagnostic.
    Debug,
    /// Informational message.
    Info,
    /// Warning condition.
    Warn,
    /// Error condition.
    Error,
}

/// No-op metrics port — discards all recordings.
#[derive(Debug, Default, Clone)]
pub struct NoOpMetricsPort;

impl MetricsPort for NoOpMetricsPort {
    fn record_counter(&self, _name: &str, _value: u64) {}
    fn record_gauge(&self, _name: &str, _value: f64) {}
    fn record_histogram(&self, _name: &str, _value: f64) {}
}

/// No-op log port — discards all records.
#[derive(Debug, Default, Clone)]
pub struct NoOpLogPort;

impl LogPort for NoOpLogPort {
    fn record_log(&self, _level: LogLevel, _message: &str) {}
}

/// Convenience type alias for an optional metrics port handle.
pub type OptionalMetricsPort = Option<Arc<dyn MetricsPort>>;
/// Convenience type alias for an optional log port handle.
pub type OptionalLogPort = Option<Arc<dyn LogPort>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_metrics_port_is_zero_overhead() {
        let port = NoOpMetricsPort;
        // Must not panic, must compile, must be callable.
        port.record_counter("test", 1);
        port.record_gauge("test", 1.0);
        port.record_histogram("test", 1.0);
    }

    #[test]
    fn noop_log_port_is_zero_overhead() {
        let port = NoOpLogPort;
        port.record_log(LogLevel::Info, "test");
        port.record_log(LogLevel::Error, "test");
    }

    #[test]
    fn metrics_port_is_object_safe() {
        let port: Arc<dyn MetricsPort> = Arc::new(NoOpMetricsPort);
        port.record_counter("x", 42);
    }

    #[test]
    fn log_port_is_object_safe() {
        let port: Arc<dyn LogPort> = Arc::new(NoOpLogPort);
        port.record_log(LogLevel::Warn, "hello");
    }

    #[test]
    fn log_level_ordering() {
        assert!(LogLevel::Trace < LogLevel::Debug);
        assert!(LogLevel::Debug < LogLevel::Info);
        assert!(LogLevel::Info < LogLevel::Warn);
        assert!(LogLevel::Warn < LogLevel::Error);
    }

    #[test]
    fn custom_metrics_port_receives_calls() {
        use std::sync::atomic::{AtomicU64, Ordering};

        struct CountingPort {
            counter_sum: Arc<AtomicU64>,
        }
        impl MetricsPort for CountingPort {
            fn record_counter(&self, _name: &str, value: u64) {
                self.counter_sum.fetch_add(value, Ordering::SeqCst);
            }
            fn record_gauge(&self, _name: &str, _value: f64) {}
            fn record_histogram(&self, _name: &str, _value: f64) {}
        }

        let sum = Arc::new(AtomicU64::new(0));
        let port: Arc<dyn MetricsPort> = Arc::new(CountingPort {
            counter_sum: sum.clone(),
        });
        port.record_counter("ops", 10);
        port.record_counter("ops", 5);
        assert_eq!(sum.load(Ordering::SeqCst), 15);
    }

    #[test]
    fn optional_port_none_by_default() {
        let m: OptionalMetricsPort = None;
        let l: OptionalLogPort = None;
        assert!(m.is_none());
        assert!(l.is_none());
    }
}
