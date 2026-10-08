# 更新日志

本项目所有显著变更将记录在此文件中。

格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.0.0/)，
并遵循 [语义化版本](https://semver.org/lang/zh-CN/v2.0.0.html)。

## 📋 目录

<details open>
<summary>📑 目录</summary>

- [Unreleased](#unreleased)
- [`0.5.0-rc.7`](#050-rc7---2026-10-03)
- [`0.5.0-rc.6`](#050-rc6---2026-09-21)
- [`0.5.0-rc.5`](#050-rc5---2026-09-14)
- [`0.5.0-rc.4`](#050-rc4---2026-09-13)
- [`0.5.0-rc.2`](#050-rc2---2026-09-03)
- [`0.4.2`](#042---2026-08-06)
- [`0.4.1`](#041---2026-08-06)
- [`0.4.0`](#040---2026-08-04)
- [`0.3.1`](#031---2026-07-22)
- [`0.3.0`](#030---2026-07-13)
- [`0.2.5`](#025---2026-07-12)
- [`0.2.4`](#024---2026-07-11)
- [`0.2.3`](#023)
- [`0.2.2`](#022)

</details>

---

## [Unreleased]

### Changed

- **proc-macro 子 crate 目录更名**：`trait-kit-macros/` → `macros/`，与 base 其余仓的子 crate 目录惯例（confers/oxcache/dbnexus/limiteron/sdforge 均为 `macros/`）统一。包名 `trait-kit-macros` 不变（crates.io 已注册且下游按名依赖），发布产物与用户可见 API 零影响；同步更新 workspace `members` 与两处 `path` 依赖（主 crate 与 `examples/`）、`release.yml` 发布步骤的 `working-directory`、`typos.toml` 的 trybuild 夹具豁免 glob、`deny.toml` 注释指向，以及 README/README_EN 的目录表格与测试规模引用

---

## [0.5.0-rc.7] - 2026-10-03

### Added

- **feature 组合矩阵抽查修复与文档旧名收尾**（workspace feature 审计遗留复核）：`cargo hack check --each-feature`（22 个非 default feature 单开 + default/no-default/all-features 端点，共 25 次编译检查）暴露三处编译裂缝并修复——`presets-remote` 补 imply `async`（remote 模块用 `AsyncKit` 构建，此前单开编译失败 E0432）、`src/kit/presets.rs` 的 `tr` 导入按唯一调用点补 `presets-remote` 门控（presets 单开 unused 警告）、`src/kit/toggle.rs` 的 `HashSet` 导入按 confers 门控（toggle 单开 unused 警告）；单开矩阵复跑归零，两两组合 powerset（`--feature-powerset --depth 2`，1+22+231=254 组合）零错误；防复发双门禁：`src/kit/presets.rs` 顶部 `compile_error!` 守卫 presets-remote→async 链（单开组合即编译失败），新增 `.github/workflows/feature-matrix.yml`（每周定时+手动触发 each-feature 与 powerset depth-2）。文档旧名统一：`negotiate`/`scope`/`interface` 裸名在 API_REFERENCE（方法 Feature 列+节名）、USER_GUIDE、ARCHITECTURE、README/README_EN（feature 表与导览 bullet，README_EN 补别名声明段）、examples/Cargo.toml（旧名 feature 补 deprecated 注释，`scope_basic`/`interface` 两示例 required-features 迁正名并编译验证；行为变化：以旧名跑迁移后的示例（如 `--features scope --example scope_basic`）不再可用，错误提示直接给出正名 feature——required-features 为 AND 语义无法双名兼容，示例作为教学入口示范正名全部改为正名 `version-negotiation`/`request-scope`/`di`，历史 CHANGELOG 章节保持当时事实不改写
- **性能回归 CI 阈值门禁**：ci.yml 新增 bench job——跑 `cargo bench --features toggle,confers` 并经 `scripts/bench_gate.py` 断言 7 个基准的中位数（criterion `estimates.json`）不超过本地基线 ×100 的数量级上限（阈值推导、基线台账与刷新规则见 docs/bench-baseline.md，刷新需三处一致变更：台账/LIMITS_NS/PERFORMANCE.md 基线段）；仓库变量 `BENCH_GATE_DISABLED=true` 可整体停用；脚本对未纳管基准目录打 WARN（防新增基准静默逃逸门禁），criterion 输出按 7 天保留上传 artifact。定位为灾难性回归护栏（≥100× 悬崖），严格回归检测仍在同机本地基线流程做，docs/PERFORMANCE.md 已同步口径与免责
- **async 健康聚合访问器**（TK-D，`health` × `report`）：`AsyncKit<Ready>::health_aggregate()` 与 `health_json()`——worst-of 整体状态 + per-module 明细（`/healthz` 结构化载荷与 JSON 导出），与同步 `Kit::health_aggregate` 口径逐点对位（空集 healthy-by-convention、worst-of 取 severity_rank 最大值——modules 明细顺序不承诺确定、serde_json 错误直传不伪装健康），复用既有 `AsyncKit::health_report` 读取基建（含 HealthChanged 事件发布路径）
- **服务探针注册面**（蓝图 TK-A，新 feature `probe=["health","dep:futures-timer"]`）：对象安全 `ServiceProbe` trait（`ProbeOutcome{status,latency}`，手写 `Pin<Box<dyn Future>>` 分派对齐 `AsyncLifecycle`，不引 `async-trait`）；`AsyncKit` 任意状态可 `register_probe`/`unregister_probe`/`probe_names`（注册序=执行序，同名重注册落到尾部）；`AsyncKit<Ready>::run_probes()` 顺序执行全部探针产出 `ProbeReport`（per-probe 状态+**框架实测墙钟延迟**+worst-of 聚合，`report` 特性下可 `to_json()`，空注册表 healthy-by-convention）与 `probe_aggregate()`（worst-of 结论，复用 `severity_rank`，无短路）；`run_probes_with_timeout()`/`probe_aggregate_with_timeout()` 以运行时无关定时器（`futures-timer`，全局后台线程驱动）给单探针硬上限——悬挂探针记 unhealthy（detail 含 timed out）并继续跑完其余探针，最坏总延迟 ≤ 探针数 × timeout；`shutdown_async()`（probe+lifecycle）与 `register_shutdown_into()`（probe+lifecycle+shutdown）自动注销全部探针并标记 stopped。健康面三者分工显性化：`HealthCheck`/`AsyncHealthCheck::check` 刻意同步只读缓存，`ServiceProbe` 承载真正的网络探活
- **构建报告 overrides 语义显式化**（async-kit-build-report 跟进项，`report` × `confers`）：`BuildReport` 新增 `config_overrides: Vec<ConfigOverrideRecord>` 字段（0.x 语义下 additive，schema 仍为 1；穷举构造 `BuildReport` 的代码需同步补字段），记录 `merge_config`（sync 与 async）每次调用的 `{ config, source, applied }`——目标配置不存在时 `applied=false`，被丢弃的 override 不再静默吞并。顺序语义：记录序在顺序调用/单线程下即调用顺序，`AsyncKit` 并发调用下为加锁到达序、不代表实际应用顺序（读-改-写非原子，同类型并发调用可能互相丢失覆盖，需独占写语义的调用方自行串行化）；`applied=true` 表示本次调用执行了 apply，不承诺最终配置仍包含该 override。模块级 `overrides` 语义不变（`override_module` 家族 sync-only，async 报告恒为空）；另新增 `config/write_merge_config` 基准（`toggle,confers` 组合且仅在无 `report` 组合下测量——report 下记录无界累积会使迭代式基准失真，见 PERFORMANCE.md）

### Fixed

- **三路审查修复**（probe 面与 overrides 面的 MEDIUM/HIGH 项）：停机协议清空探针注册表后 `run_probes()`/`probe_aggregate()` 不再误报 healthy——`ProbeReport` 新增 `stopped: bool`（穷举构造方需同步），停机后返回显性不可服务结论（`overall=="unhealthy"`、`stopped==true`），防止就绪端点向死实例导流；`ProbeEntry.latency_ms` 改为框架在 `probe().await` 外侧实测的墙钟延迟，自报的 `ProbeOutcome.latency` 降级为实现方诊断字段、不进就绪载荷；`prs03` 门禁 cfg 列表补 `probe`（此前缺 probe 的组合下测试静默通过，门禁声明失效）；`config/write_merge_config` 基准在 `report` 组合下编译剔除（无界累积使测量失真且有 OOM 风险）；CI 中 `github/codeql-action` 三处引用由可变 tag 固定为 commit SHA
- **版本协商四路径奇偶**（fix-audit-defects-r1 批次 A）：`register_lazy`/`register_multi`/`register_as` 此前不登记 `ModuleMeta::VERSION` 与 `required_versions`，版本冲突被静默跳过（fail-open）；现四条注册路径统一登记，`build()` 对任何路径的不兼容版本返回 `VersionIncompatible`
- **register_as i18n/契约登记**：`register_as` 此前丢弃模块 `i18n_ftl()` 片段且不入 contract manifest；现与 eager 路径一致（contract 的 capability 字段记接口类型名）
- **lazy 检索口径统一**（批次 B）：`contains`/`optional`/`require_ref` 此前对 `register_lazy` 模块 `require()` 后的缓存失明；现与 `require`/`get_arc` 口径一致——"已构建即可见"，且全部只读查询不触发 lazy 构建
- **关闭协调器 hook panic 隔离**（批次 C）：`ShutdownCoordinator`（sync）与 `AsyncShutdownCoordinator`（async，经零依赖 `CatchUnwindFuture`）的单个 hook panic 不再中断关闭流程，计数进 `ShutdownPhaseResult::hook_failures`
- **lazy builder panic 可恢复**（批次 C）：`Kit::require` 与 `Scope::require` 的首建路径 builder（或 decorator）panic 被隔离为 `BuildFailed`（含 panic 摘要），builder 放回槽位保持可重试
- **semver 边界**（批次 D）：build metadata（`+build-x`）不再被误判为 prerelease；prerelease 标识符按 semver §11.4 比较（`rc.1` < `rc.2`、数字段数值比较、数字段 < 字母数字段）

### Changed

- **API**：`ProbeReport` 新增 `stopped: bool` 字段（结构体字面量构造方需同步）；`register_probe` 签名从泛型 `Arc<P>` 收紧为 `Arc<dyn ServiceProbe>`（与 `with_observer` 惯例对齐，调用点源码不变）；`ServiceProbe::probe` 文档将"实现内自设超时"从建议升级为契约，并要求取消安全（有界变体在超时处 drop 探针 future）
- **API**：`Kit::take_config_overrides()` / `AsyncKit::take_config_overrides()`（`report` 特性）——排空并返回 `config_overrides` 历史（记录序），高频 `merge_config` 的长生命周期 Kit 在轮转点调用以防无界累积（每条约 40B，`build_report()` 快照整段 clone）；`BuildReport::config_overrides` 文档从权衡说明升级为使用警告
- **依赖**：新增 optional `futures-timer 3.0`（`default-features = false`，经 `probe` feature 挂载）——运行时无关的全局定时器线程，超时变体所需；不引入 tokio/async-std 耦合
- **依赖**：`confers` req 0.6.0-rc.5 → 0.6.0-rc.6（crates.io）——0.6.0-rc.6 起 `derive_field_key` 返回 `Zeroizing<[u8; 32]>`（密钥材料 drop 自清零），trait-kit 的 `derive_kit_field_key` 透传该容器并需显式命名类型，为此新增 optional `zeroize 1.9`（`default-features = false, features = ["alloc"]`，经 `encryption` feature 挂载）
- **API**：`ShutdownPhaseResult` 新增 `hook_failures: usize` 字段（结构体字面量构造方需同步）；`is_ok()` 语义收紧为 `!timed_out && hook_failures == 0`，`into_result()` 对"仅 hook panic"场景返回 `BuildFailed`
- **API**：`AsyncShutdownCoordinator::pending_local_hook_count()` 返回类型改为 `Result<usize, TraitKitError>`——锁中毒不再 panic 击穿宿主监控线程，与同 impl 块 `register_local_hook`/`shutdown_local` 的错误纪律一致；`AsyncKit::register_shutdown_into` 桥接部分失败时，返回的 `TraitKitError::BuildFailed` 以 i18n context 报告桥接操作/阶段与 stranded 计数、原错误整体降为 source（包装对任意错误变体通用），程序化检测清理丢失不再依赖 log subscriber
- **文档**：README/README_EN/SECURITY 的"无 unsafe"表述修正为"默认禁用 + 6 处经审计豁免"；API_REFERENCE 补 lazy 模块检索口径
- **文档漂移批量修正**（以代码实测为准）：USER_GUIDE/CONTRIBUTING 的"无 unsafe"表述同步为 6 处豁免口径；README/README_EN/USER_GUIDE 修正默认依赖表述（`fluent-bundle`/`unic-langid`/`log` 为 Always-on 必选依赖）与 feature 计数 18→19；workspace 成员表述统一为三个（`trait-kit-derive` 已并入 `trait-kit-macros`，含 ARCHITECTURE 架构图双节点合并）；README/README_EN 修正 typestate 编译期排除范围（`optional()` on `Unbuilt` 与 register/build on `Ready`，`require()` 双态可编译）与 `set_config`/`reload_config` 态归属；`graph_dot()`/`graph_mermaid()` 门控归属修正（无门控，仅 `build_report()`/`contract_manifest()` 属 `report`）；API_REFERENCE 修正 `create_scope_from` 签名（`&Rc<Kit>` 弱引用，不消费 Kit）并补录 `#[derive(Module)]`；测试计数 795→911；examples/README 运行命令迁正名 feature（`request-scope`/`di`）；CONTRIBUTING pre-commit 钩子表对齐实际（无 cargo-deny、大文件阈值 1000KB）；SECURITY 修正发布扫描工具表述（实际门禁为 cargo-audit+cargo-deny/CodeQL/detect-secrets）；CHANGELOG 正文版本节恢复时间倒序（rc.6→rc.5→rc.4）
- **文档**：README/README_EN 路线图关闭"cfg 门控完整性"条目——`--no-default-features --features async` 与 `async,observer` 组合 check、`clippy --features async --all-targets` 零告警，`cargo test --features async --doc` 全通过，async × observer 门控缺口经复测确认已消除
- **依赖**：新增非 optional `log 0.4`（`default-features = false, features = ["std"]`，零传递依赖 facade）——新增的 `soft_build` 可降级构建助手的 `error!` 降级日志所需；所有消费者升级 rc+1 后依赖闭包将新增 `log`
- **构建**：`shutdown.rs` 的 `crate::i18n::tr` 导入补 `#[cfg(feature = "async")]` 门控，`shutdown`∧¬`async` 组合（含 `lifecycle,shutdown`、`observer,lifecycle,shutdown`）不再产生 `unused_imports` 告警，`-D warnings` 构建在这些组合下恢复可用
- **依赖**：default-features 显式关闭批量裁剪——serde/serde_json/writeable/sys-locale 及 dev 侧 thiserror/serial_test/static_assertions/trybuild/criterion/async-trait 逐一显式声明所需特性；`trait-kit-macros` 的 syn 3.0 移除未使用的 extra-traits（quote/proc-macro2 特性声明统一）——下游宏编译不再平添 syn AST 调试 trait 的 impl 生成成本

---

## [0.5.0-rc.6] - 2026-09-21

### Added

- **AsyncKit 版本协商补齐**：negotiate feature 下 AsyncKit 与同步 Kit 同口径的版本兼容校验

### Changed

- **i18n 整改**：接入 unify-rust-i18n 统一错误与消息文案
- **confers 依赖改走 crates.io**：移除跨仓 path，req 升 0.6.0-rc.5
- **依赖升级**：syn 2.0.119 → 3.0.5、criterion 0.7.0 → 0.8.2、rustls 0.23.45（RUSTSEC-2026-0285）
- **供应链与工程加固**：detect-secrets 基线、pre-commit 门禁、typos 词表白名单、path-only 依赖补全 version 字段
- **发布流程加固**：先发 trait-kit-macros 再发主 crate，crates.io 发布后硬校验，校验 curl 补 User-Agent 并加重试

### Fixed

- **encryption 继承链对齐**：di/request-scope/version-negotiation 特性正名
- ReloadSubscriber 别名与 AsyncDecoMod 测试模块随使用处特性门控

---

## [0.5.0-rc.5] - 2026-09-14

### Changed

- **trait-kit-derive 并入 trait-kit-macros**：ConfigInherit/SharedConfig 迁移，derive 子包退役；PRE-03 契约与 docs/deny.toml 同步，CI 门禁收敛

### Fixed

- cargo-deny path-only 通配依赖违规：trait-kit-macros 的 dev-dependency trait-kit 补 version 声明
- release 工作流 publish 步骤幂等容错：本地先行发布/重跑 tag 撞车不阻断 Release 交付

---

## [0.5.0-rc.4] - 2026-09-13

> 注：0.5.0-rc.3 未单独发布（无 tag、未上 crates.io），本节内容含原 rc.3 开发批次，随 0.5.0-rc.4 一并发布。

### Added

- **观测端口**：新增 `src/kit/ports.rs`，定义 `MetricsPort`（counter/gauge/histogram）与 `LogPort`（结构化 log）trait + `NoOpMetricsPort`/`NoOpLogPort` 零开销默认实现；`Kit` 与 `AsyncKit` 构建器可注入 `Option<Arc<dyn MetricsPort>>`/`Option<Arc<dyn LogPort>>`
- **Toggle 完整落地**：`src/kit/toggle.rs` 从 doc-only 升级为完整开关句柄——`ToggleBackend` trait + `ToggleValue` 类型化枚举（Bool/Int/Float/Str）+ `MemoryToggle` 内存后端；`confers` feature 启用时自动切换为 `ConfersToggle`（委托 `confers::FeatureToggleRegistry`）；Kit/Kit\<Ready\> 新增 `set_toggle`/`get_toggle`/`remove_toggle`/`list_toggles` 方法
- **AsyncKit 配置对称**：补齐 `load_config` 系列、`subscribe`/`reload_config`、`snapshot_config`/`restore_config`、`set_encrypted`/`get_encrypted` 等 11 个方法，与同步 Kit 能力一致
- **运行时事件总线**：`EventBus` port + `MemoryEventBus`（订阅者 panic 隔离）+ `NoOpEventBus` 零开销默认；`Kit`/`AsyncKit` 经 `with_event_bus` 注入，在模块构建（`ModuleBuilt`）、健康采样（`HealthChanged`）、配置变更（`ConfigChanged`）关键点发布，公开 `emit_event` 供模块自定义事件
- **AsyncKit 并发构建**：按拓扑分层对无依赖模块并发 `build`（`BatchJoin` 限流 join，`with_max_concurrency` 上限控制），依赖序语义与既有 e2e 行为完全保持
- **密钥提供方抽象**：`KeyProvider` port + `KeyBytes` 零化容器 + `ConfersKeyProvider` 适配 confers `SecretKeyProvider`；`set_encrypted_with_key_provider` 调用时取钥（fail-closed）
- **加密密钥轮换**：加密存储带 key-version 信封；`set_encrypted_with_version`/`get_encrypted_with_version`/`rotate_master_key` 旧钥迁移（错钥 fail-closed，版本递增）
- **健康聚合导出**：`health_aggregate()`/`health_json()` 输出 worst-of 整体状态 + 各模块明细，供 /healthz 直接消费
- **健康历史环形采样**：`HealthSample` 环形缓冲（容量可配）+ `record_health_history()`/`health_history()`
- **Scope 父上下文**：`create_scope_from` + `scope.parent::<T>()` 只读查询父 Kit 能力，Weak 引用循环防护
- **子 Kit 组合**：`SubKitSpec`/`SubKitModule`/`SubKitHandle`，child Kit 作为父 Kit 单模块注册（能力命名空间隔离 + 跨 Kit 依赖图验证）
- **i18n 模块化**：`ModuleMeta::i18n_ftl` 模块自带 .ftl 片段，`build()` 合并为 kit 本地翻译 overlay（`module_tr` 优先、全局 `tr()` 兜底）
- **强类型开关句柄**：`ToggleKey`/`ToggleHandle<T>` + `define_toggle_key!` 宏，编译期防 key 拼写错
- **零克隆读取**：`get_arc::<M,T>()` Arc 能力免克隆检索（同指针）；`set_config_arc`/`config_arc` Arc 配置快照（sync + async）
- **装饰器契约前移**：`try_decorate` 注册时校验目标已注册（`DecoratorTargetMissing`），类型由泛型钉死
- **能力版本协商**：`ModuleMeta::VERSION`/`required_versions` + `negotiate` feature 构建期 semver-compat 校验（`VersionIncompatible`）
- **配置变更审计**：`set_config`/`set_config_arc`/`reload_config`/`restore_config` 经 EventBus 发布 `ConfigChanged` 审计事件（含变更摘要）
- **API 文档一致性门禁**：`tests/api_reference_gate.rs` 关键项编译存在性 + docs/API_REFERENCE.md 收录双向断言
- **presets feature**：`ConfersConfigModule` 把 confers ConfigProvider 包装为 Kit 模块 + Presets 组合 builder
- **presets-remote 远程配置源桥接**：把 confers AsyncSource 快照包装为 AsyncKit 模块（dot-path `RemoteConfigProvider` + 审计事件）
- **build_report() / contract_manifest()**：结构化构建报告（模块状态/拓扑/耗时/override 来源）与注册模块契约清单 JSON（report feature）
- **require 错误细分**：`CapabilityTypeMismatch` variant + `ErrorKind::kind()`（Missing/InitFailed/TypeMismatch/Other）
- **trait-kit-macros 子 crate**：`#[derive(Module)]` 生成 ModuleMeta（name/deps，trybuild 负面用例）
- **criterion 基准套件**：build/require/config/toggle + docs/PERFORMANCE.md 基线

### Changed

- `reload` feature 移除 `confers/watch` 依赖（自有 SubscriberMap 机制不受影响）
- `confers` feature 启用 `confers/feature-toggle` 以接入 `FeatureToggleRegistry`
- `confers` 依赖升至 0.6.0-rc.4 并移除 `[patch.crates-io]` 本地口径（改由 crates.io 解析）
- 文档套件重写：中英双语 README（workspace 架构图/build 生命周期时序图/特性全表）、文档归入 docs/ 并统一优化

### Fixed

- `derive_kit_field_key` 改为 `pub(crate)` 供 `async_kit.rs` 跨模块访问
- OCR 审查修复组：事件总线回调持锁死锁、密钥擦除补 `inline(never)`、`ConfersToggle::remove` 补移除语义、`to_json`/`health_json` 返回 `Result`、`interpolate_json_value` 迭代化
- lifecycle 测试共享计数器互斥串行（并行 build 各触发 on_ready 的计数污染致偶发红）

---

## [0.5.0-rc.2] - 2026-09-03

### 文档

- 同步 README/USER_GUIDE 等文档中的版本号 0.4 → 0.5.0-rc.2
- 同步 Kit API 表格：按 `#[derive(Kit)]` 宏生成的 `Kit` 结构体实际表项展开

### Changed

- 依赖路径本地化：`confers` 在 `[workspace.dependencies]` 改用 `path` + `version` 双写，供本地联调与 CI 发布模式共用
- 版本号递增至 `0.5.0-rc.2`（下一个 minor 预发布）

## [0.4.2] - 2026-08-06

### 修复

#### AsyncKit decorator 存储 key 错误
- `AsyncKit::decorate()` 将 decorator 存储在 `TypeId::of::<M>()`（模块 TypeId）下，
  但 `apply_decorators()` 按 `TypeId::of::<M::Capability>()`（能力 TypeId）查找，
  导致 decorator 永远不会被应用，返回未装饰的原始能力。
- 修复：存储 key 改为 `TypeId::of::<M::Capability>()`，与同步 `Kit::decorate()` 对齐。
- 影响范围：所有使用 `AsyncKit::decorate()` 的异步模块。

## [0.4.1] - 2026-08-06

### 新增

#### 国际化增强
- ICU4X 重依赖门控在 `i18n` feature 后，无 feature 时 `tr()` 和 `I18nManager` 仍可用（轻量 FTL 翻译）
- Kit 构造点诊断标记支持翻译键
- `I18nError::Display` 修复

#### 优雅关闭协调器补充
- 关闭协调器 example + 集成测试 + doctest 修复

#### 配置扩展 API 文档
- 补充 `load_and_validate`、`snapshot_config`、`restore_config`、`has_snapshot`、`load_config_with` API 文档
- 补充 `toggle`（`enable_toggle`/`is_toggle_enabled`/`register_if_toggle`）API 文档
- 补充 `Validatable` trait 和 `interpolate_json_value` 函数文档
- 新增 `snapshot_restore`、`toggle_basic`、`validation` example

### 修复

- **安全**: `EncryptedBlob` Debug 实现不再泄露加密材料
- **异步**: `decorate()` 装饰器在 `build()` 中从未生效的问题已修复
- 多项 bug 修复 + 文档示例修复 + 测试门控修正
- CI clippy 与 dead_code 错误修复

### 性能

- `Kit::require()`、`reload_config()`、`transfer_lazy_builders()` 优化
- `find_cycle()` 使用 HashMap 实现 O(1) 栈位置查找

### 重构

- `TraitKitError` 的 context/key 字段改为 `String` 支持翻译文本
- `EncryptedBlob` 字段封装在构造器和 getter 后
- examples 按模块结构重组

### 杂项

- workspace 元数据继承 + `trait-kit` 加入 `workspace.dependencies`

## [0.4.0] - 2026-08-04

### 新增

#### 生命周期管理（feature = "lifecycle"）
- `Lifecycle` trait — 同步生命周期钩子：`on_ready`（构建后）+ `on_shutdown`（清理）
- `AsyncLifecycle` trait — 异步生命周期钩子（需同时启用 `async`）
- `Kit::register_lifecycle::<M>()` / `AsyncKit::register_lifecycle::<M>()`
- `Kit::shutdown()` / `AsyncKit::shutdown()` — 按逆拓扑序执行 `on_shutdown`
- `TraitKitError::LifecycleFailed` 变体

#### 健康检查（feature = "health"）
- `HealthCheck` / `AsyncHealthCheck` trait — 模块运行时状态报告
- `HealthStatus` 枚举（`Healthy` / `Degraded` / `Unhealthy`）
- `Kit::register_health_check::<M>()` / `Kit::health_check::<M>()`

#### 作用域依赖（feature = "scope"，0.5.0 起正名为 `request-scope`，旧名保留为别名）
- `Scope` — 基于 `RefCell` 的轻量级每请求实例隔离容器
- `AsyncScope` — `Send + Sync` 异步作用域（需同时启用 `async`）

#### 条件注册（feature = "conditional"）
- `Kit::register_if::<M>(predicate)` — 运行时谓词控制的模块注册

#### 构建可观测（feature = "observability"）
- `BuildObserver` trait — 构建管线回调（`on_module_start` / `on_module_built` / `on_build_error`）
- `Kit::with_observer(obs)` / `AsyncKit::with_observer(obs)`

#### 工厂模式（feature = "factory"）
- 每次调用创建新实例（非单例）

#### 模块装饰器（feature = "decorator"）
- `Kit::decorate::<M>(f)` — 构建后能力包装/增强

#### 国际化增强
- `I18nManager` + `tr()` — 基于 Fluent FTL 的中英文消息翻译，系统环境自动检测
- `I18nFormatter` — ICU4X 驱动的数字/日期/复数/排序格式化
- `TraitKitError` Display 实现通过 `tr()` 自动本地化输出
- `icu`、`writeable`、`sys-locale` 成为必需依赖（不再通过 feature 门控）

### ⚠️ BREAKING CHANGES

- **TraitKitError 移除 `thiserror` derive** — 不再 `#[derive(Error)]`，改为手动实现 `Display` + `std::error::Error`；`Display` 输出通过 `tr()` 自动本地化
- **TraitKitError::NotReady 变体移除** — 已在 0.2.x 标记 `#[deprecated]`，现正式移除
- **TraitKitError 新增变体** — `MissingConfig`（无条件）和 `LifecycleFailed`（需 `feature = "lifecycle"`）；现有 `match` 需补充分支
- **`impl_module_meta!` 依赖名生成逻辑变更** — 从 `stringify!($dep)` 改为 `<$dep as ModuleMeta>::NAME`，依赖解析名称可能与之前不同
- **`Kit::load_config_or_default` 返回类型变更** — `Result<(), TraitKitError>` → `Result<bool, TraitKitError>`（`true` 表示加载成功，`false` 表示使用默认值）
- **i18n 变为必选依赖** — `icu`、`writeable`、`sys-locale` 不再通过 feature 门控，所有用户均编译 i18n 模块；`i18n` feature flag 移除
- **`ModuleMeta::dependencies()` 新增默认实现** — 默认返回 `&[]`，无依赖模块不再需要手动实现
- **`TraitKitResult<T>` 正式导出** — 移除 `#[allow(dead_code)]`，通过 `lib.rs` 公开 re-export

### 变更

- `examples/integration-app` 重命名为 `examples/trait-kit-example`（独立 workspace 成员）
- `kit.set_config()` 现在可在 `Kit<Unbuilt>` 和 `Kit<Ready>` 上调用
- 错误模块路径 `src/core/error.rs` → `src/error.rs`

### 依赖

- `confers` 0.4 → 0.5（传递依赖升级：notify 7→8、hkdf 0.12→0.13、sha2 0.10→0.11 等）
- `serial_test` 3 → 4.0（dev-dependency）
- 统一依赖版本写法为 `x.x` 格式（serde/serde_json/thiserror/trybuild/static_assertions）

### 构建

- 新增 `[workspace.dependencies]` 集中管理 `confers` 和 `serde`，examples crate 通过 `workspace = true` 继承

## [0.3.1] - 2026-07-22

### 修复

- `kit.rs` 重构：裸指针转型改为 `.cast::<Kit>()`（更安全），`require_all` 消费所有权避免迭代器复用错误

### CI / 依赖

- 新增跨平台 CI 矩阵（ubuntu/macos/windows）
- 修复 `cargo fmt` + `clippy` 在 main 的失败
- 解决 `integration-app` 与 dbnexus `default=[]` 的兼容性
- 移除 `examples/integration-app`（仅本地依赖 sibling path crate，不发布）
- 依赖 bump：github/codeql-action 3→4、actions/checkout 4.2.2→7.0.0、trybuild 1.0.117→1.0.118

### 测试

- 新增 `tests/e2e_advanced.rs`（78 个测试）：覆盖 B01/B03/B04/B06/B07/B09/B11/B12/B13、A01-A08/A25、E03/E05/E06/E08/E11/E19/E26、C01-C04/C06-C08/C10/C11/C13/C22/C23、A09-A12/A26/E23-E25/C19、A19/A20/E07/E27、I01-I19/E20-E22/C20/C21、C14/C15/E15/E17 等场景，补全现有测试套件的覆盖盲区

## [0.3.0] - 2026-07-13

### 新增

#### Phase 1: Override + require_ref
- `Kit::override_module<M>()` — 用预构建值覆盖模块能力，跳过 build_fn（测试注入）
- `Kit::override_module_strict<M>()` — 覆盖但验证依赖存在性
- `Kit::require_ref<M>()` — 零拷贝能力检索，返回 `Ref<'_, M::Capability>`
- `TypeMap::inner_ref()` — 暴露内部 HashMap 借用

#### Phase 2: Lazy + 多绑定
- `Kit::register_lazy<M>()` — 延迟构造，首次 `require()` 时触发构建并缓存
- `Kit::register_multi<M>()` — 多绑定注册，相同能力类型聚合为 Vec
- `Kit::require_all<M>()` — 按注册顺序返回所有多绑定能力

#### Phase 3: 接口分离（feature = "interface"，0.5.0 起正名为 `di`，旧名保留为别名）
- `Interface` marker trait — 支持 `dyn Trait` 类型擦除（`?Sized` blanket impl）
- `InterfaceBuilder` 扩展 trait — 关联 `Capability` 与 `Interface`，通过 `into_interface` 执行类型擦除
- `Kit::register_as<M>()` — 按接口类型注册，`M::Interface` 作为 key
- `Kit::resolve<I>()` — 按接口类型检索 `Arc<I>`

#### Phase 4: 宏扩展
- `impl_module_meta!` 宏 — 生成 `ModuleMeta` impl（无依赖 / 有依赖两种语法）
- `impl_async_auto_builder!` 宏（feature = "async"）— 生成 `AsyncAutoBuilder` impl

#### 跨平台与集成
- 跨平台 CI 矩阵（ubuntu/macos/windows）验证 apple/windows/linux 平台兼容性
- `examples/integration-app`：dbnexus default=[] 后显式启用 default-no-db + sqlite + kit
- `examples/integration-app`：新增 [patch.crates-io] 解决 trait-kit 版本冲突
- `examples/integration-app`：governor 改为 re-export `limiteron::Governor`（修复私有模块访问）

### 变更

- MSRV 从 1.85 提升至 1.91
- clippy/fmt 修复
- `build()` 方法优先检查 overrides map，跳过 build_fn
- `build()` 新增 lazy_slots / multi_capabilities / interface_builders 构建循环
- `build()` 中 topo-sorted 循环对 multi-binding 和 interface 模块 `continue`（与单绑定模式一致）
- 移除 trait-kit 对下游 crate（oxcache/dbnexus/limiteron/sdforge/inklog）的 dev-dependencies 循环依赖；e2e 测试由 `examples/integration-app` 承担

## [0.2.5] - 2026-07-12

### ⚠️ BREAKING CHANGES

- `KitError` 重命名为 `TraitKitError`，遵循 `ProjectNameError` 命名约定
- 新增 `TraitKitResult<T>` 类型别名
- `error` 模块从 `src/core/error.rs` 迁移到 `src/error.rs`，导入路径 `crate::core::error::KitError` → `crate::error::TraitKitError`

## [0.2.4] - 2026-07-11

### 变更

- 无代码变更，版本号对齐 workspace 同步升级

### 变更（Phase 6 前置）

- 升级至 Rust edition 2024
- 最低支持 Rust 版本 (MSRV) 设为 1.85
- 统一采用 MIT 许可证

### 新增

- `i18n` feature：集成 ICU4X，提供区域感知的数字、日期、复数和排序能力
- `async` feature：`AsyncKit` 支持 `Send + Sync` 异步能力管理

## [0.2.3]

### 新增

- `ModuleConfig` trait：模块级配置元数据（PATH + default_value）
- 四级 confers feature flag 体系（confers / confers-macros / hot-reload / encryption）
- XChaCha20-Poly1305 加密配置存储（HKDF 密钥派生）
- 热重载订阅 API（subscribe / reload_config）

### 变更

- `Kit` 采用 typestate 模式（`Kit<Unbuilt>` → `Kit<Ready>`）
- 能力检索改为按模块类型（TypeId），移除字符串键查找

## [0.2.2]

### 新增

- `ModuleMeta` + `AutoBuilder` 标准模块接口
- `Kit` 能力与配置管理中心
- `TypeMap` 类型安全存储（以 `TypeId` 为键）
- 依赖图验证：环检测 + 拓扑排序构建

[Unreleased]: https://github.com/Kirky-X/trait-kit/compare/v0.5.0-rc.6...HEAD
[0.5.0-rc.6]: https://github.com/Kirky-X/trait-kit/compare/v0.5.0-rc.5...v0.5.0-rc.6
[0.5.0-rc.5]: https://github.com/Kirky-X/trait-kit/compare/v0.5.0-rc.4...v0.5.0-rc.5
[0.5.0-rc.4]: https://github.com/Kirky-X/trait-kit/compare/v0.5.0-rc.2...v0.5.0-rc.4
[0.5.0-rc.2]: https://github.com/Kirky-X/trait-kit/compare/v0.5.0-rc.1...v0.5.0-rc.2
[0.4.2]: https://github.com/Kirky-X/trait-kit/compare/v0.4.1...v0.4.2
[0.4.1]: https://github.com/Kirky-X/trait-kit/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/Kirky-X/trait-kit/compare/v0.3.1...v0.4.0
[0.3.1]: https://github.com/Kirky-X/trait-kit/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Kirky-X/trait-kit/compare/v0.2.5...v0.3.0
[0.2.5]: https://github.com/Kirky-X/trait-kit/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/Kirky-X/trait-kit/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/Kirky-X/trait-kit/releases/tag/v0.2.3
[0.2.2]: https://github.com/Kirky-X/trait-kit/releases/tag/v0.2.2
