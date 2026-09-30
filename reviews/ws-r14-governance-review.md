# ws-R14 治理复核记录 — trait-kit（T008/T051/T040 收尾）

> Generated: 2026-10-01 · 依据: `FEATURE_AUDIT_REPORT.md` §317（§9.2-6）与 base-roadmap-full-completion T008/T051/T040
> 复核范围: docs/ 旧名统一清零（T051）、全仓任务清零确认、async/observer/shutdown 组合抽查补充（T040）、async-kit-build-report 归档评估

## 0. 结论总表

| # | 复核项 | 结论 | 证据位置 |
|---|---|---|---|
| 1 | T008：tests/api_reference_gate.rs 脏文件归属 | **已入库**（三路审查 approved） | commit 64e6373 |
| 2 | T051：docs/ 目录旧 feature 名统一 | **已清零**（修复 34 处 + 1 处存量 clippy 告警连带修复） | commit a253c39 |
| 3 | T040：async/observer/shutdown 组合抽查 | **15/15 组合零错误零警告** | 本文 §3 |
| 4 | 全仓任务清零 | **成立**（T008 ✅ + T051 ✅ + T040 本文） | 本文 §4 |
| 5 | async-kit-build-report 归档建议 | **建议归档** | 本文 §5 |

## 1. T008 — tests/api_reference_gate.rs 脏文件归属

任务原文：单文件改动核对归属，完整则审查提交。结论：已随 64e6373（健康聚合门禁补全——health_aggregate/health_json sync/async 编译断言与文档收录行钉死）审查入库，`git log -- tests/api_reference_gate.rs` 末次触碰即该提交，工作区无残留脏文件。

## 2. T051 — docs/ 目录旧 feature 名统一（§9.2-6）

**旧名映射**（以 git 历史与 Cargo.toml `[features]` 为准）：

| 旧名 | 正名 | 改名出处 |
|---|---|---|
| `interface` | `di` | 323f244（正名迁移 78 处门控，旧名保留 deprecated 别名） |
| `scope` | `request-scope` | 同上 |
| `negotiate` | `version-negotiation` | 同上 |
| `confers-hot-reload` / `hot-reload` | `reload` | 1250cb1（drop confers- 前缀）及后续 rc 演进 |

**扫描与处置计数**（`grep` 前后对比）：

- 修复前：TEST_SCENARIOS.md 34 处 feature 语境旧名（`| interface |` ×12、`| scope |` ×7、逗号组合 ×9、总览/依赖链/组合矩阵/L2 命令等孤立形态 ×6）+ ARCHITECTURE.md Feature 分层图 1 行（含 interface/scope 两个旧名）。
- 修复后：feature 语境旧名 **0 处**；两文件 diff 共 40 行（TEST_SCENARIOS 37 行 + ARCHITECTURE 3 行）。

**分类处置**：

- **修复（活性文档 feature 语境）**：TEST_SCENARIOS.md 总览表 feature 列（L76/83/88）、场景矩阵涉及 feature 列（MET-07/08、ITF-01…09、SCP-01…10、LCY-09、TGL-08、CMP-07/08/11/12）、§3.1 依赖链图、§3.2 跨 feature 门控表、§3.3 组合矩阵 C7/C8/C11/C12、§5.1 L2 集成测试命令；ARCHITECTURE.md「Feature 分层」零依赖 feature 图 Z 节点。
- **合法保留（不改写）**：
  - 别名声明本身：API_REFERENCE.md:402/542（"旧 feature 名 X 保留为兼容别名"）——正是统一口径的记载载体；
  - 历史 CHANGELOG 章节：docs/CHANGELOG.md:87/129/225/308/330/363——按 323f244 确立口径"历史章节保持当时事实不改写"；
  - 构建路径概念词："eager/lazy/multi/interface 四条构建路径"、`BuildFailed{context=interface}`、CMP-12 描述中的 interface TypeId——代码事实（`build_interface_modules`），非 feature 名；
  - 示例二进制名：`interface`、`scope_basic`（examples/ 注册名）及批跑脚本中的同名列举；
  - 模块/类型/文件名：`src/kit/scope.rs`、`Scope`/`AsyncScope`/`create_scope`、ARCHITECTURE 组件图 SC 节点（模块语境）。

