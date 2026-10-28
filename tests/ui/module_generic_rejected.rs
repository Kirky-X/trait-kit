//! 泛型结构体不能派生 `Module`：`TypeId::of::<M>` 要求具体类型。
//! 恰好一个错误：expand_module 前置拒绝，返回 compile_error。

use trait_kit_macros::Module;

#[derive(Module)]
struct Generic<T> {
    value: T,
}

fn main() {}
