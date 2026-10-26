# trait-kit-macros

[trait-kit](https://github.com/Kirky-X/trait-kit) 仓库的过程宏子 crate（目录 `trait-kit/macros`）。

## 与父 crate 的关系

| 项 | 内容 |
|---|---|
| 所属仓库 | [trait-kit](https://github.com/Kirky-X/trait-kit)（[crates.io](https://crates.io/crates/trait-kit) · [docs.rs](https://docs.rs/trait-kit)） |
| 依赖方向 | `trait-kit` → `trait-kit-macros`（父 crate 以 `path + version` 双写依赖本 crate） |
| 是否 re-export | **否**：父 crate 不重导出本 crate 的宏，需在 `Cargo.toml` 中显式添加 `trait-kit-macros` 依赖才能使用 `#[derive(...)]`（见 `trait-kit/src/prelude.rs` 的说明） |
| 适用场景 | 需要 `#[derive(Module)]` / `#[derive(ConfigInherit)]` / `#[derive(SharedConfig)]` 时必须直接依赖本 crate；仅使用 trait 与运行时能力时不需要 |

## 提供的派生宏

| 宏 | 作用 |
|---|---|
| `#[derive(Module)]` | 生成 `ModuleMeta` 实现（等价于手写 `impl_module_meta!`） |
| `#[derive(ConfigInherit)]` | 生成配置继承能力，共享字段以 `serde_json::Value` 保留类型信息 |
| `#[derive(SharedConfig)]` | 生成可跨模块共享的配置视图 |

## 使用

```rust
use trait_kit_macros::Module;

#[derive(Module)]
pub struct UserModule {
    // 字段与依赖声明
}
```

## 版本与支持

- 版本经 `version.workspace = true` 随仓库统一，与 `trait-kit` 同号发布。
- 仅维护当前 minor 版本线，详见 [trait-kit 安全文档 · 支持的版本](https://github.com/Kirky-X/trait-kit/blob/main/docs/SECURITY.md#-支持的版本与维护策略)。

## 许可证

MIT
