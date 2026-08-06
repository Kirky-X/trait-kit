# trait-kit 错误消息 — 简体中文

trait-kit-error-cycle-detected = 检测到依赖循环: { $cycle }

trait-kit-error-dependency-missing = 模块 `{ $module }` 依赖的 `{ $missing }` 未注册

trait-kit-error-already-registered = 模块 `{ $module }` 已注册

trait-kit-error-build-failed = 构建 `{ $context }` 失败: { $source }

trait-kit-error-missing-capability = 缺少能力 `{ $key }`

trait-kit-error-missing-config = 缺少配置 `{ $key }`

trait-kit-error-lifecycle-failed = `{ $context }` 生命周期钩子失败: { $source }

trait-kit-error-shutdown-timed-out = 优雅关闭在以下阶段超时: { $phases }

trait-kit-error-config-validation-failed = `{ $context }` 配置验证失败: { $errors }

trait-kit-error-no-snapshot = 未找到 `{ $key }` 的配置快照

i18n-error-invalid-locale = 无效的区域设置 '{ $input }': { $reason }

i18n-error-invalid-number = 无效的数字 '{ $input }': { $reason }

i18n-error-date = 日期错误: { $detail }

i18n-error-format = 格式化错误: { $detail }

# 诊断上下文标记（用于错误消息）

trait-kit-diag-unknown = <未知>
trait-kit-diag-multi-binding = <多绑定>
trait-kit-diag-interface = <接口>
trait-kit-diag-unknown-cycle = <未知循环>
