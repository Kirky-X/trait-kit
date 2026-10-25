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
