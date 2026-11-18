# trait-kit 日志 / 降级 / 探针载荷消息 — 简体中文
#
# 域：`log` facade 输出（软构建降级、关闭桥接）、经 health/probe API 暴露的探针 readiness 载荷，以及运行时事件载荷（KitEvent::ConfigChanged 事件摘要）。

# register_shutdown_into stranded 钩子的日志通道

trait-kit-log-shutdown-bridge-stranded = register_shutdown_into：协调器拒绝了一个钩子（{ $error }）；剩余 { $stranded } 个生命周期钩子未转移，将永远不会执行

# 软（可降级）能力构建的降级日志

trait-kit-soft-build-degraded = 模块 `{ $module }` 构建失败: { $error }；降级到回退实现

# 探针 readiness 载荷（AsyncKit run_probes / probe_aggregate 的 detail）

trait-kit-probe-timeout-measured = 探针在 { $measured_ms } 毫秒后超时
trait-kit-probe-registry-stopped = 探针注册表已因关闭而停止
trait-kit-probe-timeout-no-verdict = 探针在给出结论前超时

# 健康就绪载荷（检查器找不到对应能力时的防御分支 detail 文案）

trait-kit-health-capability-not-found = 未找到能力

# 共享配置 overlay 提取拒绝日志

trait-kit-log-shared-overlay-refused = 共享 overlay：键 `{ $key }` 已被另一个配置类型占用；已拒绝来自 `{ $owner }` 的提取

# KitEvent::ConfigChanged 事件摘要（事件总线载荷）

trait-kit-event-config-changed-merge = 合并
trait-kit-event-config-changed-remote-load = 从 { $source } 远程加载

# resource-guard：构建失败回滚与敏感清理跳过日志

trait-kit-log-build-rollback = 构建失败：正在回滚 { $count } 个已构建模块（on_shutdown，逆拓扑序）
trait-kit-log-rollback-skipped = 构建回滚：模块 `{ $module }` 的 on_shutdown 发生 panic，已被跳过
trait-kit-log-sensitive-skipped = 敏感关闭报告：模块 `{ $module }` 被跳过（on_shutdown panic 或未注册生命周期回调）

# 掩码引擎诊断（tracing warn 日志）

mask-log-oversized-input-redacted = 已跳过脱敏: 输入超过 1 MiB 上限；内容已整体遮蔽（未扫描即不可证明安全）
mask-log-regex-match-error-detect = detect 期间发生正则匹配错误；跳过本次匹配
mask-log-regex-match-error-secret-scan = secret 模式扫描期间发生正则匹配错误；跳过本次匹配
mask-log-oversized-input-rejected = fail-closed secret 扫描出口已拒绝超大输入
mask-log-scan-limit-exceeded = 超出 secret 扫描门限；输出已扣留（fail-closed）
