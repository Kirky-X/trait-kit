# 🤝 Trait-Kit 贡献指南

感谢您对 trait-kit 项目的关注！本文档描述参与开发所需的工具、流程和规范。

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [👋 欢迎](#-欢迎)
- [🧰 环境准备](#-环境准备)
- [🔁 开发工作流（TDD）](#-开发工作流tdd)
- [📐 代码规范](#-代码规范)
- [🚀 提交与 PR 流程](#-提交与-pr-流程)
- [📜 行为准则](#-行为准则)
- [🧪 测试策略](#-测试策略)
- [🗺️ 开发路线图](#️-开发路线图)

</details>

---

## 👋 欢迎

欢迎参与 trait-kit 的开发！无论是修复 bug、新增 feature 门控能力、改进文档还是完善示例，都非常有价值。

- Bug 与功能建议请提交到 [GitHub Issues](https://github.com/Kirky-X/trait-kit/issues)。
- 安全漏洞**不要**使用公开 Issue，参见 [安全文档](SECURITY.md) 的漏洞报告流程。
- 动手前建议先在 Issue 中沟通方案，避免与现有 feature 规划冲突。

## 🧰 环境准备

- **Rust 1.97.1+**（`Cargo.toml` 中 `rust-version = "1.97.1"`，edition 2024）
- **cargo**、**rustfmt**、**clippy**（随 rustup 安装）
- **cargo-deny**：`cargo install cargo-deny`（lefthook 与 CI 依赖审计需要）
- **pre-commit** 或 **lefthook**（二选一；两者覆盖面不完全等价——pre-commit 含 `detect-secrets` / `cargo-check` 等检查，lefthook 在提交期含 `cargo-deny` 与私钥 grep、pre-push 含覆盖率门禁；`cargo-deny` / `cargo-audit` 在 pre-commit 框架中挂在 pre-push 阶段（`pre-commit install -t pre-push` 启用））：

```sh
uv tool install pre-commit   # 或 pip install pre-commit
pre-commit install
```

> trait-kit 本体无系统库依赖；workspace 成员 `examples` 的 `confers` 依赖走 crates.io（无跨仓 path），全 workspace 均无需 `protoc` / `protobuf-compiler`。

### 常用命令

```sh
# 构建（全特性 / 无默认特性，与 CI 一致）
cargo build --all-features --workspace
cargo build --no-default-features --workspace

# 测试（全特性，与 CI 一致）
cargo test --workspace --all-features

# 格式化与 Lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 文档（零告警标准）
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps

# 依赖审计
cargo deny check

# MSRV 验证
cargo +1.97.1 check
```

## 🔁 开发工作流（TDD）

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

```sh
cargo test --workspace --all-features   # 与 CI test job 一致
cargo test                              # 默认特性
cargo test --no-default-features
cargo test --features confers
cargo test --features shutdown
cargo test --features i18n
```

新增 feature 时：

- 在 `Cargo.toml` 的 `[features]` 中声明，并写清启用关系与说明注释。
- 在 README（中/英）的「特性标志」表格与 [docs/cn/API_REFERENCE.md](API_REFERENCE.md) 中同步登记。
- 如涉及 confers 集成，注意高级 feature 会自动启用低级 feature，方法需做 `cfg(feature = "...")` 门控。

## 📐 代码规范

- 遵循现有代码库的命名与模块组织惯例（`src/core` 接口层、`src/kit` 管理中心、`src/i18n` 国际化）。
- **简洁优先**：只写能解决问题的最少代码，不添加不必要的注释、docstring 或类型标注。
- **依赖必须通过 feature 门控**：可选依赖（`confers`、`serde`、`serde_json` 等）禁止进入默认特性；`icu_collator` / `icu_datetime` / `icu_decimal` / `icu_locale` / `icu_plurals`（ICU4X 组件子 crate，不引 `icu` 门面 crate）/ `writeable` / `sys-locale` 仅为 `i18n` feature 的可选依赖。
- **无新增 unsafe**：crate 全局 `#![deny(unsafe_code)]`，生产代码现存仅 5 处经 SAFETY 论证与 code review 的局部豁免（typestate 布局断言 cast ×3、volatile 清零 ×1、guard 生命周期重锚 ×1：`AsyncTypeMap` 的 `RwLock` 读守卫；同步 `TypeMap` 改用 `Ref::map` 守卫包装，无需 unsafe），另有 1 处测试模块豁免（见 [安全文档](SECURITY.md)）。任何 PR 不得引入新的 `unsafe`；确有必要时须附 SAFETY 注释并在 PR 中单独论证。
- **错误显性化**：新错误场景在 `TraitKitError` 中新增变体（或复用现有变体），错误消息接入 `tr()` 本地化（按域写入 `locales/en/*.ftl` 与 `locales/zh/*.ftl`，两份键集合必须齐平），严禁吞错或藏进默认值。
- **文档示例必须可验证**：README / docs 中的代码示例与实际 API 签名一致；API 变更时同步 [docs/cn/API_REFERENCE.md](API_REFERENCE.md) 与 [docs/cn/USER_GUIDE.md](USER_GUIDE.md)。

### Pre-commit Hooks

| Hook | 说明 |
|------|------|
| `trailing-whitespace` / `end-of-file-fixer` | 行尾空格与文件末尾换行 |
| `check-yaml` / `check-toml` / `check-merge-conflict` | 配置语法与冲突标记检查 |
| `check-added-large-files` | 大文件拦截（`--maxkb=1000`，即 >1000KB） |
| `detect-private-key` / `detect-secrets` | 私钥泄露检测与密钥基线扫描 |
| `typos` | 拼写检查 |
| `cargo-fmt` | `cargo fmt --all -- --check` |
| `cargo-check` | `cargo check --all-features` |
| `cargo-clippy` | `cargo clippy --all-targets --all-features -- -D warnings` |

> `cargo-deny`（许可证/安全通告/封禁依赖审计）两套钩子均有配置：pre-commit 框架中为 pre-push 阶段钩子（`pre-commit install -t pre-push` 启用），lefthook 中在 pre-commit 阶段运行；CI 亦有对应 job（`.github/workflows/ci.yml`）。

> **禁止使用 `--no-verify` 跳过 hooks**，这是安全红线。

### 代码质量工具

- **代码质量审查**：代码简化、架构优化、性能审查
- **SAST 安全扫描**：发布前必须 0 CRITICAL
- **kueiku**：硬性 bug 分析与根因定位

## 🚀 提交与 PR 流程

1. 本仓工作流为 **main 直提**（`.pre-commit-config.yaml` 注明：不设 `no-commit-to-branch`——该钩子保护的是 `master`，在本仓不会生效，故未配置分支保护钩子）；外部贡献者可从 `main` 创建 feature 分支（如 `feat/<功能>` 或 `fix/<问题>`）提 PR。
2. 提交信息遵循 conventional commits：`feat(scope): ...`、`fix(scope): ...`、`docs: ...` 等。
3. 确保本地检查全部通过：
   - pre-commit hooks 全绿
   - `cargo test --workspace --all-features` 通过
   - 新功能附带测试；API/文档变更保持 README、CHANGELOG 同步
4. 推送分支并创建 PR，描述中包含：变更说明、测试结果、影响的 feature。
5. CI（fmt / clippy / test / 跨平台构建 / cargo deny / doc）全部通过后等待 review。

发布由 Kirky.X 执行：tag 推送触发 `release.yml`（构建验证 → GitHub Release → crates.io 发布），贡献者无需操作。

## 📜 行为准则

本项目遵循 [Rust 行为准则](https://www.rust-lang.org/policies/code-of-conduct)。所有贡献者均需遵守：保持友善、尊重与专业，让社区对每个人都友好。

---

## 🧪 测试策略

| 类型 | 位置 | 说明 |
|------|------|------|
| 单元测试 | `src/`（`#[cfg(test)]`） | 模块内部逻辑 |
| 集成测试 | `tests/`（9 个目标）+ `tests/e2e/`（13 个注册目标） | `basic`、`config_inheritance_e2e`、`e2e_core`、`e2e_async`、`e2e_concurrency`、`e2e_feature_combinations` 等 |
| 编译期 UI 测试 | `tests/compile_fail.rs` + `tests/ui/`（11 个用例：根 7 个 + `probe_sensitive/`、`observer_ready/`、`decorator_ready/`、`lifecycle_ready/` 各 1 个） | 基于 trybuild，断言 typestate 误用（未构建即检索等）无法编译 |
| 宏 crate 测试 | `macros/tests/` | `#[derive(Module)]` 展开正确性与编译失败用例 |
| 文档测试 | README 与 doc 注释中的 `rust` 代码块 | 随 `cargo test` 编译运行 |
| 示例验证 | `examples/`（20 个） | 每个示例独立运行，断言失败即 panic |
| 基准测试 | `benches/kit_bench.rs` | criterion 基准（需 `toggle` feature） |

测试规模：主 crate `src/` 与 `tests/` 共 **1040** 个 `#[test]` 函数（`grep -rEo '#\[(tokio::)?test\]' --include='*.rs' src tests | wc -l` 统计），另有 `macros/tests/` 6 个。

### 常用命令（与 CI 一致）

```sh
# 全量测试（workspace + 全部 feature，与 CI test job 一致）
cargo test --workspace --all-features

# 按特性组合运行
cargo test --features confers

# Lint 与格式检查（CI 门禁，零告警）
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

# 文档零告警
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

# 依赖审计（advisories / licenses / bans / sources）
cargo deny check

# 覆盖率门禁（CI coverage job：行覆盖 ≥ 95%）
cargo llvm-cov --workspace --all-features --fail-under-lines 95
```

## 🗺️ 开发路线图

条目沿用既有规划，状态对齐当前仓库事实：

<table>
<tr><th>状态</th><th>条目</th><th>说明</th></tr>
<tr><td>✅</td><td><b>0.5.0-rc.12</b>（2026-10-31）</td><td>新增 <code>masking</code> feature——PII/凭据掩码引擎（<code>DataMasker</code> / <code>MaskRule</code> / <code>MaskRuleRegistry</code> / <code>AcMasker</code>）自 inklog 下沉为本仓单一来源，含 secret-scan 出站门族（<code>SecretScanGate</code> / <code>SecretPatternRegistry</code> / <code>EntropyScanner</code>）与 <code>MaskError</code>；inklog 与 limiteron 共用同一实现，21 条内置规则语义与顺序不变（详见 <a href="../CHANGELOG.md">CHANGELOG</a>）。</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.11</b>（2026-10-28）</td><td><code>override_module</code> 依赖图校验收紧（未注册依赖注入期即拒绝）与共享 overlay 属主隔离；对外文本与错误 <code>Debug</code> 全面脱敏；主密钥准入策略；<code>TypeMap</code> 读引用守卫（unsafe 豁免 6→5 处）；深嵌套 JSON 深度守卫前移；<code>confers</code> 依赖 0.6.0-rc.12（详见 <a href="../CHANGELOG.md">CHANGELOG</a>）。</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.10</b>（2026-10-19）</td><td>i18n 默认语言严格英文与 <code>locales/{en,zh}/</code> 按域拆分、<code>rustls</code> 0.23.46 安全升级、统一纯 MIT 许可、发布切 crates.io trusted publishing（OIDC）与发布幂等三元组（详见 <a href="../CHANGELOG.md">CHANGELOG</a>）。</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.9</b>（2026-10-10）</td><td>服务探针注册面（新 <code>probe</code> feature）、async 健康聚合（<code>health_aggregate</code> / <code>health_json</code>）、构建报告 <code>config_overrides</code> 显式化、feature 组合矩阵与性能回归 CI 门禁；修复版本协商四路径奇偶、lazy 检索口径统一、关闭协调器 hook panic 隔离等（详见 <a href="../CHANGELOG.md">CHANGELOG</a>）。</td></tr>
<tr><td>✅</td><td><b>0.5.0-rc.6</b>（2026-09-21）</td><td>rc.4–rc.6 已发布：观测端口与事件总线、Toggle 完整落地、<code>trait-kit-derive</code> 并入 <code>trait-kit-macros</code>、<code>confers</code> 依赖改走 crates.io（详见 <a href="../CHANGELOG.md">CHANGELOG</a>）。</td></tr>
<tr><td>✅</td><td><b>性能基准</b></td><td>criterion 基准 <code>benches/kit_bench.rs</code> 与 <code>docs/cn/PERFORMANCE.md</code> 基线报告已建立（现行基线 2026-09-29）。</td></tr>
<tr><td>✅</td><td><b>0.5.0 正式发布</b>（2026-11-14）</td><td>定版：API 与行为相对 0.5.0-rc.14 无变更，<code>confers</code> 依赖基线自 0.6.0-rc.12 升至 <code>0.6</code> 正式版（详见 <a href="../CHANGELOG.md">CHANGELOG</a>）；下游仓库（oxcache、dbnexus、inklog、limiteron、sdforge）的 <code>path + version</code> 依赖同步按工作区发布计划跟踪。</td></tr>
<tr><td>✅</td><td><b>cfg 门控完整性</b></td><td><code>--no-default-features --features async</code> 与 <code>async,observer</code> 组合 check、<code>clippy --features async --all-targets</code> 零告警，async doc 测试全通过——门控缺口已消除（2026-09-27 复测确认）。</td></tr>
</table>
