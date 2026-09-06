# 🔒 Trait-Kit 安全文档

本文档描述 trait-kit 的版本支持策略、漏洞报告流程，以及库在安全设计上的真实机制与使用建议。

## 📋 目录

<details open>
<summary>目录</summary>

- [支持版本](#支持版本)
- [漏洞报告流程](#漏洞报告流程)
- [安全设计概览](#安全设计概览)
- [安全最佳实践](#安全最佳实践)

</details>

---

## 支持版本

建议始终使用 crates.io 上的最新发布版本，并及时跟进补丁更新。

| 版本线 | 当前版本 | 状态 |
|--------|---------|------|
| 0.5.x | 0.5.0-rc.2 | 开发主线（预发布） |
| 0.4.x | 0.4.2 | 上一稳定线，仅接受关键修复 |

> trait-kit 遵循语义化版本：0.x 阶段次版本位的升级可能包含破坏性变更（如 0.4.0 的 BREAKING CHANGES），升级时请阅读 [CHANGELOG](CHANGELOG.md)。

---

## 漏洞报告流程

- **请不要通过公开 GitHub Issue 报告安全漏洞**，避免漏洞细节在修复前暴露。
- 请使用 GitHub 的私密漏洞报告通道：仓库页面 **Security → Report a vulnerability**（私密安全建议）。
- 报告时请尽量包含：影响版本、复现步骤或概念验证代码、影响评估。
- 维护者确认后会评估严重性、开发修复并在补丁发布时致谢报告者（除非您希望匿名）。

**安全加固渠道**：除漏洞报告外，仓库 CI 持续运行依赖与静态分析门禁（见下文），发布前还会执行 SAST 扫描（Semgrep / cargo-audit / Trivy / 密钥扫描），历史上保持零未处置 Critical/High 的记录。

---

## 安全设计概览

以下是 trait-kit 中真实存在的安全相关机制（均可从源码与 CHANGELOG 验证）：

### 无 unsafe 代码

整个 crate 标注 `#![deny(unsafe_code)]`，编译期强制排除未定义行为的主要来源之一。依赖图校验、类型检索等均以安全 Rust 实现。

### 类型安全的能力检索

能力以 `TypeId` 为键存放在 `TypeMap` 中，按模块类型检索（`kit.require::<M>()`）。没有字符串键查找与 downcast 路径，减少了运行时类型混淆类问题的暴露面。

### 构建期验证（typestate）

`Kit<Unbuilt>` → `Kit<Ready>` 两阶段类型状态使"未构建就检索"成为编译错误（见 `tests/ui/` 下的 trybuild 断言）。依赖图在 `build()` 时做缺失依赖检测与环检测，把装配错误前置到应用启动之前。

### 明确的线程安全边界

| 类型 | Send | Sync | 内部实现 |
|---|---|---|---|
| `Kit<S>` / `TypeMap` / `Scope` | ✗ | ✗ | `RefCell`（单线程，无数据竞争） |
| `AsyncKit<S>` / `AsyncTypeMap` / `AsyncScope` | ✓ | ✓ | `Arc<RwLock>` |

编译器强制 `Kit` 不可跨线程共享，从类型层面排除了该误用。

### 加密配置存储 `encryption`

- 算法：XChaCha20-Poly1305 认证加密（AEAD），静态配置加密存储。
- 密钥派生：加密密钥通过 HKDF 从主密钥与 `ModuleConfig::PATH` 派生，同一主密钥为不同模块生成不同字段密钥，限制跨模块的密钥复用。
- 防泄露：`EncryptedBlob` 的 `Debug` 实现不输出加密材料（0.4.1 中的安全修复，见 [CHANGELOG](CHANGELOG.md)）。

### 错误信息与国际化

`TraitKitError` 统一错误类型，`Display` 通过 `tr()` 本地化输出，避免异常路径上拼接敏感细节；`BuildFailed` 等变体保留结构化的 `source` 供程序化处理而非仅靠文本。

### CI 安全门禁

- `cargo deny check`：依赖许可证/安全通告/重复依赖审计（CI 必过项）。
- CodeQL 静态分析（`.github/workflows/codeql.yml`）。
- 发布流程含 cargo-audit、Trivy、Semgrep 与密钥扫描（Gitleaks/Trufflehog）。

---

## 安全最佳实践

使用 trait-kit 构建应用时：

1. **妥善保管主密钥**：`encryption` feature 的 32 字节主密钥是加密配置的根信任，应从环境变量、密钥管理服务或硬件密钥源获取；切勿硬编码进源码或提交进仓库（文档示例中的 `[0u8; 32]` 仅为演示）。
2. **敏感配置启用加密**：数据库口令、API 密钥等静态敏感配置使用 `set_encrypted` / `get_encrypted`，避免明文驻留内存结构或日志。
3. **配置校验前置**：外部来源的配置用 `Validatable` + `load_and_validate::<C>()` 在加载时校验，失败不存入，防止恶意/损坏配置进入运行时。
4. **保持依赖更新**：定期运行 `cargo audit` 与 `cargo deny check`（CI 已内置），及时跟进传递依赖的安全通告。
5. **按需启用 feature**：只启用实际需要的 feature，缩小编译进二进制的代码面；高级 confers feature 会自动继承低级 feature（`encryption` → `reload` → `confers`）。
6. **单线程/多线程各取所需**：单线程场景使用 `Kit`（`!Sync`），不要绕过类型系统强行跨线程共享；多线程场景使用 `AsyncKit`。
7. **及时升级**：关注 [CHANGELOG](CHANGELOG.md) 中的安全类条目（如 0.4.1 的 `EncryptedBlob` Debug 修复），升级到包含修复的版本。
