# trait-kit log / degradation / probe payload messages — English (fallback)
#
# Domain: `log` facade output (soft-build degradation, shutdown bridge),
# probe readiness payloads surfaced through health/probe APIs, and runtime
# event payloads (KitEvent::ConfigChanged summary).

# register_shutdown_into stranded-hook log channel

trait-kit-log-shutdown-bridge-stranded = register_shutdown_into: coordinator rejected a hook ({ $error }); { $stranded } remaining lifecycle hook(s) are stranded and will never run

# soft (degradable) capability build degradation log

trait-kit-soft-build-degraded = module `{ $module }` build failed: { $error }; degrading to fallback

# probe readiness payloads (AsyncKit run_probes / probe_aggregate details)

trait-kit-probe-timeout-measured = probe timed out after { $measured_ms }ms
trait-kit-probe-registry-stopped = probe registry stopped by shutdown
trait-kit-probe-timeout-no-verdict = probe timed out before returning a verdict

# health readiness payload (detail text of the defensive branch where a checker finds no capability)

trait-kit-health-capability-not-found = capability not found

# shared-config overlay extraction refusal log

trait-kit-log-shared-overlay-refused = shared overlay: key `{ $key }` is already owned by another config type; extraction from `{ $owner }` was refused

# KitEvent::ConfigChanged summary (event-bus payload)

trait-kit-event-config-changed-merge = merge
