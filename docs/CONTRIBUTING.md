# 🤝 Trait-Kit 贡献指南

感谢您对 trait-kit 项目的关注！本文档描述参与开发所需的工具、流程和规范。

## 欢迎

欢迎参与 trait-kit 的开发！无论是修复 bug、新增 feature 门控能力、改进文档还是完善示例，都非常有价值。

- Bug 与功能建议请提交到 [GitHub Issues](https://github.com/Kirky-X/trait-kit/issues)。
- 安全漏洞**不要**使用公开 Issue，参见 [安全文档](SECURITY.md) 的漏洞报告流程。
- 动手前建议先在 Issue 中沟通方案，避免与现有 feature 规划冲突。

## 环境准备

- **Rust 1.97.1+**（`Cargo.toml` 中 `rust-version = "1.97.1"`，edition 2024）
- **cargo**、**rustfmt**、**clippy**（随 rustup 安装）
- **cargo-deny**：`cargo install cargo-deny`（pre-commit 与 CI 依赖审计需要）
- **pre-commit**（与 `lefthook.yml` 功能等价，二选一）：

```bash
uv tool install pre-commit   # 或 pip install pre-commit
pre-commit install
```

> trait-kit 本体无系统库依赖；workspace 成员 `examples` 通过 path 依赖 `confers`，若 `cargo check` 报 protoc 缺失，安装 protobuf-compiler 即可（CI 中已安装）。

### 常用命令

```bash
# 构建（全特性 / 无默认特性，与 CI 一致）
cargo build --all-features --lib
cargo build --no-default-features --lib

# 测试（全特性，与 CI 一致）
cargo test --all-features --lib

# 格式化与 Lint
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings

# 文档（零告警标准）
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# 依赖审计
cargo deny check

# MSRV 验证
cargo +1.97.1 check
```

## 开发工作流（TDD）

每个开发任务遵循 Red → Green → Commit → Analyze → Next 循环：

1. **定接口**：先定义 trait / API 签名（如 `trait Xxx { ... }`），不写实现。
2. **写测试**：基于接口编写单元测试（`#[cfg(test)] mod tests`），此时测试应失败（red）。typestate 误用类需求补充 trybuild UI 测试（`tests/compile_fail.rs` + `tests/ui/*.stderr`）。
3. **写代码**：实现接口，使测试通过（green）。
4. **跑测试**：`cargo test --features <对应特性> --lib`。
5. **commit**：`git commit -m "feat(<模块>): <描述>"`。
6. **analyze**：分析本任务对其他模块及下游依赖方的影响，识别需联动修改的代码。
7. **继续下一个**。

### Feature 组合要求

trait-kit 的 feature 之间存在继承关系（`encryption` → `reload` → `confers`），提交前请至少覆盖以下组合（对齐 CI 与验收矩阵）：

```bash
cargo test --all-features --lib
cargo test                          # 默认特性
cargo test --no-default-features
cargo test --features confers
cargo test --features shutdown
cargo test --features i18n
```

新增 feature 时：

- 在 `Cargo.toml` 的 `[features]` 中声明，并写清启用关系与说明注释。
- 在 README（中/英）的「特性标志」表格与 [docs/API_REFERENCE.md](API_REFERENCE.md) 中同步登记。
- 如涉及 confers 集成，注意高级 feature 会自动启用低级 feature，方法需做 `cfg(feature = "...")` 门控。

## 代码规范

- 遵循现有代码库的命名与模块组织惯例（`src/core` 接口层、`src/kit` 管理中心、`src/i18n` 国际化）。
- **简洁优先**：只写能解决问题的最少代码，不添加不必要的注释、docstring 或类型标注。
- **依赖必须通过 feature 门控**：可选依赖（`confers`、`serde`、`serde_json` 等）禁止进入默认特性；`icu` / `writeable` / `sys-locale` 仅为 `i18n` feature 的可选依赖。
- **无 unsafe**：crate 全局 `#![deny(unsafe_code)]`，任何 PR 不得引入 `unsafe`。
- **错误显性化**：新错误场景在 `TraitKitError` 中新增变体（或复用现有变体），错误消息接入 `tr()` 本地化（`src/i18n/messages/{zh,en}.ftl`），严禁吞错或藏进默认值。
- **文档示例必须可验证**：README / docs 中的代码示例与实际 API 签名一致；API 变更时同步 [docs/API_REFERENCE.md](API_REFERENCE.md) 与 [docs/USER_GUIDE.md](USER_GUIDE.md)。

### Pre-commit Hooks

| Hook | 说明 |
|------|------|
| `trailing-whitespace` / `end-of-file-fixer` | 行尾空格与文件末尾换行 |
| `check-yaml` / `check-toml` / `check-merge-conflict` | 配置语法与冲突标记检查 |
| `check-added-large-files` | 大文件拦截（>1024KB） |
| `detect-private-key` | 私钥泄露检测 |
| `typos` | 拼写检查 |
| `cargo-fmt` | `cargo fmt --all -- --check` |
| `cargo-clippy` | `cargo clippy --all-targets --all-features -- -D warnings` |
| `cargo-deny` | `cargo deny check`（许可证/安全通告/封禁依赖） |

> **禁止使用 `--no-verify` 跳过 hooks**，这是安全红线。

### 代码质量工具

- **diting**：代码简化、架构优化、性能审查
- **tiangang**：SAST 安全扫描（发布前必须 0 CRITICAL）
- **kueiku**：硬性 bug 分析与根因定位

## 提交与 PR 流程

1. 从 `main` 创建 feature 分支（如 `feat/<功能>` 或 `fix/<问题>`），**禁止直接提交到 main**。
2. 提交信息遵循 conventional commits：`feat(scope): ...`、`fix(scope): ...`、`docs: ...` 等。
3. 确保本地检查全部通过：
   - pre-commit hooks 全绿
   - `cargo test --all-features --lib` 通过
   - 新功能附带测试；API/文档变更保持 README、CHANGELOG 同步
4. 推送分支并创建 PR，描述中包含：变更说明、测试结果、影响的 feature。
5. CI（fmt / clippy / test / 跨平台构建 / cargo deny / doc）全部通过后等待 review。

发布由维护者执行：tag 推送触发 `release.yml`（构建验证 → GitHub Release → crates.io 发布），贡献者无需操作。

## 行为准则

本项目遵循 [Rust 行为准则](https://www.rust-lang.org/policies/code-of-conduct)。所有贡献者均需遵守——保持友善、尊重与专业，让社区对每个人都友好。
