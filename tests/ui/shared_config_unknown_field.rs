//! `#[shared(...)]` 列出不存在的字段必须在编译期 panic 指名道姓。
//! 恰好一个错误：字段校验 panic，无生成代码。

use trait_kit_macros::SharedConfig;

#[derive(SharedConfig)]
#[shared(nonexistent)]
struct S {
    host: String,
}

fn main() {}
