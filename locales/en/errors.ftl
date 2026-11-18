# trait-kit error messages — English (fallback)
#
# Domain: error `Display` templates (TraitKitError / I18nError) and the
# diagnostic context markers they interpolate.

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
trait-kit-error-access-denied = access denied: module `{ $module }` is not visible to caller `{ $caller }`
trait-kit-error-policy-forbidden = dependency policy violation: module `{ $dependent }` must not depend on `{ $dependency }`
trait-kit-error-policy-required-missing = dependency policy violation: module `{ $dependent }` must depend on a module matching `{ $dependency }`
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
trait-kit-error-snapshot-downcast-failed = { $key } (snapshot downcast)
trait-kit-error-config-nesting-depth-exceeded = config nesting depth exceeds { $max_depth } levels
trait-kit-error-master-key-too-short = { $key_label } must be at least { $min_len } bytes, got { $actual }
trait-kit-error-master-key-weak-material = { $key_label } must not be constant-byte key material
trait-kit-error-key-version-mismatch = key-version mismatch for `{ $key }`: stored v{ $stored }, expected v{ $expected_version }
i18n-error-invalid-locale = invalid locale `{ $input }`: { $reason }
i18n-error-invalid-number = invalid number `{ $input }`: { $reason }
i18n-error-number-not-finite = value is not finite (NaN or Infinity)
i18n-error-date = date error: { $detail }
i18n-error-format = formatting error: { $detail }

# shutdown hook isolation (ShutdownResult::into_result BuildFailed source)

trait-kit-error-shutdown-hooks-panicked = { $failures } shutdown hook(s) panicked and were isolated

# non-string panic payload summary (BuildFailed source)

trait-kit-error-panic-payload-non-string = non-string panic payload

# diagnostic context markers (used in error messages)

trait-kit-diag-unknown = <unknown>
trait-kit-diag-multi-binding = <multi-binding>
trait-kit-diag-interface = <interface>
trait-kit-diag-unknown-cycle = <unknown cycle>

# public context category labels (folded from internal call-site labels, interpolated into error messages)

trait-kit-context-configuration-load = configuration load
trait-kit-context-configuration-load-serialize = configuration load (serialize)
trait-kit-context-configuration-load-deserialize = configuration load (deserialize)
trait-kit-context-configuration-load-validated = configuration load (validated)
trait-kit-context-configuration-reload = configuration reload
trait-kit-context-encrypted-config-write = encrypted configuration write
trait-kit-context-encrypted-config-read = encrypted configuration read
trait-kit-context-master-key-rotation = master key rotation
trait-kit-context-master-key-rotation-old-key = master key rotation (old key)
trait-kit-context-master-key-rotation-new-key = master key rotation (new key)
trait-kit-context-readiness-hook = readiness hook
trait-kit-context-shutdown = shutdown
trait-kit-context-hook-registration = hook registration
trait-kit-context-master-key-provider = master key provider

# security posture audit: verification evidence (PostureCheck.detail)

trait-kit-audit-unsafe-code = #![deny(unsafe_code)] enforced crate-wide at compile time; any exemption requires an explicit allow with a SAFETY justification
trait-kit-audit-typestate = Unbuilt/Ready typestate split enforced at compile time: capability queries only exist on Kit<Ready>
trait-kit-audit-dependency-policy-configured = policy configured: { $forbidden } forbidden, { $required } required edges
trait-kit-audit-dependency-policy-no-rules = policy feature enabled but no rules configured — violations undetectable
trait-kit-audit-dependency-policy-no-policy = policy feature enabled but no policy set — violations undetectable
trait-kit-audit-dependency-policy-disabled = policy feature not enabled — architecture violations undetectable
trait-kit-audit-capability-visibility-restricted = { $restricted } of { $total } declared capabilities restricted
trait-kit-audit-capability-visibility-all-public = all { $total } declared capabilities Public — plugins can retrieve everything
trait-kit-audit-capability-visibility-no-declaration = no visibility declared — every capability Public
trait-kit-audit-capability-visibility-disabled = visibility feature not enabled — every capability Public
trait-kit-audit-sensitive-resources-marked = { $marked } sensitive modules marked and auditable
trait-kit-audit-sensitive-resources-none-marked = resource-guard enabled; no sensitive modules marked
trait-kit-audit-sensitive-resources-disabled = resource-guard feature not enabled — sensitive cleanup unverifiable
trait-kit-audit-config-encryption-enabled = config encryption enabled (feature chain: encryption → reload → confers)
trait-kit-audit-config-encryption-reload = hot reload enabled without encryption (chain: reload → confers)
trait-kit-audit-config-encryption-confers = config integration enabled without encryption/reload
trait-kit-audit-config-encryption-disabled = no config persistence feature (confers/reload/encryption) enabled
trait-kit-audit-masking-enabled = PII/credential masking engine enabled
trait-kit-audit-masking-disabled = masking feature not enabled — secrets pass through unredacted
trait-kit-audit-lifecycle-shutdown-both = lifecycle hooks and graceful shutdown coordinator both enabled
trait-kit-audit-lifecycle-shutdown-lifecycle-only = lifecycle hooks enabled; graceful shutdown coordinator not enabled
trait-kit-audit-lifecycle-shutdown-shutdown-only = shutdown coordinator enabled; lifecycle hooks not enabled
trait-kit-audit-lifecycle-shutdown-disabled = neither lifecycle hooks nor graceful shutdown enabled

# masking engine diagnostics (MaskError display + config validation)

mask-config-error = Masking config error: { $message }
mask-secret-scan-limit = Secret scan limit exceeded: pattern '{ $pattern }' hit { $count } times in one input (limit { $limit }); output withheld
mask-secret-scan-oversized = Secret scan oversized input: { $size } bytes exceeds the { $limit }-byte masking limit; output withheld (fail-closed)
mask-rule-already-registered = rule '{ $name }' already registered
mask-toml-parse-failed = failed to parse TOML: { $error }
mask-toml-missing-rules-array = TOML missing '[[masking_rules]]' array
mask-toml-missing-name = masking_rules entry missing 'name' field
mask-toml-duplicate-name-skipped = duplicate masking rule name in TOML config; keeping the first definition and skipping the later one
mask-toml-missing-pattern = masking_rules entry '{ $name }' missing 'pattern' field
mask-rule-requires-pattern = masking rule '{ $name }' requires a `pattern`
mask-invalid-regex-in-rule = invalid regex in rule '{ $name }': { $error }
