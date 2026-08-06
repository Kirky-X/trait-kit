# 架构文档

trait-kit 的架构设计围绕一个核心目标：**在应用启动时，以类型安全、可验证的方式装配模块依赖**。

## 整体架构

```mermaid
graph TB
    subgraph core["core — 核心接口层"]
        MM[ModuleMeta<br/>名称 + 依赖声明]
        AB[AutoBuilder<br/>同步构建]
        AAB[AsyncAutoBuilder<br/>异步构建]
        IB[InterfaceBuilder<br/>接口分离]
        LC[Lifecycle<br/>on_ready + on_shutdown]
        HC[HealthCheck<br/>HealthStatus 报告]
        OBS[BuildObserver<br/>构建回调]
    end

    subgraph kit["kit — 能力管理中心"]
        K[Kit&lt;Unbuilt&gt; → Kit&lt;Ready&gt;]
        DG[DependencyGraph<br/>环检测 + 拓扑排序]
        TM[TypeMap<br/>TypeId 键值存储]
        CFG[Config<br/>confers 集成]
        SC[Scope / AsyncScope<br/>作用域隔离]
    end

    subgraph async_kit["async_kit — 异步能力管理"]
        AK[AsyncKit&lt;Unbuilt&gt; → AsyncKit&lt;Ready&gt;]
        ATM[AsyncTypeMap<br/>Arc&lt;RwLock&gt; 存储]
    end

    subgraph i18n_mod["i18n — 国际化"]
        I18N[I18nManager + tr<br/>Fluent FTL 翻译]
        FMT[I18nFormatter<br/>ICU4X 格式化]
    end

    MM --> K
    AB --> K
    AAB --> AK
    IB --> K
    LC --> K
    HC --> K
    OBS --> K
    K --> DG
    K --> TM
    K --> CFG
    K --> SC
    AK --> ATM
```

## 核心设计模式

### Typestate 模式

Kit 使用 typestate 模式确保构建时验证：

```
Kit<Unbuilt>                    Kit<Ready>
┌─────────────────┐   build()   ┌─────────────────┐
│ register()      │ ──────────→ │ require()       │
│ register_lazy() │             │ require_ref()   │
│ register_multi()│             │ optional()      │
│ register_as()   │             │ require_all()   │
│ set_config()    │             │ resolve()       │
│ build()         │             │ contains()      │
│ shutdown()      │             │ health_check()  │
└─────────────────┘             │ shutdown()      │
                                └─────────────────┘
```

- `Kit<Unbuilt>`：注册模块、配置、生命周期钩子。
- `kit.build()`：验证依赖图 → 拓扑排序 → 按序构建 → 返回 `Kit<Ready>`。
- `Kit<Ready>`：只读检索能力，不可再注册。

### 内部可变性

- **同步 Kit**：基于 `RefCell`，单线程 `!Sync` 设计，避免锁开销。
- **AsyncKit**：基于 `Arc<RwLock>`，多线程 `Send + Sync` 设计。

### 依赖图验证

`DependencyGraph` 在 `build()` 时执行两阶段验证：

1. **缺失依赖检测**：确保所有声明的依赖已注册。
2. **环检测 + 拓扑排序**：使用 Kahn 算法，发现环则返回错误。

构建按拓扑序执行，确保依赖先于消费者构建。

## 模块系统

### 能力注册模式

| 模式 | 方法 | 说明 |
|---|---|---|
| 即时构建 | `register::<M>()` | `build()` 时按拓扑序构建 |
| 延迟构建 | `register_lazy::<M>()` | 首次 `require()` 时触发构建并缓存 |
| 多绑定 | `register_multi::<M>()` | 同类型聚合为 Vec，`require_all()` 检索 |
| 接口分离 | `register_as::<M>()` | `dyn Trait` 类型擦除注册 |
| 条件注册 | `register_if::<M>(pred)` | 运行时谓词控制 |
| 覆盖注入 | `override_module::<M>(cap)` | 测试注入，跳过 build_fn |

### Feature 分层

```mermaid
graph LR
    subgraph 核心["核心（无 feature）"]
        C1[ModuleMeta + AutoBuilder]
        C2[Kit typestate]
        C3[DependencyGraph]
        C4[I18nManager + tr]
    end

    subgraph 可选["可选 Feature"]
        F1[async]
        F2[confers → confers-macros → hot-reload → encryption]
        F3[interface]
        F4[lifecycle]
        F5[health]
        F6[scope]
        F7[conditional]
        F8[observability]
        F9[factory]
        F10[decorator]
        F11[shutdown]
    end
```

