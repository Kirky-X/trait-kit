//! `deps` 既非 `deps(A, B)` 也非 `deps = [A, B]` 时报语法错误。
//! 恰好一个错误：解析中止于该属性，无生成代码。

use trait_kit_macros::Module;

#[derive(Module)]
#[module(deps)]
struct M;

fn main() {}
