# Agents Guide

## Overview

trait-kit 是 Rust 生态的统一 trait 定义和 DI 装配层，提供标准化的模块接口（`ModuleMeta` + `AutoBuilder`）和集中式能力与配置管理中心（`Kit`）。采用 typestate 模式确保构建时验证，基于 `RefCell` 实现单线程内部可变性。

## Project Structure

```
src/
├── lib.rs              # crate 入口，#![deny(unsafe_code)]
├── prelude.rs          # 常用类型再导出
├── core/
│   ├── mod.rs          # 模块声明
│   ├── meta.rs         # ModuleMeta + AutoBuilder + AsyncAutoBuilder trait
│   └── error.rs        # KitError 错误类型
├── kit/
│   ├── mod.rs          # Kit 模块声明
│   ├── kit.rs          # Kit<Unbuilt> → Kit<Ready> typestate 实现
│   ├── graph.rs        # 依赖图：环检测 + 拓扑排序
│   ├── typemap.rs      # TypeMap：以 TypeId 为键的能力存储
│   ├── async_kit.rs    # AsyncKit（async feature）
│   ├── async_typemap.rs # AsyncTypeMap（async feature）
│   └── config.rs       # confers 集成（confers feature）
└── i18n/
    └── mod.rs          # ICU4X 国际化（i18n feature）
```

## Where to Look

- 核心定义: `src/lib.rs`
- 模块 trait: `src/core/meta.rs`
- 错误类型: `src/core/error.rs`
- Kit 实现: `src/kit/kit.rs`
- 依赖图: `src/kit/graph.rs`
- 异步 Kit: `src/kit/async_kit.rs`
- confers 配置: `src/kit/config.rs`
- 国际化: `src/i18n/mod.rs`
- 预导出: `src/prelude.rs`

## Conventions

- edition 2024, rust 1.85+
- MIT License
- 依赖必须通过 feature 门控，禁止使用默认特性
- TDD 开发流程
- 中文注释
- `#![deny(unsafe_code)]` — 禁止 unsafe
- `#![warn(clippy::all, clippy::pedantic)]` — 启用 pedantic lint

## Commands

- `cargo build --all-features`
- `cargo test --all-features --lib`
- `cargo fmt`
- `cargo clippy --all-features -- -D warnings`
- `cargo doc --no-deps --all-features`
