//! `ConfigInherit` 只支持命名字段结构体；enum 触发宏 panic。
//! 恰好一个错误：proc-macro panic 报为单条编译错误。

use trait_kit_macros::ConfigInherit;

#[derive(ConfigInherit)]
enum NotAStruct {
    A,
}

fn main() {}
