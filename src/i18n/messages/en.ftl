# trait-kit error messages — English (fallback)

trait-kit-error-cycle-detected = dependency cycle detected: { $cycle }

trait-kit-error-dependency-missing = module `{ $module }` depends on `{ $missing }` which is not registered

trait-kit-error-graph-cycle-detected = dependency cycle detected: { $cycle }
trait-kit-error-graph-dependency-missing = module `{ $module }` depends on unregistered module `{ $missing }`

trait-kit-error-already-registered = module `{ $module }` is already registered

trait-kit-error-build-failed = failed to build `{ $context }`: { $source }

trait-kit-error-missing-capability = missing capability `{ $key }`
trait-kit-error-capability-type-mismatch = capability type mismatch for `{ $key }`

trait-kit-error-missing-config = missing config `{ $key }`

trait-kit-error-lifecycle-failed = lifecycle hook failed for `{ $context }`: { $source }

trait-kit-error-shutdown-timed-out = graceful shutdown timed out in phases: { $phases }

trait-kit-error-decorator-target-missing = decorator target module `{ $module }` is not registered (checked at registration time)

trait-kit-error-version-incompatible = module `{ $module }` requires capability `{ $dependency }` >= { $required }, but the provider declares { $provided }

trait-kit-preset-remote-load-failed = remote config source failed: { $message }

trait-kit-preset-provider-not-injected = ConfersConfigModule built without a provider; call register_confers_config(kit, provider) first

# lock-poisoned scaffolding (shutdown coordinator context fragments + source)

trait-kit-error-lock-poisoned-source = RwLock poisoned

trait-kit-error-lock-poisoned-phase = shutdown phase `{ $phase }`

trait-kit-error-lock-poisoned-operation = shutdown coordinator `{ $operation }`

trait-kit-error-lock-poisoned-operation-phase = shutdown coordinator `{ $operation }` on phase `{ $phase }`

# shutdown bridge partial-failure context (register_shutdown_into stranded hooks)

trait-kit-error-shutdown-bridge-stranded = shutdown bridge `{ $operation }` on phase `{ $phase }`; { $stranded } remaining lifecycle hook(s) stranded and will never run

trait-kit-error-config-validation-failed = config validation failed: { $errors }

trait-kit-error-no-snapshot = no snapshot found for `{ $key }`

i18n-error-invalid-locale = invalid locale `{ $input }`: { $reason }

i18n-error-invalid-number = invalid number `{ $input }`: { $reason }

i18n-error-date = date error: { $detail }

i18n-error-format = formatting error: { $detail }

# shutdown hook isolation (ShutdownResult::into_result BuildFailed source)

trait-kit-error-shutdown-hooks-panicked = { $failures } shutdown hook(s) panicked and were isolated

# register_shutdown_into stranded-hook log channel

trait-kit-log-shutdown-bridge-stranded = register_shutdown_into: coordinator rejected a hook ({ $error }); { $stranded } remaining lifecycle hook(s) are stranded and will never run

# soft (degradable) capability build degradation log

trait-kit-soft-build-degraded = module `{ $module }` build failed: { $error }; degrading to fallback

# probe readiness payloads (AsyncKit run_probes / probe_aggregate details)

trait-kit-probe-timeout-measured = probe timed out after { $measured_ms }ms

trait-kit-probe-registry-stopped = probe registry stopped by shutdown

trait-kit-probe-timeout-no-verdict = probe timed out before returning a verdict

# non-string panic payload summary (BuildFailed source)

trait-kit-error-panic-payload-non-string = non-string panic payload

# diagnostic context markers (used in error messages)

trait-kit-diag-unknown = <unknown>
trait-kit-diag-multi-binding = <multi-binding>
trait-kit-diag-interface = <interface>
trait-kit-diag-unknown-cycle = <unknown cycle>

# example demo output (examples/ crate; layout such as indentation stays in code)

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