**附带修正与发现（同提交）**：

- ARCHITECTURE.md feature 计数漂移："18 个可选 feature" 实为 19 个正名（+3 个 deprecated 别名），分层图漏 `probe`——已按 Cargo.toml 修正计数并补 probe 节点。
- src/lib.rs `MockError` 缺 `#[cfg(all(test, feature = "async"))]` 门控：meta.rs:435 注释早已声明该契约而定义侧缺失，默认组合下 `cargo clippy --all-targets` 存量 dead_code 告警——已补门控消零（存量告警，非本次文档改动引入）。

## 3. T040 — cargo-hack 组合抽查（async/observer/shutdown 矩阵）

cargo-hack 0.6.45 已安装，直接使用 powerset：

```
cargo hack check --feature-powerset --depth 3 \
  --exclude-features confers,reload,encryption,di,interface,lifecycle,health,probe,\
request-scope,scope,toggle,decorator,i18n,report,presets,presets-remote,compose,version-negotiation,negotiate
```

即 {async, observer, shutdown, default} 四元素的 depth-3 powerset，**15/15 组合全部编译通过，0 错误 0 警告**。核心目标组合全覆盖：

| 组合 | 结果 |
|---|---|
| `async` 单开 | ✅ |
| `observer` 单开 | ✅ |
| `shutdown` 单开 | ✅ |
| `async,observer` | ✅（c5ae7bc 曾复测零告警，本次复核一致） |
| `async,shutdown` | ✅ |
| `observer,shutdown` | ✅ |
| `async,observer,shutdown` 三开 | ✅ |
| 其余 8 组（含 `default` 元素镜像与空集） | ✅ |

背景：323f244 已做 each-feature（25 次编译检查）与 powerset depth-2（254 组合）全量，`.github/workflows/feature-matrix.yml` 每周定时守卫；本次为任务点名的三 feature 深组合（depth-3）抽查补充。

## 4. 全仓任务清零结论

base-roadmap-full-completion 中 trait-kit 仓库范围任务：T008 ✅（64e6373）、T051 ✅（a253c39，本文 §2）、T040 ✅（本文）。tasks.md 其余未勾选项（T017/T039/T044/T045/T046/T049）分别归属 inklog/confers/limiteron/sdforge，不属 trait-kit 仓库。**trait-kit 全仓任务清零成立。**

## 5. async-kit-build-report 归档建议

**建议：归档。** 理由：

1. tasks.md T001-T009 全部勾选，T009 附 2026-09-23 验证记录（async,report lib 263 通过；all-features 22 个测试二进制全过；report 单开 check 通过；clippy 零警告）；
2. Convergence Phase（2026-09-23）缺口 0（CRITICAL/HIGH/MEDIUM/LOW 全 0），验收标准表 R-async-build-report-001…004 与 Constraints 全部 ✓ PASS；
3. git 落地证据在库：`src/kit/async_kit.rs:1928/1938`（`build_report`/`contract_manifest` 访问器）、`src/kit/report.rs:139`（`AsyncReportFields`）、`docs/API_REFERENCE.md:153-154`（两行收录）、`tests/api_reference_gate.rs:217-218/489-490`（编译存在性 + 文档收录钉死断言）；
4. 无遗留缺口：变更内预先存在的 compile_fail（trybuild stderr）基线即挂属独立维护事项（rustc 版本漂移），已在 convergence 记录披露，不阻塞归档。

## 6. 验证基线

- `cargo fmt --check` 通过（a253c39 门禁）
- `cargo clippy --workspace --all-targets --quiet` 0 警告（MockError 门控补齐后）
- `cargo test --workspace --quiet` 退出码 0（25 个测试二进制 0 failed）
- pre-commit 钩子 10 项全 Passed（含 cargo check / cargo clippy / typos / detect-secrets）
- cargo-hack powerset depth-3（async/observer/shutdown）15/15 通过
