# Tasks — trait-kit-async-integration

## Phase 1: trait-kit 0.2.2 AsyncKit 核心实现

- [x] [T001] [P0] 在 trait-kit/Cargo.toml 新增 `async = []` feature flag（无额外依赖，Rust 原生 async）
- [x] [T002] [P0] Red: 在 trait-kit/src/kit/async_typemap.rs 编写失败测试，验证 AsyncTypeMap 的 insert/get_cloned/contains/contains_by_type_id 行为
- [x] [T003] [P0] Green: 实现 AsyncTypeMap（`Arc<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>>`），通过 T002 测试，commit "feat(kit): add AsyncTypeMap for Send+Sync type-keyed storage"
- [x] [T004] [P0] Red: 在 trait-kit/src/core/meta.rs 编写失败测试，定义 mock module 实现 AsyncAutoBuilder，验证 build() 返回 Pin<Box<dyn Future + Send>>
- [x] [T005] [P0] Green: 在 trait-kit/src/core/meta.rs 新增 AsyncAutoBuilder trait（`type Capability: Clone + Send + Sync + 'static`、`type Error: std::error::Error + Send + 'static`、`fn build<'a>(kit: &'a AsyncKit) -> Pin<Box<dyn Future<Output = Result<Self::Capability, Self::Error>> + Send + 'a>>`），通过 T004，commit "feat(core): add AsyncAutoBuilder trait"
- [x] [T006] [P0] Red: 在 trait-kit/src/kit/async_kit.rs 编写失败测试，验证 AsyncKit::new/register/set_config 基本行为
- [x] [T007] [P0] Green: 实现 AsyncKit<Unbuilt> 结构（builders: `Arc<RwLock<HashMap<TypeId, AsyncBuildFn>>>`、graph: DependencyGraph、configs: AsyncTypeMap、capabilities: AsyncTypeMap）和 new/register/set_config 方法，通过 T006，commit "feat(kit): add AsyncKit<Unbuilt> basic API"
- [x] [T008] [P0] Red: 在 trait-kit/src/kit/async_kit.rs 编写失败测试，验证 AsyncKit::build() 拓扑排序 + 异步模块构造（mock 两个无依赖模块）
- [x] [T009] [P0] Green: 实现 AsyncKit::build() async 方法（graph.validate() 拓扑排序 → 按序调用 AsyncBuildFn → 存入 AsyncTypeMap → 返回 AsyncKit<Ready>），通过 T008，commit "feat(kit): implement AsyncKit::build() with topological async construction"
- [x] [T010] [P0] Red: 在 trait-kit/src/kit/async_kit.rs 编写失败测试，验证 AsyncKit<Ready>::require/optional/contains/contains_config 行为
- [x] [T011] [P0] Green: 实现 AsyncKit<Ready> 的 require/optional/contains/contains_config 方法，通过 T010，commit "feat(kit): implement AsyncKit<Ready> retrieval API"
- [x] [T012] [P0] Red: 在 trait-kit/src/kit/async_kit.rs 编写失败测试，验证跨模块依赖注入（StorageModule 依赖 LoggerModule，build 时 StorageModule 能通过 kit.require::<LoggerModule>() 获取能力）
- [x] [T013] [P0] Green: 修正 AsyncKit::build() 在构造每个模块时传入 &AsyncKit 引用，使 AsyncBuildFn 可调用 kit.require::<DepModule>()，通过 T012，commit "feat(kit): support cross-module dependency injection in AsyncKit::build()"
- [x] [T014] [P0] 更新 trait-kit/src/lib.rs 添加 `#[cfg(feature="async")] pub use kit::{AsyncKit, Ready as AsyncReady, Unbuilt as AsyncUnbuilt}` 和 `#[cfg(feature="async")] pub use core::meta::AsyncAutoBuilder`；更新 trait-kit/src/prelude.rs 添加对应 re-export
- [x] [T015] [P0] 将 trait-kit/Cargo.toml 版本从 0.2.1 升至 0.2.2，commit "chore: bump trait-kit to 0.2.2"
- [x] [T016] [P1] 在 trait-kit/README.md 新增 AsyncKit 章节，包含 feature flag 启用方式 + 镜像 design.md 决策5 的应用使用示例代码

## Phase 2: oxcache OxcacheModule（底层独立）