## 数据流

### 构建流程

```mermaid
sequenceDiagram
    participant User as 用户代码
    participant Kit as Kit<Unbuilt>
    participant Graph as DependencyGraph
    participant TypeMap as TypeMap

    User->>Kit: register::<M>()
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
    Kit-->>User: Kit<Ready>
```

### 能力检索流程

```mermaid
sequenceDiagram
    participant User as 用户代码
    participant Kit as Kit<Ready>
    participant TypeMap as TypeMap

    User->>Kit: require::<M>()
    Kit->>TypeMap: get_cloned_by_type_id(TypeId)

    alt 即时构建模块
        TypeMap-->>Kit: Box<dyn Any>
        Kit-->>User: M::Capability (clone)
    else 延迟构建模块
        Kit->>Kit: 检查 OnceLock
        alt 首次访问
            Kit->>Kit: 调用 BuildFn
            Kit->>TypeMap: 缓存结果
        end
        Kit-->>User: M::Capability (clone)
    end
```

## 目录结构

```
src/
├── lib.rs              # crate 入口，re-export
├── error.rs            # TraitKitError 错误类型（i18n 本地化）
├── prelude.rs          # 常用类型再导出
├── core/
│   ├── mod.rs          # 模块声明 + re-export
│   ├── meta.rs         # ModuleMeta / AutoBuilder / AsyncAutoBuilder / InterfaceBuilder
│   ├── macros.rs       # impl_module_meta! / impl_auto_builder! / impl_async_auto_builder!
│   ├── health.rs       # HealthCheck / AsyncHealthCheck / HealthStatus
│   ├── lifecycle.rs    # Lifecycle / AsyncLifecycle
│   └── observer.rs     # BuildObserver
├── kit/
│   ├── mod.rs          # Kit 模块声明 + re-export
│   ├── kit.rs          # Kit<Unbuilt> → Kit<Ready> typestate 实现
│   ├── graph.rs        # DependencyGraph：环检测 + 拓扑排序
│   ├── typemap.rs      # TypeMap：TypeId 键值存储
│   ├── scope.rs        # Scope / AsyncScope
│   ├── shutdown.rs     # ShutdownCoordinator / AsyncShutdownCoordinator
│   ├── async_kit.rs    # AsyncKit
│   ├── async_typemap.rs # AsyncTypeMap
│   └── config.rs       # confers 集成
└── i18n/
    ├── mod.rs          # I18nManager + I18nFormatter + tr()
    ├── i18n_impl.rs    # 实现细节
    └── messages/
        ├── mod.rs      # FTL 嵌入
        ├── en.ftl      # 英文消息
        └── zh.ftl      # 中文消息
```

## 线程安全模型

| 类型 | Send | Sync | 说明 |
|---|---|---|---|
| `Kit<S>` | ✗ | ✗ | `RefCell` 内部可变性 |
| `AsyncKit<S>` | ✓ | ✓ | `Arc<RwLock>` |
| `Scope` | ✗ | ✗ | `RefCell` |
| `AsyncScope` | ✓ | ✓ | `Arc<RwLock>` |
| `TypeMap` | ✗ | ✗ | `RefCell<HashMap>` |
| `AsyncTypeMap` | ✓ | ✓ | `Arc<RwLock<HashMap>>` |
| `BuildObserver` | ✓ | ✓ | trait bound: `Send + Sync` |
| `ShutdownCoordinator` | ✗ | ✗ | `RefCell` |
| `AsyncShutdownCoordinator` | ✓ | ✓ | `Arc<RwLock>` |

## 错误处理

`TraitKitError` 统一所有 Kit 操作错误，`Display` 通过 `tr()` 自动本地化：

| 变体 | 触发场景 |
|---|---|
| `CycleDetected` | 依赖图中检测到环 |
| `DependencyMissing` | 依赖的模块未注册 |
| `AlreadyRegistered` | 模块重复注册 |
| `BuildFailed` | 模块构建失败 |
| `MissingCapability` | 能力不存在 |
| `MissingConfig` | 配置不存在 |
| `LifecycleFailed` | 生命周期钩子失败（需 `lifecycle` feature） |
| `ShutdownTimedOut` | 优雅关闭超时（需 `shutdown` feature） |
