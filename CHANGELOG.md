# Changelog

所有显著变更记录于此。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## Unreleased

## [0.5.0-rc.13] - 2026-11-12

### Added

- `I18nManager` 宿主注册 API：`register_translation(locale, key, value)` /
  `clear_registered_translations()`——宿主（sdforge 桥接层及各下游仓）可在运行期注册自有
  翻译并覆盖内置文案，回退链保持 注册 → 内置 → 键本身；占位符语义与内置 Fluent 路径一致，
  含 8 条单测（覆盖/归一/后写胜出/回退链/占位符一致性/清除恢复）。

## [0.5.0-rc.12] - 2026-10-31

### Added

- `masking` feature：PII/凭据掩码引擎（`DataMasker` / `MaskRule` / `MaskRuleRegistry` /
  `MaskMatch` / `AcMasker`）自 inklog 下沉为本仓单一来源，含 secret-scan 出站门族
  （`SecretScanGate` / `SecretPatternRegistry` / `EntropyScanner`）与
  `MaskError` 错误类型；inklog 与 limiteron 共用同一实现，21 条内置规则语义与顺序不变。
