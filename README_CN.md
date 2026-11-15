[English](README.md) | 中文

<div align="center">

<img src="docs/assets/trait-kit.png" alt="trait-kit logo" width="240">

[![Rust 2024](https://img.shields.io/badge/Rust-2024-edded?logo=rust&style=for-the-badge)](https://www.rust-lang.org/) [![Rustc 1.97.1+](https://img.shields.io/badge/Rustc-1.97.1+-orange.svg?style=for-the-badge)](https://www.rust-lang.org/) [![crates.io](https://img.shields.io/crates/v/trait-kit?style=for-the-badge)](https://crates.io/crates/trait-kit) [![docs.rs](https://img.shields.io/docsrs/trait-kit?style=for-the-badge)](https://docs.rs/trait-kit) [![downloads](https://img.shields.io/crates/d/trait-kit?style=for-the-badge)](https://crates.io/crates/trait-kit) [![CI](https://img.shields.io/github/actions/workflow/status/Kirky-X/trait-kit/ci.yml?branch=main&label=CI&style=for-the-badge)](https://github.com/Kirky-X/trait-kit/actions/workflows/ci.yml) [![coverage](https://img.shields.io/codecov/c/github/Kirky-X/trait-kit?style=for-the-badge)](https://codecov.io/gh/Kirky-X/trait-kit) [![license](https://img.shields.io/crates/l/trait-kit?style=for-the-badge)](./LICENSE)

**模块标准接口 + `Kit` 能力管理中心**

[✨ 功能特性](#-功能特性) • [🚀 快速开始](#-快速开始) • [📚 文档](#-文档) • [💻 示例](#-示例) • [🤝 参与贡献](#-参与贡献)

</div>

---

## ✨ 功能特性

<table style="width:100%; border-collapse: collapse">
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🧩 <b>标准模块接口</b><br><span style="color:#64748B"><code>ModuleMeta</code> + <code>AutoBuilder</code> 定义统一契约，<code>impl_module_meta!</code> / <code>impl_auto_builder!</code> 宏一行声明模块。</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🏗️ <b>Typestate 构建验证</b><br><span style="color:#64748B"><code>Kit&lt;Unbuilt&gt;</code> 注册模块与配置，<code>build()</code> 做依赖图校验（缺失依赖 + 环检测）后返回 <code>Kit&lt;Ready&gt;</code>，错误在启动前暴露。</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🔎 <b>类型安全能力检索</b><br><span style="color:#64748B">能力按模块类型存储与检索（<code>kit.require::&lt;M&gt;()</code>），无字符串键、无 downcast、无运行时查找表。</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🗂️ <b>配置中心</b><br><span style="color:#64748B"><code>set_config</code> / <code>config::&lt;C&gt;</code> 基于 <code>TypeId</code> 键的 <code>TypeMap</code> 存取类型化配置。</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">⚙️ <b>confers 配置集成</b><br><span style="color:#64748B">三级 feature 继承接入 <a href="https://crates.io/crates/confers">confers</a>：derive 宏配置加载、热重载订阅、XChaCha20-Poly1305 加密存储。</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🌐 <b>AsyncKit 异步支持</b><br><span style="color:#64748B"><code>async</code> feature 提供 <code>Send + Sync</code> 的 <code>AsyncKit</code>，适配连接池、HTTP 客户端等异步初始化场景。</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🩺 <b>运行时可观测</b><br><span style="color:#64748B"><code>lifecycle</code> 生命周期钩子、<code>health</code> 健康检查、<code>observer</code> 构建回调、<code>shutdown</code> 分阶段优雅关闭。</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🌍 <b>ICU4X 国际化</b><br><span style="color:#64748B">区域感知的数字 / 日期 / 复数 / 排序格式化，内置 Fluent FTL 中英文消息翻译（<code>tr()</code>）。</span></td>
</tr>
<tr>
<td width="50%" style="vertical-align:top; padding: 12px">🧱 <b>极简默认依赖</b><br><span style="color:#64748B"><code>default = []</code> 不启用任何可选 feature；仅固定携带 Fluent 翻译（<code>fluent-bundle</code> / <code>unic-langid</code>）与 <code>log</code> 门面 3 个必选依赖，<code>confers</code>、<code>serde</code>、<code>serde_json</code>、<code>icu_*</code>（ICU4X 组件）等其余依赖全部经 feature 门控可选。</span></td>
<td width="50%" style="vertical-align:top; padding: 12px">🚫 <b>默认禁用 unsafe</b><br><span style="color:#64748B"><code>#![deny(unsafe_code)]</code> 全 crate 强制；生产代码仅 5 处经 SAFETY 论证与 code review 的局部豁免（typestate 布局断言 cast ×3、volatile 清零 ×1、guard 生命周期重锚 ×1：<code>AsyncTypeMap</code> 的 <code>RwLock</code> 读守卫；同步 <code>TypeMap</code> 改用 <code>Ref::map</code> 守卫包装，无需 unsafe），另有 1 处测试模块豁免（<code>src/kit/config.rs</code> 的 zeroize spare-capacity 断言）。</span></td>
</tr>
</table>

更多能力（注册模式、构建报告、组合与协商）与「为什么选择 trait-kit？」的方案对比见 [📖 用户指南](docs/cn/USER_GUIDE.md)。

---

## 🚀 快速开始

### 📦 安装

最低支持 Rust 版本（MSRV）：**1.97.1**（edition 2024）。

```sh
cargo add trait-kit
```

默认特性与依赖门控说明见 [📖 用户指南 · 安装](docs/cn/USER_GUIDE.md#安装)。

### 💡 最小示例

定义一个 logger 模块，注册、构建 Kit，然后检索能力（出处：[examples/src/core/default_basic.rs](examples/src/core/default_basic.rs)，有精简）：

```rust
use std::sync::Arc;
use trait_kit::prelude::*;

// 1. 定义能力（任意 Clone 类型）
struct StdoutLogger;
impl StdoutLogger {
    fn info(&self, msg: &str) {
        println!("[LOG] {msg}");
    }
}

// 2. 定义模块：ModuleMeta 声明名称与依赖，AutoBuilder 声明如何构建能力
struct LoggerModule;
impl ModuleMeta for LoggerModule {
    const NAME: &'static str = "logger";
    fn dependencies() -> &'static [(&'static str, std::any::TypeId)] {
        &[]
    }
}

impl AutoBuilder for LoggerModule {
    type Capability = Arc<StdoutLogger>;
    type Error = TraitKitError;
    fn build(_kit: &Kit) -> Result<Self::Capability, Self::Error> {
        Ok(Arc::new(StdoutLogger))
    }
}

// 3. 注册、构建、使用
fn main() {
    let mut kit = Kit::new();
    kit.register::<LoggerModule>().unwrap();
    let kit = kit.build().unwrap();

    let logger = kit.require::<LoggerModule>().unwrap();
    logger.info("Hello from trait-kit!");
    assert!(kit.contains::<LoggerModule>());
}
```

---

## 📚 文档

| 文档 | 说明 |
|------|------|
| [📖 用户指南](docs/cn/USER_GUIDE.md) | 从安装到进阶用法的完整教程 |
| [📘 API 参考](docs/API_REFERENCE.md) | 核心公开 API 速查，逐项标注 feature 门控（非穷尽全量，完整清单以 rustdoc / docs.rs 为准） |
| [🏗️ 架构文档](docs/cn/ARCHITECTURE.md) | 设计模式、数据流、线程安全模型与目录结构 |
| [🧪 验收测试场景](docs/cn/TEST_SCENARIOS.md) | 验收场景穷举矩阵与 e2e 落地对账 |
| [⚡ 性能基准](docs/cn/PERFORMANCE.md) | criterion 基准设施、基线数据与复现方法 |
| [🔒 安全文档](docs/cn/SECURITY.md) | 支持策略、漏洞报告流程与安全设计 |
| [📋 更新日志](docs/CHANGELOG.md) | 每个版本的变更记录 |
| [🤝 贡献指南](docs/cn/CONTRIBUTING.md) | 环境准备、TDD 工作流与提交规范 |
| [📦 crates.io](https://crates.io/crates/trait-kit) / [docs.rs](https://docs.rs/trait-kit) | 发布版本与在线 API 文档 |

---

## 💻 示例

`examples/` 是一个独立的 workspace 成员 `trait-kit-examples`（`publish = false`），共 20 个可独立运行的示例，覆盖 13 个 feature 门控的核心用法（`probe` / `report` / `compose` / `presets` / `presets-remote` / `version-negotiation` 暂无专属示例，用法见 [📘 API 参考](docs/API_REFERENCE.md)）。运行方式：

```sh
cargo run -p trait-kit-examples --example <名称> --features <特性>
```

示例详情见 [examples/README.md](examples/README.md)。

---

## 🏗️ 架构

trait-kit workspace 由三个成员组成：主 crate `trait-kit`、过程宏 crate `trait-kit-macros`（`trait-kit-derive` 已于 0.5.0-rc.5 并入退役）、示例 crate `trait-kit-examples`；主 crate 的 `src/` 分 `core` 接口层、`kit` 能力管理中心、`i18n` 国际化三层。

workspace 结构图、依赖图验证、数据流、线程安全模型与目录结构等完整设计见 [架构文档](docs/cn/ARCHITECTURE.md)。

---

## 🧪 测试

测试策略（单元 / 集成 / 编译期 UI / 宏 / 文档测试 / 示例 / 基准）与常用命令（与 CI 一致）见 [🤝 贡献指南](docs/cn/CONTRIBUTING.md#-测试策略)；验收场景穷举矩阵见 [🧪 验收测试场景](docs/cn/TEST_SCENARIOS.md)。

---

## 📊 性能

性能来自设计层面：typestate 把依赖图验证全部前置到 `build()`，`Kit<Ready>` 上的能力检索是 `TypeId` 查表 + 克隆；同步 `Kit` 基于 `RefCell` 无锁设计。

基线数字、测量环境与复现命令见 [⚡ 性能基准](docs/cn/PERFORMANCE.md)。

---

## 🔒 安全

- **漏洞报告**：请勿通过公开 Issue 报告，使用 GitHub 私密通道 [Security Advisories](https://github.com/Kirky-X/trait-kit/security/policy)（"Report a vulnerability"）。维护者承诺 48 小时内确认、7 天内给出初步评估（见 [SECURITY.md](docs/cn/SECURITY.md)）。

完整的安全设计、最佳实践与安全修复记录见 [docs/cn/SECURITY.md](docs/cn/SECURITY.md)。

---

## 🤝 参与贡献

欢迎参与贡献！完整的开发环境准备、TDD 工作流与提交规范请参见 [贡献指南](docs/cn/CONTRIBUTING.md)。

---

## 📋 更新日志

详见 [CHANGELOG.md](docs/CHANGELOG.md)。

---

## 📄 许可证

本项目基于 [MIT](https://github.com/Kirky-X/trait-kit/blob/main/LICENSE) 许可证发布。

Copyright (c) 2026 Kirky.X🌠

---

## 🙏 致谢

- [`confers`](https://crates.io/crates/confers) — 配置加载、热重载与加密存储的底层能力。
- [ICU4X](https://github.com/unicode-org/icu4x) — 国际化格式化（数字/日期/复数/排序）。
- [Project Fluent](https://projectfluent.org/) — Fluent FTL 消息本地化方案。
- [syn](https://github.com/dtolnay/syn) / [quote](https://github.com/dtolnay/quote) / [proc-macro2](https://github.com/dtolnay/proc-macro2) — 过程宏基础设施（`trait-kit-macros`）。
- [Rust 社区](https://www.rust-lang.org/community) — 优秀的语言生态与工具链。

---

## 📞 联系与支持

- **Bug 与功能建议**：[GitHub Issues](https://github.com/Kirky-X/trait-kit/issues)
- **安全漏洞**：请勿通过公开 Issue 报告，参见 [安全文档](docs/cn/SECURITY.md) 的漏洞报告流程。
- **维护者**：Kirky.X

---

## ⭐ Star 历史

[![Star History Chart](https://api.star-history.com/svg?repos=Kirky-X/trait-kit&type=Date)](https://star-history.com/#Kirky-X/trait-kit&Date)

### 💝 支持本项目

如果您觉得这个项目有用，请考虑给它一个 ⭐️！
