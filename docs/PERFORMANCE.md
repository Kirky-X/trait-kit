# ⚡ Trait-Kit 性能基准

> 兑现 README "运行时零开销" 主张：以 criterion 基准建立可复现的性能基线。
> 本文件是性能基线数字、门禁阈值与刷新规则的**单一事实源**。**CI 门禁口径**：ci.yml 的 bench job 以"本地基线
> ×100 后向上取整到整千"的数量级阈值做灾难性回归护栏（仓库变量
> `BENCH_GATE_DISABLED=true` 可整体停用）——共享 runner 噪音大，该门禁**不是**严 格
> 回归检测，±20% 级的精细回归判断只能在同一台机器的本地基线流程做。

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
| config | `config/write_merge_config` | `merge_config` 读-改-写（`confers` feature；Ready Kit 可用）。**仅在无 `report` 的组合下测量**：`report` 下该基准被编译剔除——报告历史虽有界（1024 条环形语义），但满容量后每次调用触发 O(n) 头删 + 丢弃计数（见 `take_config_overrides`），迭代式基准会把簿记成本混入测量并退化为 memmove/cache 测量（迭代数即机器速度的函数，数字不可比） |
| toggle | `toggle/set` | `enable_toggle`（Ready Kit，HashMap 后端） |
| toggle | `toggle/get` | `is_toggle_enabled`（未命中 → 命中路径均为此量级） |

`report` 特性的记录开销（每次 `merge_config` 一次 `Vec` push、构建路径每模块一次累加）为诊断用途、量级远低于上述热路径，不设独立基准轴，属**非目标**；`config/write_merge_config` 在无 `report` 的组合下测量即其纯逻辑成本。探针路径（`probe` feature）无独立基准轴：`run_probes` 的框架测量开销为每次探针一对 `Instant::now()`（纳秒级）+ 超时变体一次定时器 arm，相对网络探针本身的毫秒级延迟属非目标。

## 📊 基线（2026-09-29 安静复测，现行）

- 机器：AMD Ryzen 9 9950X（16C/32T），WSL2 kernel 6.6.87
- 工具链：rustc 1.97.1，`bench` profile（继承 `release`：`opt-level=3`、`lto=fat`、`codegen-units=1`）
- criterion 配置：`sample_size=20`，`warm_up_time=500ms`，`measurement_time=1s`
- 报告口径：median（点估计），单位 ns/iter；feature 集 `toggle,confers`
- 取数位置：`target/criterion/<bench>/new/estimates.json` 的 `median.point_estimate`
- 测量模式：20 samples × 1s 测量、500ms 预热的短模式（`benches/kit_bench.rs::bench_config_short`）；数字为单次运行的实测值，非多次取平均
- HEAD：`c04d9ab`（feature 集 toggle,confers，无 report——`config/write_merge_config`
  在 report 组合下被编译剔除，见文末测量说明）

| 基准 | median |
| --- | --- |
| `build/three_module_chain` | ~625 ns |
| `require/arc_capability_top` | ~16 ns |
| `config/read_clone` | ~45 ns |
| `config/write_set_config` | ~36 ns |
| `config/write_merge_config` | ~39 ns |
| `toggle/set` | ~61 ns |
| `toggle/get` | ~1728 ns |

CI 阈值推导以本表为准（×100 后向上取整到整千，见下方「CI 阈值」）。本表为门禁推导源：刷新时同步修改 `scripts/bench_gate.py` 的 `LIMITS_NS`（两处一致变更）。

### 测量可复现性备注

