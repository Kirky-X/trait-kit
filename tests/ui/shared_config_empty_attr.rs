//! 空的 `#[shared()]` 无共享字段可提取，必须编译期拒绝。
//! 恰好一个错误：空列表 panic，无生成代码。

use trait_kit_macros::SharedConfig;

#[derive(SharedConfig)]
#[shared()]
struct S {
    host: String,
}

fn main() {}
