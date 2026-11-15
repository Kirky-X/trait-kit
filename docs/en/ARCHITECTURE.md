# 🏗️ Trait-Kit Architecture

trait-kit's architecture is designed around one core goal: **assembling module dependencies at application startup in a type-safe, verifiable way**.

## 📋 Table of Contents

<details open>
<summary>📑 Table of Contents</summary>

- [🗺️ Overall Architecture](#️-overall-architecture)
- [🧩 Core Design Patterns](#-core-design-patterns)
- [📦 Module System](#-module-system)
- [🔗 Self-Managed Library (confers) Gap Registry](#self-managed-library-confers-gap-registry)
- [🔄 Data Flow](#-data-flow)
- [📁 Directory Layout](#-directory-layout)
- [🧵 Thread-Safety Model](#-thread-safety-model)
- [🚪 Error Handling](#-error-handling)

</details>

---

## 🗺️ Overall Architecture

The trait-kit workspace consists of three members (the main crate, the proc-macro crate, and the examples crate). The main crate's `src/` is organized into three layers: the `core` interface layer, the `kit` capability management center, and `i18n` internationalization.

```mermaid
flowchart TD
    subgraph ws["trait-kit workspace"]
        TK["trait-kit 主 crate<br/>core / kit / i18n"]
        MAC["trait-kit-macros<br/>derive Module / ConfigInherit / SharedConfig"]
        EX["trait-kit-examples<br/>20 个可运行示例"]
    end

    subgraph tk["trait-kit src 模块"]
        CORE["core 接口层<br/>ModuleMeta / AutoBuilder / Lifecycle / HealthCheck / BuildObserver"]
        KIT["kit 管理中心<br/>Kit / DependencyGraph / TypeMap / Scope / Shutdown / AsyncKit"]
        I18N["i18n 国际化<br/>tr + I18nFormatter"]
    end

    TK --> CORE
    TK --> KIT
    TK --> I18N
    MAC -->|"dev-dependency（测试）"| TK
    EX --> TK
    EX --> MAC
```

Relationships between components inside the `kit` management center:

```mermaid
flowchart TD
    K["Kit typestate<br/>Unbuilt → Ready"]
    DG["DependencyGraph<br/>环检测 + 拓扑排序"]
    TM["TypeMap<br/>TypeId 键值存储"]
    CFG["config<br/>confers 集成"]
    SC["scope / shutdown / toggle<br/>运行时扩展"]
    AK["AsyncKit<br/>并发构建"]
    ATM["AsyncTypeMap<br/>Arc RwLock 存储"]
    EV["events / ports<br/>事件总线与观测端口"]
    RP["report / presets / sub_kit<br/>可选组合面"]

    K --> DG
    K --> TM
    K --> CFG
    K --> SC
    AK --> ATM
    AK --> DG
    K -.-> EV
    K -.-> RP
```

## 🧩 Core Design Patterns

### Typestate Pattern

Kit uses the typestate pattern to ensure build-time validation:

```text
Kit<Unbuilt>                    Kit<Ready>
┌─────────────────┐   build()   ┌─────────────────┐
│ register()      │ ──────────→ │ require()       │
│ register_lazy() │             │ require_ref()   │
│ register_multi()│             │ optional()      │
│ register_as()   │             │ require_all()   │
│ set_config()    │             │ resolve()       │
│ build()         │             │ contains()      │
└─────────────────┘             │ health_check()  │
                                │ shutdown()      │
                                └─────────────────┘
```

- `Kit<Unbuilt>`: register modules, configs, and lifecycle hooks.
- `kit.build()`: validate the dependency graph → topological sort → build in order → return `Kit<Ready>`.
- `Kit<Ready>`: read-only capability retrieval; no further registration.

### Interior Mutability

- **Sync Kit**: based on `RefCell`, a single-threaded `!Sync` design that avoids lock overhead.
- **AsyncKit**: based on `Arc<RwLock>`, a multi-threaded `Send + Sync` design.

### Dependency-Graph Validation

`DependencyGraph` performs two-phase validation at `build()`:

1. **Missing-dependency detection**: ensures all declared dependencies are registered.
2. **Cycle detection + topological sort**: uses the Kahn algorithm; a cycle returns an error.

Builds execute in topological order, ensuring dependencies build before their consumers.

## 📦 Module System

### Capability Registration Modes

| Mode | Method | Description |
|---|---|---|
| Eager build | `register::<M>()` | Built in topological order during `build()` |
| Lazy build | `register_lazy::<M>()` | Built and cached on first `require()` |
| Multi-binding | `register_multi::<M>()` | Same type aggregated into a Vec, retrieved via `require_all()` |
| Interface separation | `register_as::<M>()` | `dyn Trait` type-erased registration |
| Conditional registration | `register_if::<M>(pred)` | Runtime predicate control |
| Override injection | `override_module::<M>(cap)` | Test injection, skipping build_fn |

### Feature Layering

Of the 19 canonical features (plus the 3 deprecated compatibility aliases `interface`/`scope`/`negotiate`), the confers family has an inheritance chain while the rest are zero-dependency standalone features:

```mermaid
graph LR
    C["confers"] --> R["reload"]
    R --> E["encryption"]

    subgraph zc["zero-dependency standalone features"]
        Z["async / di / lifecycle / health<br/>request-scope / toggle / observer / decorator / shutdown<br/>version-negotiation / compose"]
    end

    subgraph dep["features with optional dependencies"]
        D1["i18n<br/>icu_* components / writeable / sys-locale"]
        D2["report<br/>serde / serde_json"]
        PR["probe<br/>futures-timer（→health）"]
        P["presets / presets-remote"]
    end

    P --> C
```

### Four-Layer Config Inheritance

The `confers` feature provides a four-layer config inheritance mechanism supporting seamless config reuse across modules/projects:

```mermaid
graph TB
    subgraph Layer4["Layer 4: populate_defaults"]
        L4["零配置自动填充<br/>ModuleConfig default_value"]
    end

    subgraph Layer3["Layer 3: SharedConfig"]
        L3["serde_json Value overlay<br/>跨类型共享字段继承<br/>derive SharedConfig"]
    end

    subgraph Layer2["Layer 2: ConfigInherit"]
        L2["编译期安全字段覆盖<br/>Option T 仅 Some 时覆盖<br/>derive ConfigInherit"]
    end

    subgraph Layer1["Layer 1: merge_json_deep"]
        L1["递归深合并 JSON Object"]
    end

    Layer4 --> Layer3 --> Layer2 --> Layer1
```

**Data flow**: `TraitKitConfig` → `extract_shared` → `shared_fields overlay` → `inject_shared` → `DbConfig`

### Self-Managed Library (confers) Gap Registry

Some of trait-kit's facilities remain self-managed; migrating them to confers requires confers to close the following gaps first. For the full evidence chain (confers-side source locations, blocked migration surfaces, unlock benefits) see the specmark archive `selfdev-lib-adoption-r1` (2026-10-10 snapshot).

| # | Gap | confers-side status | Blocked migration surface | Priority | Current handling |
|---|---|---|---|---|---|
| 1 | `LocalDynamicField` (non-Send/Sync variant) | `DynamicField<T>` requires `T: Clone + Send + Sync + 'static`, callbacks are `Fn(&T) + Send + Sync` | Sync `Kit` subscriber bookkeeping (`Rc<dyn Fn()>`) | Medium | Keep self-managing `SubscriberMap` |
| 2 | `dynamic` has no lightweight non-tokio mode | `dynamic` pulls in `arc-swap`/`dashmap`/`tokio` | Dependency footprint of the `reload` feature | Medium | Footprint accepted (AsyncKit is a Send+Sync scenario anyway) |
| 3 | TypeId-heterogeneous registry | confers has no TypeId-keyed heterogeneous field registry | Per-config-type slot registration bookkeeping in AsyncKit/Kit | Low | Stays user-side, self-managed |
| 4 | `SecretBytes: From<Vec<u8>>` | Only `new(Vec<u8>)`; orphan rules prevent implementing it for trait-kit | `KeyBytes::from(vec)` construction sites | Low | Migrated to `KeyBytes::new(vec)` |

> This table is the **sole index** of the gap registry; exact line numbers and code citations drift with versions — the specmark archive snapshot prevails and they are not duplicated here.

## 🔄 Data Flow

### Build Flow

```mermaid
sequenceDiagram
    participant User as 用户代码
    participant Kit as Kit Unbuilt
    participant Graph as DependencyGraph
    participant TypeMap as TypeMap

    User->>Kit: register 模块 M
    Kit->>Graph: add(ModuleEntry)
    Kit->>Kit: 存储 BuildFn

    User->>Kit: set_config(value)
    Kit->>TypeMap: insert(config)

    User->>Kit: build()
    Kit->>Graph: validate()
    Graph-->>Kit: topo_sorted_ids

    loop 每个模块（拓扑序）
        Kit->>Kit: 检查 overrides
        Kit->>Kit: 调用 BuildFn
        Kit->>TypeMap: insert(capability)
    end

    Kit->>Kit: 执行 ready_callbacks（lifecycle）
    Kit-->>User: Kit Ready
```

### Capability Retrieval Flow

```mermaid
sequenceDiagram
    participant User as 用户代码
    participant Kit as Kit Ready
    participant TypeMap as TypeMap

    User->>Kit: require 模块 M
    Kit->>TypeMap: get_cloned_by_type_id(TypeId)

    alt 即时构建模块
        TypeMap-->>Kit: Box dyn Any
        Kit-->>User: M::Capability（clone）
    else 延迟构建模块
        Kit->>Kit: 检查 OnceLock
        alt 首次访问
            Kit->>Kit: 调用 BuildFn
            Kit->>TypeMap: 缓存结果
        end
        Kit-->>User: M::Capability（clone）
    end
```

## 📁 Directory Layout

```text
src/
├── lib.rs              # crate 入口，re-export
├── error.rs            # TraitKitError 错误类型（i18n 本地化）
├── redact.rs           # 对外文本脱敏：抹掉凭据片段、绝对路径与内部类型路径（内部模块）
├── prelude.rs          # 常用类型再导出
├── core/
│   ├── mod.rs          # 模块声明 + re-export
│   ├── meta.rs         # ModuleMeta / AutoBuilder / AsyncAutoBuilder / InterfaceBuilder
│   ├── macros.rs       # impl_module_meta! / impl_auto_builder! / impl_async_auto_builder!
│   ├── health.rs       # HealthCheck / AsyncHealthCheck / HealthStatus
│   ├── lifecycle.rs    # Lifecycle / AsyncLifecycle
│   ├── observer.rs     # BuildObserver
│   └── probe.rs        # ServiceProbe / ProbeReport / ProbeEntry（probe feature）
├── kit/
│   ├── mod.rs          # Kit 模块声明 + re-export
│   ├── kit.rs          # Kit<Unbuilt> → Kit<Ready> typestate 实现
│   ├── kit_tests.rs    # kit.rs 内联测试的独立承载文件
│   ├── graph.rs        # DependencyGraph：环检测 + 拓扑排序
│   ├── typemap.rs      # TypeMap：TypeId 键值存储
│   ├── scope.rs        # Scope / AsyncScope
│   ├── shutdown.rs     # ShutdownCoordinator / AsyncShutdownCoordinator
│   ├── soft_build.rs   # soft_build 降级构建助手（失败时落 fallback 能力）
│   ├── toggle.rs       # ToggleBackend / MemoryToggle / 类型化开关句柄
│   ├── async_kit.rs    # AsyncKit（并发构建 + Send + Sync）
│   ├── async_typemap.rs # AsyncTypeMap
│   ├── config.rs       # confers 集成（含加密存储 / KeyProvider）
│   ├── events.rs       # EventBus / KitEvent / MemoryEventBus
│   ├── ports.rs        # MetricsPort / LogPort 观测端口
│   ├── report.rs       # BuildReport 结构化构建报告
│   ├── presets.rs      # ConfersConfigModule 预设模块包
│   └── sub_kit.rs      # 子 Kit 组合（compose feature）
└── i18n/
    ├── mod.rs          # I18nManager + I18nFormatter + tr()
    ├── i18n_impl.rs    # 实现细节
    └── messages/
        └── mod.rs      # FTL 嵌入（include_str! 内嵌仓库根 locales/ 下的域文件）

locales/               # FTL 语言包（仓库根，按域拆分，非 src/ 内）
├── en/
│   ├── errors.ftl     # 英文错误消息
│   ├── logs.ftl       # 英文日志/降级/探针载荷消息
│   └── examples.ftl   # 英文示例演示输出
└── zh/
    ├── errors.ftl     # 中文错误消息
    ├── logs.ftl       # 中文日志/降级/探针载荷消息
    └── examples.ftl   # 中文示例演示输出
```

## 🧵 Thread-Safety Model

| Type | Send | Sync | Notes |
|---|---|---|---|
| `Kit<S>` | ✗ | ✗ | `RefCell` interior mutability |
| `AsyncKit<S>` | ✓ | ✓ | `Arc<RwLock>` |
| `Scope` | ✗ | ✗ | `RefCell` |
| `AsyncScope` | ✓ | ✓ | `Arc<RwLock>` |
| `TypeMap` | ✗ | ✗ | `RefCell<HashMap>` |
| `AsyncTypeMap` | ✓ | ✓ | `Arc<RwLock<HashMap>>` |
| `BuildObserver` | ✓ | ✓ | trait bound: `Send + Sync` |
| `ShutdownCoordinator` | ✗ | ✗ | `RefCell` |
| `AsyncShutdownCoordinator` | ✓ | ✓ | `Arc<RwLock>` |

## 🚪 Error Handling

`TraitKitError` unifies all Kit operation errors; `Display` is automatically localized via `tr()`. For trigger scenarios and handling advice for every variant see the [User Guide · Troubleshooting](USER_GUIDE.md#-troubleshooting); for the enum definition and fields see [API Reference · Error Types](../API_REFERENCE.md#-错误类型).
