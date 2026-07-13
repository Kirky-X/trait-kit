# 贡献指南

## 开发环境

- Rust 1.91+ (edition 2024)
- cargo, rustfmt, clippy
- pre-commit (pip install pre-commit && pre-commit install)

## TDD 工作流

按照以下循环执行：

1. **定接口**：先定义 trait / API 签名，不写实现
2. **写测试**：基于接口编写单元测试，此时测试应失败（red）
3. **写代码**：实现接口，使测试通过（green）
4. **跑测试**：`cargo test --features <对应特性> --lib`
5. **commit**：`git commit -m "feat(<模块>): <描述>"`
6. **gitnexus analyze**：分析对其他模块的影响
7. **继续下一个**

## Pre-commit Hooks

- `cargo fmt --check`
- `cargo clippy -D warnings`
- `cargo check`
- 禁止使用 `--no-verify` 跳过

## 代码质量

- **diting**: 代码简化、架构优化、性能审查
- **tiangang**: SAST 安全扫描（0 CRITICAL）
- **kueiku**: 硬性 bug 分析

## Pull Request 流程

1. 创建 feature 分支（禁止直接提交到 main）
2. 确保 pre-commit hooks 通过
3. 确保 `cargo test --all-features` 通过
4. PR 描述包含变更说明和测试结果

## 代码风格

- 遵循现有代码库命名和架构惯例
- 不添加不必要的注释、docstring 或类型标注
- 简洁优先：只写能解决问题的最少代码