- `toggle/get` 在本机两次独立全量运行（高负载时段 1778 ns、负载 1.12 复测 1728 ns）稳定在 ~1.7 µs 量级；审查轮曾观察到一次 454 ns 的单点值，在本机不可复现，不作为基线。若未来出现稳定的更低值，按下方刷新规则重测并同步两处。
- 历史（2026-09-10）快照中 toggle/set ≈21 ns、toggle/get ≈12 ns 与当前 HEAD 的实测不可对齐，根因未查明（其测量条件无法复现），仅作历史记录保留在下节，不参与门禁推导。
- `report` 特性的记录/排空路径（`merge_config` 的 `push_config_override` 记录成本与 `take_config_overrides` 的 `mem::take` 轮转换出）**无基准覆盖，属显式非目标**：`config/write_merge_config` 在 report 组合下被编译剔除（迭代式基准下环形驱逐的簿记成本混入测量，见 `benches/kit_bench.rs`），CI bench 亦只跑 `toggle,confers`。该路径为诊断用途、量级远低于热路径，不要因本表无此轴而误以为已测。

## 🗄 历史快照（2026-09-10，不参与门禁推导）

- 同机同 criterion 配置的早期快照：build ~706 / require ~15 / read_clone ~39 /
  write_set_config ~34 / toggle/set ~21 / toggle/get ~12（ns）。
- build/require/config 四行与现行测量同量级吻合；toggle 两行与现行实测
  （61 ns / 1728 ns）相差 3×–144×，根因未查明（当时的测量条件无法复现，
  toggle.rs 自快照以来仅版权头变更），以现行复测为准。

### 结论

- **require ≈ 16 ns**：与裸 `Arc::clone`（≈1–2 ns）+ `TypeId` HashMap 查找 + downcast 同量级，
  "零开销能力检索"成立；非 `Arc` 能力经 `require` 会整体 `Clone`，大结构请改用
  `require_ref`（借用读，注意与 `RefCell` 写路径互斥）。
- **config 读 ≈ 45 ns**：包含中型结构（String + Vec）的真实拷贝成本；纯开销部分（查找 + downcast）
  与 require 同量级。
- **build ≈ 625 ns / 3 模块**：图校验（缺依赖 + 环检测）+ Kahn 排序 + 3 次构建回调，
  启动期一次性成本，量级符合预期。

## 🔁 复现与对比

1. 固定机器插电、关闭省电模式；
2. `cargo bench --features toggle -- --save-baseline <name>` 保存基线；
3. 改动后 `cargo bench --features toggle -- --baseline <name>` 对比；
   criterion 会在输出中标注回归/改进（noise_threshold=5%）。

## 🚦 CI 阈值（已启用，数量级护栏）

- ci.yml bench job 断言 7 个基准中位数不超过本地基线 ×100 后向上取整到整千
  （`scripts/bench_gate.py`），仓库变量 `BENCH_GATE_DISABLED=true` 可整体停用。
- **为什么是 ×100**：GitHub 共享 runner（ubuntu-latest）与本地的差距由三层叠加——
  单核性能差（约 3–10×）、虚拟化/邻居噪音（约 3–10×）、保守余量。合计上限取两个
  数量级。门禁定位是灾难性回归护栏（算法复杂度劣化、锁竞争爆炸、意外分配风暴等
  ≥100× 的悬崖），不是严格回归检测；共享 runner 上任何更紧的阈值都会被噪音击穿而
  失去门禁意义。
- **覆盖告警**：每次运行把 `target/criterion` 下实际产出 `new/estimates.json` 的基准
  目录集与 `LIMITS_NS` 键集求差，未纳管的基准打 WARN 到 stderr（不失败——criterion
  会保留已删除/改名基准的陈旧目录，fail 会产生误报）；新增基准应同步补录阈值并重测基线。
- **基线刷新**：升级 rustc / criterion / 机器 / feature 集后，重跑本地 bench 并
  **两处一致变更**：本文件基线表 + `scripts/bench_gate.py` 的 `LIMITS_NS`（提交注明触发原因）。
- ±20% 级的精细回归判断仍走本地 `--save-baseline`/`--baseline` 流程；多机方差
  数据稳定后可再收紧 CI 阈值（届时按刷新规则两处一致变更）。
