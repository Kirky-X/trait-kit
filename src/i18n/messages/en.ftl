# trait-kit error messages — English (fallback)

trait-kit-error-cycle-detected = dependency cycle detected: { $cycle }

trait-kit-error-dependency-missing = module `{ $module }` depends on `{ $missing }` which is not registered

trait-kit-error-already-registered = module `{ $module }` is already registered

trait-kit-error-build-failed = failed to build `{ $context }`: { $source }

trait-kit-error-missing-capability = missing capability `{ $key }`

trait-kit-error-missing-config = missing config `{ $key }`

trait-kit-error-lifecycle-failed = lifecycle hook failed for `{ $context }`: { $source }

trait-kit-error-shutdown-timed-out = graceful shutdown timed out in phases: { $phases }

i18n-error-invalid-locale = invalid locale '{ $input }': { $reason }

i18n-error-invalid-number = invalid number '{ $input }': { $reason }

i18n-error-date = date error: { $detail }

i18n-error-format = formatting error: { $detail }

# diagnostic context markers (used in error messages)

trait-kit-diag-unknown = <unknown>
trait-kit-diag-multi-binding = <multi-binding>
trait-kit-diag-interface = <interface>
trait-kit-diag-unknown-cycle = <unknown cycle>
