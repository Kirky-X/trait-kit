# confers 库侧需求登记（selfdev-lib-requests）

> trait-kit 依赖 confers 0.6.0-rc.6。本文登记 trait-kit 自研设施迁移 confers
> 过程中暴露的库侧缺口：每项列出 confers 侧现状证据、trait-kit 侧被阻塞的迁移面
> 与解锁后的收益。登记来源：变更 `selfdev-lib-adoption-r1`（specmark）的
> design.md 证据表（2026-10-10 快照，施工以符号为准）。

## 需求总览

| # | 需求 | confers 侧现状 | trait-kit 侧阻塞的迁移面 | 优先级 |
|---|------|---------------|--------------------------|--------|
| 1 | `LocalDynamicField`（非 Send/Sync 变体） | `DynamicField<T>` 强制 `T: Clone + Send + Sync + 'static`，回调 `Fn(&T) + Send + Sync` | 同步 `Kit` 订阅侧（`Rc<dyn Fn()>` 簿记） | 中 |
| 2 | `dynamic` 无 tokio 轻量特性 | `dynamic = ["dep:arc-swap", "dep:tokio", "dep:dashmap"]`，tokio 仅被 `#[cfg(feature = "watch")]` 的 FieldWatcher 使用 | `reload` 特性的依赖体积 | 中 |
| 3 | TypeId 异质注册表 | confers 无 TypeId keyed 的异质字段注册设施 | AsyncKit/Kit 每配置类型一槽的注册簿记仍需使用侧自管 | 低 |
| 4 | `SecretBytes: From<Vec<u8>>` | 仅 `new(Vec<u8>)`；孤儿规则下 trait-kit 无法代为实现 | `KeyBytes::from(vec)` → `KeyBytes::new(vec)` 构造点（已迁移接受） | 低 |

---

## 1. LocalDynamicField（非 Send/Sync 变体）

**confers 侧现状（证据）**

- `confers/src/impl_/dynamic.rs`：`pub struct DynamicField<T: Clone + Send + Sync + 'static>`
  （约 :59）；`on_change(&self, f: impl Fn(&T) + Send + Sync + 'static) -> CallbackGuard`
  （约 :129-136）；回调存储 `Arc<DashMap<CallbackId, Arc<dyn Fn(&T) + Send + Sync>>>`。
- Send+Sync 界是 `DashMap`（多分片、线程安全）与 `Arc` 容器选择的直接后果，库侧不存在
  面向单线程场景的 `Rc`/`RefCell` 变体。

**trait-kit 侧阻塞的迁移面**

- `src/kit/kit.rs:150`：`type SubscriberMap = RefCell<HashMap<TypeId, Vec<Rc<dyn Fn()>>>>`。
  同步 `Kit` 整个面向单线程（`Rc` 回调、`RefCell` 内部可变性），`Rc<dyn Fn()>` 是
  `!Send + !Sync`，类型系统上无法进入 `DynamicField` 的界。
- `Kit::subscribe` / `Kit::reload_config`（`src/kit/kit.rs:2102-2166`）及其文档化行为
  （RLD-06 panic 穿透，`tests/e2e/e2e_config.rs`）整体被阻塞。

**解锁后的迁移面与收益**

- 同步 `Kit` 订阅簿记整体迁至 confers，删除 trait-kit 侧自维护的 SubscriberMap
  （别名 + subscribe + reload_config 含文档约 63 行），并获得 confers 的版本序、
  锁外快照派发与（如启用）panic 隔离能力。
- 政策面不冲突：同步侧当前刻意不拉 `confers/watch`（Cargo.toml 注释），本项不引入
  notify-debouncer-full。

## 2. dynamic 无 tokio 轻量特性

**confers 侧现状（证据）**

- `confers/Cargo.toml:162`：`dynamic = ["dep:arc-swap", "dep:tokio", "dep:dashmap"]`。
- `confers/src/impl_/dynamic.rs`：tokio 仅出现在 `#[cfg(feature = "watch")]` 门控的
  FieldWatcher（约 :317-322）；核心 `DynamicField`（`on_change` / `update` / 派发 /
  versioning）不依赖 tokio。

**trait-kit 侧阻塞的迁移面**

- trait-kit `reload` 特性为使用 confers `DynamicField` 必须拉 `confers/dynamic`，
- 连带把 `arc-swap`、`dashmap`、`tokio` 引入 reload 使用方的依赖树（本变更接受该
  体积：AsyncKit 本就是 Send+Sync 场景）。

**解锁后的迁移面与收益**

- 若 confers 拆出 `dynamic`（arc-swap + dashmap）与 `dynamic-watch`（+tokio）两档，
  trait-kit `reload` 特性只依赖前者，tokio 不再进入纯 reload 使用方的依赖树。

## 3. TypeId 异质注册表

**confers 侧现状（证据）**

- confers 无 TypeId keyed 的异质字段注册设施：`DynamicField<T>` 按具体类型 `T`
  单列，没有"一类一槽"的注册表抽象。

**trait-kit 侧阻塞的迁移面**

- AsyncKit 侧每配置类型一 `DynamicField<u64>` epoch 槽（`src/kit/async_kit.rs`
  `FieldEntry`）仍需使用侧自管 `HashMap<TypeId, FieldEntry>`；同步 Kit 若迁移
  （依赖需求 1）同样需要该簿记层。

**解锁后的迁移面与收益**

- 若 confers 提供 `TypeId → DynamicField` 注册表（含守卫生命周期托管），使用侧删除
  自管 HashMap 与 per-type get-or-create 代码。

## 4. SecretBytes 的 From 实现

**confers 侧现状（证据）**

- `confers/src/secret/bytes.rs`：构造仅 `pub fn new(bytes: Vec<u8>) -> Self`
  （约 :11-13），无 `impl From<Vec<u8>> for SecretBytes`。
- `SecretBytes` 显式不实现 `Clone`（防旁路 Drop 零化）。

**trait-kit 侧阻塞的迁移面**

- trait-kit 为使用方，孤儿规则禁止 trait-kit 侧补 `impl From<Vec<u8>> for SecretBytes`。
- 原自研 `KeyBytes` 的 `KeyBytes::from(vec)` 构造点（`src/kit/config.rs` 的
  `ConfersKeyProvider` 适配层）已迁移为 `KeyBytes::new(vec)`；0.5.0-rc 阶段接受该
  破坏性构造 API 变更，`KeyBytes` 名称作为别名长期存续。

**解锁后的迁移面与收益**

- confers 侧补 `From<Vec<u8>>`（可与 `From<&[u8]>` 一并提供）后，`KeyBytes` 别名可直接
- 评估改用 `confers::SecretBytes` 正名，删除 trait-kit 侧别名层。

---

## 状态

- 本文档为登记记录，不承诺 confers 侧排期；confer 侧落地对应需求后，trait-kit 以新
  变更跟进接入。
- 需求 2、4 的 trait-kit 侧工作已由变更 `selfdev-lib-adoption-r1` 先行落地（接受
  构造 API `new()` 化与 reload 传递依赖体积），本文档保留诉求供 confers 侧裁量。