- [x] [T017] [P1] 更新 oxcache/Cargo.toml：新增 `trait-kit = { version = "0.2.2", features = ["async"], optional = true }` 和 `kit = ["dep:trait-kit"]` feature
- [x] [T018] [P1] Red: 在 oxcache/src/integrations/kit/module.rs 编写失败测试，验证 OxcacheModule::build() 返回 `Arc<dyn UnifiedCache + Send + Sync>`
- [x] [T019] [P1] Green: 实现 OxcacheModule（ModuleMeta NAME="oxcache" dependencies=&[]、AsyncAutoBuilder 从 kit.config::<OxcacheConfig>() 读取配置并 async 构造 Cache），创建 oxcache/src/integrations/kit/mod.rs 和 oxcache/src/integrations/mod.rs，通过 T018，commit "feat(oxcache): add OxcacheModule for trait-kit 0.2.2 async integration"

## Phase 3: limiteron LimiteronModule（底层独立）

- [x] [T020] [P1] 更新 limiteron/Cargo.toml：新增 `trait-kit = { version = "0.2.2", features = ["async"], optional = true }` 和 `kit = ["dep:trait-kit"]` feature
- [x] [T021] [P1] Red: 在 limiteron/src/integrations/kit/module.rs 编写失败测试，验证 LimiteronModule::build() 返回 `Arc<dyn Limiter + Send + Sync>`
- [x] [T022] [P1] Green: 实现 LimiteronModule（ModuleMeta NAME="limiteron" dependencies=&[]、AsyncAutoBuilder 从 kit.config::<LimiteronConfig>() 读取配置并 async 构造 Governor），创建 limiteron/src/integrations/kit/mod.rs 和 limiteron/src/integrations/mod.rs，通过 T021，commit "feat(limiteron): add LimiteronModule for trait-kit 0.2.2 async integration"

## Phase 4: dbnexus DbNexusModule（依赖 oxcache）

- [x] [T023] [P0] 删除 dbnexus/src/kit/ 整个目录（mod.rs、keys.rs），从 dbnexus/Cargo.toml 移除 `trait-kit = "0.1"`
- [x] [T024] [P0] 更新 dbnexus/Cargo.toml：新增 `trait-kit = { version = "0.2.2", features = ["async"] }`、`oxcache = { version = "0.x", optional = true }`、`oxcache-integration = ["dep:oxcache"]`、`kit = ["dep:trait-kit", "oxcache-integration"]` feature
- [x] [T025] [P0] Red: 在 dbnexus/src/domain/cache_provider.rs 编写失败测试，验证 DbCacheProvider trait 的 get/set/delete 方法返回 `Pin<Box<dyn Future + Send>>`
- [x] [T026] [P0] Green: 在 dbnexus/src/domain/cache_provider.rs 定义 DbCacheProvider trait（`fn get<'a>(&'a self, key: &'a str) -> Pin<Box<dyn Future<Output = Result<Option<Vec<u8>>, DbError>> + Send + 'a>>` + set/delete），通过 T025，commit "feat(dbnexus): define DbCacheProvider trait for cache abstraction"
- [x] [T027] [P0] Red: 在 dbnexus/src/integrations/oxcache_adapter.rs 编写失败测试，验证 OxcacheDbCacheAdapter 实现 DbCacheProvider 并正确代理 oxcache::Cache
- [x] [T028] [P0] Green: 在 dbnexus/src/integrations/oxcache_adapter.rs 实现 OxcacheDbCacheAdapter（包装 `Arc<oxcache::Cache>`，impl DbCacheProvider 将调用代理到 oxcache 并映射错误类型），通过 T027，commit "feat(dbnexus): add OxcacheDbCacheAdapter"
- [x] [T029] [P0] Red: 在 dbnexus/src/integrations/kit/module.rs 编写失败测试，验证 DbNexusModule::build() 通过 kit.require::<OxcacheModule>() 获取缓存能力并包装为 OxcacheDbCacheAdapter 注入 DbPool
- [x] [T030] [P0] Green: 实现 DbNexusModule（ModuleMeta NAME="dbnexus" dependencies=[("oxcache", TypeId::of::<OxcacheModule>())]、AsyncAutoBuilder 在 build 中 require::<OxcacheModule>() + 包装 adapter + 构造 DbPool），创建 dbnexus/src/integrations/kit/mod.rs 和 dbnexus/src/integrations/mod.rs，通过 T029，commit "feat(dbnexus): add DbNexusModule with oxcache dependency injection"

## Phase 5: sdforge SdforgeModule（依赖 limiteron）

