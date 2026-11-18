# trait-kit 错误消息 — 简体中文
#
# 域：错误 `Display` 模板（TraitKitError / I18nError）及其插值的诊断上下文标记。

trait-kit-error-cycle-detected = 检测到依赖循环: { $cycle }
trait-kit-error-dependency-missing = 模块 `{ $module }` 依赖的 `{ $missing }` 未注册
trait-kit-error-graph-cycle-detected = 检测到依赖循环: { $cycle }
trait-kit-error-graph-dependency-missing = 模块 `{ $module }` 依赖的模块 `{ $missing }` 未注册
trait-kit-error-already-registered = 模块 `{ $module }` 已注册
trait-kit-error-build-failed = 构建 `{ $context }` 失败: { $source }
trait-kit-error-missing-capability = 缺少能力 `{ $key }`
trait-kit-error-capability-type-mismatch = 能力类型不匹配 `{ $key }`
trait-kit-error-missing-config = 缺少配置 `{ $key }`
trait-kit-error-lifecycle-failed = `{ $context }` 生命周期钩子失败: { $source }
trait-kit-error-shutdown-timed-out = 优雅关闭在以下阶段超时: { $phases }
trait-kit-error-access-denied = 拒绝访问: 模块 `{ $module }` 对调用方 `{ $caller }` 不可见
trait-kit-error-policy-forbidden = 依赖策略违规: 模块 `{ $dependent }` 不得依赖 `{ $dependency }`
trait-kit-error-policy-required-missing = 依赖策略违规: 模块 `{ $dependent }` 必须依赖匹配 `{ $dependency }` 的模块
trait-kit-error-decorator-target-missing = 装饰目标模块 `{ $module }` 未注册（注册时校验）
trait-kit-error-version-incompatible = 模块 `{ $module }` 要求能力 `{ $dependency }` >= { $required }，但提供方声明为 { $provided }
trait-kit-preset-remote-load-failed = 远程配置源加载失败: { $message }
trait-kit-preset-provider-not-injected = ConfersConfigModule 构建时未注入 provider；请先调用 register_confers_config(kit, provider)

# 锁中毒文案（关闭协调器的 context 片段与 source）

trait-kit-error-lock-poisoned-source = 读写锁中毒
trait-kit-error-lock-poisoned-phase = 关闭阶段 `{ $phase }`
trait-kit-error-lock-poisoned-operation = 关闭协调器 `{ $operation }`
trait-kit-error-lock-poisoned-operation-phase = 关闭协调器 `{ $operation }`（阶段 `{ $phase }`）

# 关闭桥接部分失败文案（register_shutdown_into 的 stranded 钩子）

trait-kit-error-shutdown-bridge-stranded = 关闭桥接 `{ $operation }`（阶段 `{ $phase }`）；剩余 { $stranded } 个生命周期钩子未转移，将永远不会执行
trait-kit-error-config-validation-failed = 配置验证失败: { $errors }
trait-kit-error-no-snapshot = 未找到 `{ $key }` 的配置快照
trait-kit-error-snapshot-downcast-failed = { $key }（快照类型转换失败）
trait-kit-error-config-nesting-depth-exceeded = 配置嵌套深度超过 { $max_depth } 层
trait-kit-error-master-key-too-short = `{ $key_label }` 至少需要 { $min_len } 字节，实际 { $actual } 字节
trait-kit-error-master-key-weak-material = `{ $key_label }` 不能是常量字节密钥材料
trait-kit-error-key-version-mismatch = `{ $key }` 的密钥版本不匹配: 已存 v{ $stored }，期望 v{ $expected_version }
i18n-error-invalid-locale = 无效的区域设置 `{ $input }`: { $reason }
i18n-error-invalid-number = 无效的数字 `{ $input }`: { $reason }
i18n-error-number-not-finite = 数值不是有限数（NaN 或 Infinity）
i18n-error-date = 日期错误: { $detail }
i18n-error-format = 格式化错误: { $detail }

# 关闭钩子隔离（ShutdownResult::into_result 的 BuildFailed source）

trait-kit-error-shutdown-hooks-panicked = { $failures } 个关闭钩子已 panic 并被隔离

# 非字符串 panic 载荷摘要（BuildFailed source）

trait-kit-error-panic-payload-non-string = 非字符串 panic 载荷

# 诊断上下文标记（用于错误消息）

trait-kit-diag-unknown = <未知>
trait-kit-diag-multi-binding = <多绑定>
trait-kit-diag-interface = <接口>
trait-kit-diag-unknown-cycle = <未知循环>

# 对外类别标签（内部调用点折叠后的文案，插入错误消息）

