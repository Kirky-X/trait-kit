// Copyright (c) 2026 Kirky.X🌠
// SPDX-License-Identifier: MIT
//! Soft (degradable) capability build helper.
//!
//! `Kit::build()` / `AsyncKit::build()` abort the whole build on the first
//! module error — failure propagation is the default contract. Some
//! capabilities are best-effort instead: the host must still come up when
//! they fail, with the capability replaced by a fallback (in-memory store,
//! no-op service, ...). [`soft_build`] expresses exactly that degradation,
//! with the failure logged at error level so it can never pass silently.

use std::fmt;
use std::future::Future;

/// Build a capability, degrading to `fallback` on failure.
///
/// Awaits `build`; on `Ok` the capability passes through unchanged. On
/// `Err`, the failure is logged at error level (via the `log` facade, so it
/// surfaces in whatever subscriber the consumer installed) and `fallback`
/// is returned instead — the function is infallible by design, which is
/// what distinguishes a degradable capability from a required module.
///
/// `module` is a diagnostic name (e.g. `MyModule::NAME`) used in the
/// failure log only; it does not participate in Kit registration or
/// dependency resolution.
///
/// This is a free function on purpose: degradation decisions belong to the
/// composition site, not to the Kit. It never interacts with
/// `AsyncKit::build()`'s failure propagation, which stays untouched.
///
/// # Example
///
/// ```ignore
/// use trait_kit::kit::soft_build;
///
/// # struct PromptStore;
/// # impl PromptStore { fn in_memory() -> Self { Self } }
/// # struct LoadError;
/// # async fn load_remote_prompts() -> Result<PromptStore, LoadError> { unimplemented!() }
///
/// // Prompt store is best-effort: if the remote load fails, the host still
/// // comes up serving from the in-memory fallback (the failure is logged).
/// let prompts: PromptStore = soft_build(
///     "prompt-store",
///     load_remote_prompts(),
///     PromptStore::in_memory(),
/// )
/// .await;
/// ```
pub async fn soft_build<C, E, F>(module: &'static str, build: F, fallback: C) -> C
where
    F: Future<Output = Result<C, E>>,
    E: fmt::Display,
{
    match build.await {
        Ok(capability) => capability,
        Err(error) => {
            let message = crate::i18n::tr(
                "trait-kit-soft-build-degraded",
                &[("module", module), ("error", &error.to_string())],
            );
            log::error!("{message}");
            fallback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::block_on;

    #[allow(
        clippy::unused_async,
        reason = "test doubles model async build closures; the async-ness is the simulated contract"
    )]
    async fn ok_build() -> Result<&'static str, String> {
        Ok("real-capability")
    }

    #[allow(
        clippy::unused_async,
        reason = "test doubles model async build closures; the async-ness is the simulated contract"
    )]
    async fn failing_build() -> Result<&'static str, String> {
        Err("boom".to_owned())
    }

    #[test]
    fn soft_build_success_returns_capability() {
        let capability = block_on(soft_build("soft-module", ok_build(), "fallback"));
        assert_eq!(
            capability, "real-capability",
            "a successful build must pass its capability through, fallback unused"
        );
    }

    #[test]
    fn soft_build_failure_logs_error_and_returns_fallback() {
        let _ = log::set_logger(&CAPTURE_LOGGER);
        log::set_max_level(log::LevelFilter::Error);
        CAPTURED_RECORDS
            .lock()
            .expect("capture mutex poisoned")
            .clear();

        let capability = block_on(soft_build("soft-module", failing_build(), "fallback"));
        assert_eq!(
            capability, "fallback",
            "a failed build must degrade to the fallback, not propagate the error"
        );

        let records = CAPTURED_RECORDS.lock().expect("capture mutex poisoned");
        assert!(
            records
                .iter()
                .any(|m| m.contains("soft-module") && m.contains("boom")),
            "degradation must emit an error-level log naming the module and \
             the failure, got {records:?}"
        );
        assert!(
            records.iter().any(|m| m.contains("[ERROR]")),
            "degradation must be logged at error level, got {records:?}"
        );
    }

    static CAPTURED_RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct CaptureLogger;

    impl log::Log for CaptureLogger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            metadata.level() <= log::Level::Error
        }

        fn log(&self, record: &log::Record) {
            CAPTURED_RECORDS
                .lock()
                .expect("capture mutex poisoned")
                .push(format!("[{}] {}", record.level(), record.args()));
        }

        fn flush(&self) {}
    }

    static CAPTURE_LOGGER: CaptureLogger = CaptureLogger;

    use std::sync::Mutex;
}