- [x] [T031] [P1] 更新 sdforge/Cargo.toml：升级 trait-kit 至 0.2.2 + async feature，新增 `limiteron = { version = "0.x", optional = true }`、`limiteron-integration = ["dep:limiteron"]`、`kit = ["dep:trait-kit", "limiteron-integration"]` feature
- [x] [T032] [P1] Red: 在 sdforge/src/domain/rate_limiter.rs 编写失败测试，验证 ForgeRateLimiter trait 的 check/record 方法返回 `Pin<Box<dyn Future + Send>>`
- [x] [T033] [P1] Green: 在 sdforge/src/domain/rate_limiter.rs 定义 ForgeRateLimiter trait（check/record async 方法），通过 T032，commit "feat(sdforge): define ForgeRateLimiter trait"
- [x] [T034] [P1] Red: 在 sdforge/src/integrations/limiteron_adapter.rs 编写失败测试，验证 LimiteronForgeAdapter 实现 ForgeRateLimiter 并正确代理 limiteron::Governor
- [x] [T035] [P1] Green: 在 sdforge/src/integrations/limiteron_adapter.rs 实现 LimiteronForgeAdapter（包装 `Arc<limiteron::Governor>`，impl ForgeRateLimiter 代理调用），通过 T034，commit "feat(sdforge): add LimiteronForgeAdapter"
- [x] [T036] [P1] Red: 在 sdforge/src/integrations/kit/module.rs 编写失败测试，验证 SdforgeModule::build() 通过 kit.require::<LimiteronModule>() 获取限流器并包装为 LimiteronForgeAdapter 注入 ForgeApp
- [x] [T037] [P1] Green: 实现 SdforgeModule（ModuleMeta NAME="sdforge" dependencies=[("limiteron", TypeId::of::<LimiteronModule>())]、AsyncAutoBuilder 在 build 中 require::<LimiteronModule>() + 包装 adapter + 构造 ForgeApp），创建 sdforge/src/integrations/kit/mod.rs 和 sdforge/src/integrations/mod.rs，通过 T036，commit "feat(sdforge): add SdforgeModule with limiteron dependency injection"

## Phase 6: inklog InklogModule（依赖 dbnexus）

- [x] [T038] [P0] 删除 inklog/src/integrations/kit/ 整个目录（module.rs、keys.rs、mod.rs），从 inklog/Cargo.toml 移除 `trait-kit = "0.1.0"`
- [x] [T039] [P0] 更新 inklog/Cargo.toml：新增 `trait-kit = { version = "0.2.2", features = ["async"] }`，保留 `dbnexus = { version = "0.x", optional = true }`，新增 `kit = ["dep:trait-kit", "dep:dbnexus"]` feature
- [x] [T040] [P0] Red: 在 inklog/src/domain/db_provider.rs 编写失败测试，验证 LogDbProvider trait 的 execute_log/batch_insert 方法返回 `Pin<Box<dyn Future + Send>>`
- [x] [T041] [P0] Green: 在 inklog/src/domain/db_provider.rs 定义 LogDbProvider trait（execute_log/batch_insert async 方法），通过 T040，commit "feat(inklog): define LogDbProvider trait"
- [x] [T042] [P0] Red: 在 inklog/src/integrations/dbnexus_adapter.rs 编写失败测试，验证 DbNexusLogDbAdapter 实现 LogDbProvider 并正确代理 dbnexus DatabaseSession
- [x] [T043] [P0] Green: 在 inklog/src/integrations/dbnexus_adapter.rs 实现 DbNexusLogDbAdapter（包装 `Arc<dyn dbnexus::ConnectionPool>`，impl LogDbProvider 代理调用），替换旧 src/integrations/infra/database.rs 中 DbNexusAdapter，通过 T042，commit "feat(inklog): add DbNexusLogDbAdapter replacing old DbNexusAdapter"
- [x] [T044] [P0] Red: 在 inklog/src/integrations/kit/module.rs 编写失败测试，验证 InklogModule::build() 通过 kit.require::<DbNexusModule>() 获取数据库能力并包装为 DbNexusLogDbAdapter 注入 LoggerManager
- [x] [T045] [P0] Green: 实现 InklogModule（ModuleMeta NAME="inklog" dependencies=[("dbnexus", TypeId::of::<DbNexusModule>())]、AsyncAutoBuilder 在 build 中 require::<DbNexusModule>() + 包装 adapter + 构造 LoggerManager），创建 inklog/src/integrations/kit/mod.rs 和 inklog/src/integrations/mod.rs，通过 T044，commit "feat(inklog): add InklogModule with dbnexus dependency injection"

## Phase 7: 端到端验证

- [x] [T046] [P1] 在 trait-kit/tests/async_e2e.rs 编写集成测试：注册全部 5 个模块（OxcacheModule、LimiteronModule、DbNexusModule、SdforgeModule、InklogModule），build AsyncKit，验证 require::<EachModule>() 返回正确 Capability 类型
- [ ] [T047] [P1] 在 6 个 crate（trait-kit、oxcache、dbnexus、inklog、limiteron、sdforge）分别运行 `cargo test --all-features`，验证全部测试通过，commit "test: verify end-to-end async integration across all 6 crates"
