# Changelog

所有显著变更记录于此。
格式参考 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## Unreleased

## [0.5.0-rc.12] - 2026-10-31

### Added

- `masking` feature：PII/凭据掩码引擎（`DataMasker` / `MaskRule` / `MaskRuleRegistry` /
  `MaskMatch` / `AcMasker`）自 inklog 下沉为本仓单一来源，含 secret-scan 出站门族
  （`SecretScanGate` / `SecretPatternRegistry` / `EntropyScanner`）与
  `MaskError` 错误类型；inklog 与 limiteron 共用同一实现，21 条内置规则语义与顺序不变。
