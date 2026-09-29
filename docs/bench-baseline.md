# 性能基线（bench-baseline）

> 供 CI bench 门禁与本地回归对照使用的基线台账（本文件的**单一事实源**地位覆盖
> docs/PERFORMANCE.md 的基线段——后者是面向读者的摘要，刷新时两处一致变更）。
> **口径**：criterion 中位点估计（`target/criterion/<bench>/new/estimates.json` 的
> `median.point_estimate`，单位 ns），20 samples × 1s 测量、500ms 预热的短模式
> （`benches/kit_bench.rs::bench_config_short`）。环境与运行命令见 docs/PERFORMANCE.md；
> 数字为单次运行的实测值，非多次取平均。

## 本地基线（2026-09-29 安静复测，本仓库门禁推导源）

- 机器：AMD Ryzen 9 9950X（16C/32T），WSL2 kernel 6.6.87
- 负载：`/proc/loadavg` 1 分钟均值 1.12（近似安静；本轮为审查后的全量重跑）
- 命令：`cargo bench --features toggle,confers`
- HEAD：c04d9ab（feature 集 toggle,confers，无 report——`config/write_merge_config`
  在 report 组合下被编译剔除，见 PERFORMANCE.md 测量说明）

| 基准 | 中位（ns） | 说明 |
| --- | ---: | --- |
| build/three_module_chain | 625 | 3 模块链注册 + `build()`（含图校验/拓扑排序/逐模块构建） |
| require/arc_capability_top | 16.3 | `require::<M>()` 取 `Arc` 能力 |
| config/read_clone | 45.4 | `config::<C>()` 读（含 `Clone` 拷贝） |
| config/write_set_config | 36.2 | `set_config` 覆写 |
| config/write_merge_config | 38.8 | `merge_config` 读-改-写 |
| toggle/set | 61.2 | `enable_toggle` |
| toggle/get | 1728 | `is_toggle_enabled`（未命中 → 命中路径均为此量级） |

### 测量可复现性备注

- `toggle/get` 在本机两次独立全量运行（高负载时段 1778ns、负载 1.12 复测 1728ns）
  稳定在 ~1.7µs 量级；审查轮曾观察到一次 454ns 的单点值，在本机不可复现，不作为
  基线。若未来出现稳定的更低值，按下方刷新规则重测并同步三处。
- 历史（2026-09-10）快照中 toggle/set ≈21ns、toggle/get ≈12ns 与当前 HEAD 的实测
  （61ns/1728ns）不可对齐，根因未查明（其测量条件无法复现）；该快照仅作历史记录
  保留在 PERFORMANCE.md，不参与门禁推导。
- `report` 特性的记录/排空路径（`merge_config` 的 `push_config_override` 记录成本与
  `take_config_overrides` 的 `mem::take` 轮转换出）**无基准覆盖，属显式非目标**：
  `config/write_merge_config` 在 report 组合下被编译剔除（迭代式基准下记录 `Vec`
  无上限增长，见 benches/kit_bench.rs 与 PERFORMANCE.md 测量说明），CI bench 亦只跑
  `toggle,confers`。该路径为诊断用途、量级远低于热路径（每次调用一次 `Vec` push /
  一次整体换出），不要因本台账无此轴而误以为已测。

## CI 门禁阈值（推导与口径）

**规则**：CI 阈值 = 本地中位 × 100 后**向上取整到整千**，由
`scripts/bench_gate.py` 的 `LIMITS_NS` 表承载（`cargo bench --features
toggle,confers` 跑完后本地 `python3 scripts/bench_gate.py` 即可复验；ci.yml
bench job 调用同一脚本）。

**为什么是 ×100**：GitHub 共享 runner（ubuntu-latest）与本地的差距由三层叠加——
单核性能差（约 3–10×）、虚拟化/邻居噪音（约 3–10×）、保守余量。合计上限取两个
数量级。**门禁定位是灾难性回归护栏**（算法复杂度劣化、锁竞争爆炸、意外分配风暴
等 ≥100× 的悬崖），不是严格回归检测；±20% 级的精细回归判断只能在同一台机器的
本地基线流程做（见 PERFORMANCE.md），共享 runner 上任何更紧的阈值都会被噪音击穿
而失去门禁意义。

**开关**：bench job 受仓库变量 `vars.BENCH_GATE_DISABLED` 控制——Settings →
Secrets and variables → Actions → Variables 新建 `BENCH_GATE_DISABLED=true` 即整体
停用（如 runner 池迁移导致的暂时性误报期间）；未设置时默认启用。

**覆盖告警**：脚本每次运行会把 `target/criterion` 下实际产出 `new/estimates.json`
的基准目录集与 `LIMITS_NS` 键集求差，未纳管的基准打 WARN 到 stderr（不失败——
criterion 会保留已删除/改名基准的陈旧目录，fail 会产生误报）；新增基准应同步补
录阈值并重测基线。

**基线刷新**：升级 rustc / criterion / 机器 / feature 集后，重跑本地 bench 并**三处
一致变更**：本表 + `scripts/bench_gate.py` 的 `LIMITS_NS` + docs/PERFORMANCE.md
基线段（提交注明触发原因）。
