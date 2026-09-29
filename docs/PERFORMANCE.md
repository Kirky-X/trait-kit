# ⚡ Trait-Kit 性能基准

> 兑现 README "运行时零开销" 主张：以 criterion 基准建立可复现的性能基线。
> 本文件记录基线数字与测量方法；CI 阈值门禁**暂不启用**（待多机数据稳定后引入，回归阈值建议 ±20%）。

## 🏃 运行方式

```sh
# 全部基准（toggle 基准需要 toggle feature；config/write_merge_config 需追加 confers；
# 注意不要用 --all-features 跑基准——见下方 merge_config 行的测量说明）
cargo bench --features toggle,confers

# 仅跑某个基准
cargo bench --features toggle -- build/three_module_chain
```

基准源码：`benches/kit_bench.rs`（单 target，`required-features = ["toggle"]`，
默认 `cargo bench`（无 feature）会跳过该 target 并给出提示）。

基准覆盖四个热路径轴：

| 轴 | 基准 | 说明 |
| --- | --- | --- |
| build | `build/three_module_chain` | 3 模块链（leaf→mid→top）注册 + `build()`（含图校验/拓扑排序/逐模块构建） |
| require | `require/arc_capability_top` | `require::<M>()` 取 `Arc` 能力（期望 ≈ 一次 `Arc::clone`） |
| config | `config/read_clone` | `config::<C>()` 读（含 `Clone` 拷贝，5 元素 Vec 的中型结构） |
| config | `config/write_set_config` | `set_config` 覆写（`TypeMap` 换值） |
| config | `config/write_merge_config` | `merge_config` 读-改-写（`confers` feature；Ready Kit 可用）。**仅在无 `report` 的组合下测量**：`report` 下该基准被编译剔除——报告记录随调用无上限累积（见 `take_config_overrides`），迭代式基准会在单次运行内累积 GB 级 `Vec` 并退化为 realloc/cache 测量（迭代数即机器速度的函数，数字不可比，2–8GB 内存 runner 有 OOM 风险） |
| toggle | `toggle/set` | `enable_toggle`（Ready Kit，HashMap 后端） |
| toggle | `toggle/get` | `is_toggle_enabled`（未命中 → 命中路径均为此量级） |

`report` 特性的记录开销（每次 `merge_config` 一次 `Vec` push、构建路径每模块一次累加）为诊断用途、量级远低于上述热路径，不设独立基准轴，属**非目标**；`config/write_merge_config` 在无 `report` 的组合下测量即其纯逻辑成本。探针路径（`probe` feature）无独立基准轴：`run_probes` 的框架测量开销为每次探针一对 `Instant::now()`（纳秒级）+ 超时变体一次定时器 arm，相对网络探针本身的毫秒级延迟属非目标。

## 📊 基线（2026-09-10）

- 机器：AMD Ryzen 9 9950X（16C/32T），WSL2 kernel 6.6.87
- 工具链：rustc 1.97.1，`bench` profile（继承 `release`：`opt-level=3`、`lto=fat`、`codegen-units=1`）
- criterion 配置：`sample_size=20`，`warm_up_time=500ms`，`measurement_time=1s`
- 报告口径：median（点估计），单位 ns/iter

| 基准 | median |
| --- | --- |
| `build/three_module_chain` | ~706 ns |
| `require/arc_capability_top` | ~15 ns |
| `config/read_clone` | ~39 ns |
| `config/write_set_config` | ~34 ns |
| `toggle/set` | ~21 ns |
| `toggle/get` | ~12 ns |

### 结论

- **require ≈ 15 ns**：与裸 `Arc::clone`（≈1–2 ns）+ `TypeId` HashMap 查找 + downcast 同量级，
  "零开销能力检索"成立；非 `Arc` 能力经 `require` 会整体 `Clone`，大结构请改用
  `require_ref`（借用读，注意与 `RefCell` 写路径互斥）。
- **config 读 ≈ 39 ns**：包含中型结构（String + Vec）的真实拷贝成本；纯开销部分（查找 + downcast）
  与 require 同量级。
- **build ≈ 706 ns / 3 模块**：图校验（缺依赖 + 环检测）+ Kahn 排序 + 3 次构建回调，
  启动期一次性成本，量级符合预期。

## 🔁 复现与对比

1. 固定机器插电、关闭省电模式；
2. `cargo bench --features toggle -- --save-baseline <name>` 保存基线；
3. 改动后 `cargo bench --features toggle -- --baseline <name>` 对比；
   criterion 会在输出中标注回归/改进（noise_threshold=5%）。

## 🚦 CI 阈值（待启用）

- 建议：任一基准 median 回归 > 20% 时失败（连续两次运行确认，排除噪声）。
- 启用前置：至少两台不同机器各留存 3 次基线，确认方差 < 10%。
- 当前阶段仅记录基线，阈值门禁待多机数据稳定后引入。
