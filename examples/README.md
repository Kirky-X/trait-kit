# trait-kit-examples

本示例集合覆盖 `trait-kit` 全部公开 API 与所有 feature 门控。每个示例都是独立的二进制，并通过 `required-features` 门控，仅在启用对应 feature 时才会编译。

最低支持 Rust 版本（MSRV）：**1.97.1+**。

## 运行

```sh
# 默认 feature — 无需额外特性
cargo run -p trait-kit-examples --example default_basic

# confers：Configurable + load_config
cargo run -p trait-kit-examples --example confers_loader --features confers

# confers：ModuleConfig trait + Config derive 再导出
cargo run -p trait-kit-examples --example confers_macros --features confers

# confers：Validatable trait + load_and_validate
cargo run -p trait-kit-examples --example validation --features confers

# reload：subscribe + reload_config
cargo run -p trait-kit-examples --example hot_reload --features reload

# confers：snapshot_config + restore_config + has_snapshot
cargo run -p trait-kit-examples --example snapshot_restore --features confers

# confers：四层配置继承体系
cargo run -p trait-kit-examples --example config_inheritance --features confers

# encryption：加密配置存取（set_encrypted + get_encrypted + HKDF 密钥派生）
cargo run -p trait-kit-examples --example encryption --features encryption

# async：AsyncKit typestate 流程
cargo run -p trait-kit-examples --example async_basic --features async

# lifecycle：on_ready + on_shutdown 钩子
cargo run -p trait-kit-examples --example lifecycle --features lifecycle

# shutdown：ShutdownCoordinator 分阶段优雅关闭
cargo run -p trait-kit-examples --example shutdown --features shutdown

# health：HealthCheck + health_report
cargo run -p trait-kit-examples --example health_check --features health

# observer：BuildObserver 回调
cargo run -p trait-kit-examples --example observability --features observer

# scope：每请求实例隔离
cargo run -p trait-kit-examples --example scope_basic --features scope

# toggle：运行时 feature 开关式模块启用/禁用
cargo run -p trait-kit-examples --example toggle_basic --features toggle

# conditional：谓词门控注册（无 feature 门控 — 始终可用）
cargo run -p trait-kit-examples --example conditional

# factory：每次调用创建实例（无 feature 门控 — 始终可用）
cargo run -p trait-kit-examples --example factory

# decorator：构建后能力包装
cargo run -p trait-kit-examples --example decorator --features decorator

# interface：dyn Trait 依赖注入
cargo run -p trait-kit-examples --example interface --features interface

# i18n：ICU4X 本地化格式化
cargo run -p trait-kit-examples --example i18n --features i18n
```

## 示例清单

| 示例 | Feature | 演示内容 |
| ------------------ | ---------------- | --------------------------------------------------------------------------------------------- |
| `default_basic` | `default` | `ModuleMeta` + `AutoBuilder` + `Kit::new`/`register`/`build`/`require`/`contains`/`optional` |
| `confers_loader` | `confers` | `#[derive(Config)]` + `Configurable` 实现 + `Kit::load_config` + 环境变量回退 |
| `confers_macros` | `confers` | `ModuleConfig` trait（`PATH` + `default_value`）+ 模块在 `build()` 中消费配置 |
| `validation` | `confers` | `Validatable` trait + `Kit::load_and_validate` — 加载时配置校验 |
| `hot_reload` | `reload` | `subscribe::<C>` + `reload_config::<C>` + 基于 `Rc<Cell<_>>` 的回调计数 |
| `snapshot_restore` | `confers` | `snapshot_config` + `restore_config` + `has_snapshot` — 配置快照与回滚 |
| `config_inheritance` | `confers` | 四层配置继承体系（`merge_json_deep` → `ConfigInherit` → `SharedConfig` → `populate_defaults`） |
| `encryption` | `encryption` | `set_encrypted` + `get_encrypted` 加密存取 + 错误密钥拒绝 + `contains_encrypted` |
| `async_basic` | `async` | `AsyncAutoBuilder` + `AsyncKit::new`/`register`/`build`/`require`/`contains`/`set_config` |
| `lifecycle` | `lifecycle` | `Lifecycle` trait（`on_ready` + `on_shutdown`）+ `register_lifecycle` + `Kit::shutdown()` |
| `shutdown` | `shutdown` | `ShutdownCoordinator` 分阶段优雅关闭（`StopRequests` → `DrainQueue` → `CloseConnections`）+ 阶段/全局超时控制 |
| `health_check` | `health` | `HealthCheck` trait + `HealthStatus` + `register_health_check` + `health_check` + `health_report` |
| `observability` | `observer` | `BuildObserver` trait + `with_observer` + `on_module_start`/`on_module_built` 回调 |
| `scope_basic` | `scope` | `Scope::new`/`register`/`require`/`contains` + 每请求实例隔离 + 懒构建缓存 |
| `toggle_basic` | `toggle` | `enable_toggle` + `is_toggle_enabled` + `register_if_toggle` — 运行时 feature 开关控制 |
| `conditional` | — | `register_if::<M>(predicate)` + 运行时谓词门控注册 |
| `factory` | — | `Kit<Ready>::factory::<M>()` + 每次调用创建实例（对比单例 `require()`） |
| `decorator` | `decorator` | `Kit::decorate::<M>(fn)` + 构建后能力转换 |
| `interface` | `interface` | `InterfaceBuilder` + `register_as::<M>()` + `resolve::<dyn Trait>()` 类型擦除 DI |
| `i18n` | `i18n` | `I18nFormatter` + `format_number`/`format_date`/`plural_category`/`compare` + 错误处理 |

## 说明

- 示例 crate 已设置 `publish = false`，是根 `trait-kit` workspace 的成员，不会发布到 crates.io。
- 每个示例成功时以退出码 0 结束，失败时 panic（断言）。
- `async_basic` 示例使用一个最小 `block_on` 执行器（无需 tokio），因为其中的 async future 会立即完成。

---

[← 返回 trait-kit](../README.md)
