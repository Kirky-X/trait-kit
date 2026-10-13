# trait-kit 日志 / 降级 / 探针载荷消息 — 简体中文
#
# 域：`log` facade 输出（软构建降级、关闭桥接）与经 health/probe API 暴露的探针 readiness 载荷。

# register_shutdown_into stranded 钩子的日志通道

trait-kit-log-shutdown-bridge-stranded = register_shutdown_into：协调器拒绝了一个钩子（{ $error }）；剩余 { $stranded } 个生命周期钩子未转移，将永远不会执行

# 软（可降级）能力构建的降级日志

trait-kit-soft-build-degraded = 模块 `{ $module }` 构建失败: { $error }；降级到回退实现

# 探针 readiness 载荷（AsyncKit run_probes / probe_aggregate 的 detail）

trait-kit-probe-timeout-measured = 探针在 { $measured_ms } 毫秒后超时
trait-kit-probe-registry-stopped = 探针注册表已因关闭而停止
trait-kit-probe-timeout-no-verdict = 探针在给出结论前超时
