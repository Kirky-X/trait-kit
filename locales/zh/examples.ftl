# trait-kit 示例演示输出 — 简体中文
#
# 域：examples/ crate 控制台输出；缩进等版式留在代码中。


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
