# trait-kit 错误消息 — 简体中文

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

i18n-error-invalid-locale = 无效的区域设置 `{ $input }`: { $reason }

i18n-error-invalid-number = 无效的数字 `{ $input }`: { $reason }

i18n-error-date = 日期错误: { $detail }

i18n-error-format = 格式化错误: { $detail }

# 关闭钩子隔离（ShutdownResult::into_result 的 BuildFailed source）

trait-kit-error-shutdown-hooks-panicked = { $failures } 个关闭钩子已 panic 并被隔离

# register_shutdown_into stranded 钩子的日志通道

trait-kit-log-shutdown-bridge-stranded = register_shutdown_into：协调器拒绝了一个钩子（{ $error }）；剩余 { $stranded } 个生命周期钩子未转移，将永远不会执行

# 软（可降级）能力构建的降级日志

trait-kit-soft-build-degraded = 模块 `{ $module }` 构建失败: { $error }；降级到回退实现

# 探针 readiness 载荷（AsyncKit run_probes / probe_aggregate 的 detail）

trait-kit-probe-timeout-measured = 探针在 { $measured_ms } 毫秒后超时

trait-kit-probe-registry-stopped = 探针注册表已因关闭而停止

trait-kit-probe-timeout-no-verdict = 探针在给出结论前超时

# 非字符串 panic 载荷摘要（BuildFailed source）

trait-kit-error-panic-payload-non-string = 非字符串 panic 载荷

# 诊断上下文标记（用于错误消息）

trait-kit-diag-unknown = <未知>
trait-kit-diag-multi-binding = <多绑定>
trait-kit-diag-interface = <接口>
trait-kit-diag-unknown-cycle = <未知循环>

# 示例演示输出（examples/ crate；缩进等版式留在代码中）

trait-kit-example-shutdown-title = === ShutdownCoordinator 示例 ===

trait-kit-example-shutdown-stop-requests-listener = [阶段 1] 停止接收新请求——关闭 HTTP 监听器
trait-kit-example-shutdown-stop-requests-draining = [阶段 1] 标记为 draining 状态

trait-kit-example-shutdown-drain-queue = [阶段 2] 排空消息队列中的待处理任务

trait-kit-example-shutdown-close-db-pool = [阶段 3] 关闭数据库连接池
trait-kit-example-shutdown-close-redis = [阶段 3] 关闭 Redis 连接

trait-kit-example-shutdown-running = 正在执行关闭流程...

trait-kit-example-shutdown-results-header = --- 关闭结果 ---

trait-kit-example-shutdown-status-done = ✓ 完成
trait-kit-example-shutdown-status-timed-out = ✗ 超时

trait-kit-example-shutdown-result-line = { $phase } ({ $phase_debug }): { $status } (耗时 { $elapsed })

trait-kit-example-shutdown-ok = shutdown: OK (共执行 { $total_hooks } 个钩子)

trait-kit-example-i18n-system-locale = 系统 locale: { $locale }
trait-kit-example-i18n-translated-message = 翻译结果: { $message }
trait-kit-example-i18n-en-translation = 英文翻译: { $message }
trait-kit-example-i18n-error-message = 错误消息: { $error }
trait-kit-example-i18n-en-us-number = en-US 数字: { $number }
trait-kit-example-i18n-en-us-date = en-US 日期: { $date }
trait-kit-example-i18n-zh-cn-number = zh-CN 数字: { $number }
trait-kit-example-i18n-en-plural = en 复数: 1 -> { $one }, 5 -> { $other }
trait-kit-example-i18n-en-collation = en 排序: 'apple' vs 'banana' -> { $cmp }