trait-kit-context-configuration-load = 配置加载
trait-kit-context-configuration-load-serialize = 配置加载（序列化）
trait-kit-context-configuration-load-deserialize = 配置加载（反序列化）
trait-kit-context-configuration-load-validated = 配置加载（已校验）
trait-kit-context-configuration-reload = 配置重载
trait-kit-context-encrypted-config-write = 加密配置写入
trait-kit-context-encrypted-config-read = 加密配置读取
trait-kit-context-master-key-rotation = 主密钥轮换
trait-kit-context-master-key-rotation-old-key = 主密钥轮换（旧密钥）
trait-kit-context-master-key-rotation-new-key = 主密钥轮换（新密钥）
trait-kit-context-readiness-hook = 就绪钩子
trait-kit-context-shutdown = 关闭
trait-kit-context-hook-registration = 钩子注册
trait-kit-context-master-key-provider = 主密钥供给方

# 安全态势审计：验证证据（PostureCheck.detail）

trait-kit-audit-unsafe-code = crate 全局在编译期强制 `#![deny(unsafe_code)]`；任何豁免都必须显式 allow 并附 SAFETY 论证
trait-kit-audit-typestate = Unbuilt/Ready 类型状态拆分在编译期强制：能力查询只存在于 Kit<Ready> 上
trait-kit-audit-dependency-policy-configured = 已配置策略: { $forbidden } 条禁止边，{ $required } 条必需边
trait-kit-audit-dependency-policy-no-rules = 已启用 policy feature 但未配置规则 — 无法检测违规
trait-kit-audit-dependency-policy-no-policy = 已启用 policy feature 但未设置策略 — 无法检测违规
trait-kit-audit-dependency-policy-disabled = 未启用 policy feature — 无法检测架构违规
trait-kit-audit-capability-visibility-restricted = 已声明的 { $total } 个能力中有 { $restricted } 个受限
trait-kit-audit-capability-visibility-all-public = 已声明的 { $total } 个能力全部为 Public — 插件可获取一切
trait-kit-audit-capability-visibility-no-declaration = 未声明可见性 — 所有能力均为 Public
trait-kit-audit-capability-visibility-disabled = 未启用 visibility feature — 所有能力均为 Public
trait-kit-audit-sensitive-resources-marked = 已标记 { $marked } 个敏感模块，可审计
trait-kit-audit-sensitive-resources-none-marked = 已启用 resource-guard；未标记任何敏感模块
trait-kit-audit-sensitive-resources-disabled = 未启用 resource-guard feature — 无法验证敏感清理
trait-kit-audit-config-encryption-enabled = 已启用配置加密（feature 链: encryption → reload → confers）
trait-kit-audit-config-encryption-reload = 已启用热重载但未启用加密（链: reload → confers）
trait-kit-audit-config-encryption-confers = 已启用配置集成但未启用加密/重载
trait-kit-audit-config-encryption-disabled = 未启用任何配置持久化 feature（confers/reload/encryption）
trait-kit-audit-masking-enabled = PII/凭据脱敏引擎已启用
trait-kit-audit-masking-disabled = 未启用 masking feature — 密钥未经脱敏直接透传
trait-kit-audit-lifecycle-shutdown-both = 生命周期钩子与优雅关闭协调器均已启用
trait-kit-audit-lifecycle-shutdown-lifecycle-only = 已启用生命周期钩子；未启用优雅关闭协调器
trait-kit-audit-lifecycle-shutdown-shutdown-only = 已启用关闭协调器；未启用生命周期钩子
trait-kit-audit-lifecycle-shutdown-disabled = 生命周期钩子与优雅关闭均未启用

# 掩码引擎诊断（MaskError display 与配置校验）

mask-config-error = 脱敏配置错误: { $message }
mask-secret-scan-limit = 超出 secret 扫描门限: 模式 '{ $pattern }' 在单条输入中命中 { $count } 次（门限 { $limit }）；输出已扣留
mask-secret-scan-oversized = secret 扫描输入过大: { $size } 字节超过 { $limit } 字节脱敏上限；输出已扣留（fail-closed）
mask-rule-already-registered = 规则 '{ $name }' 已注册
mask-toml-parse-failed = TOML 解析失败: { $error }
mask-toml-missing-rules-array = TOML 缺少 '[[masking_rules]]' 数组
mask-toml-missing-name = masking_rules 条目缺少 'name' 字段
mask-toml-duplicate-name-skipped = TOML 配置中脱敏规则名重复；保留首个定义并跳过后续定义
mask-toml-missing-pattern = masking_rules 条目 '{ $name }' 缺少 'pattern' 字段
mask-rule-requires-pattern = 脱敏规则 '{ $name }' 缺少 `pattern`
mask-invalid-regex-in-rule = 规则 '{ $name }' 中的正则无效: { $error }
