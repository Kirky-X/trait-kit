# trait-kit example demo output — English (fallback)
#
# Domain: examples/ crate console output; layout such as indentation stays in code.


trait-kit-example-shutdown-title = === ShutdownCoordinator demo ===

trait-kit-example-shutdown-stop-requests-listener = [phase 1] stop accepting new requests — close HTTP listener
trait-kit-example-shutdown-stop-requests-draining = [phase 1] mark as draining
trait-kit-example-shutdown-drain-queue = [phase 2] drain pending tasks in the message queue
trait-kit-example-shutdown-close-db-pool = [phase 3] close database connection pool
trait-kit-example-shutdown-close-redis = [phase 3] close Redis connection
trait-kit-example-shutdown-running = running shutdown...
trait-kit-example-shutdown-results-header = --- shutdown results ---
trait-kit-example-shutdown-status-done = ✓ done
trait-kit-example-shutdown-status-timed-out = ✗ timed out
trait-kit-example-shutdown-result-line = { $phase } ({ $phase_debug }): { $status } (elapsed { $elapsed })
trait-kit-example-shutdown-ok = shutdown: OK ({ $total_hooks } hooks executed)
trait-kit-example-i18n-system-locale = system locale: { $locale }
trait-kit-example-i18n-translated-message = translated message: { $message }
trait-kit-example-i18n-en-translation = en translation: { $message }
trait-kit-example-i18n-error-message = error message: { $error }
trait-kit-example-i18n-en-us-number = en-US number: { $number }
trait-kit-example-i18n-en-us-date = en-US date: { $date }
trait-kit-example-i18n-zh-cn-number = zh-CN number: { $number }
trait-kit-example-i18n-en-plural = en plural: 1 -> { $one }, 5 -> { $other }
trait-kit-example-i18n-en-collation = en collation: 'apple' vs 'banana' -> { $cmp }
