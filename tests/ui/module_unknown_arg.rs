//! `#[module(...)]` 未知参数必须在宏展开期报出带 span 的命名错误。
//! 恰好一个错误：syn 解析失败不产生任何生成代码，无级联。

use trait_kit_macros::Module;

#[derive(Module)]
#[module(nme = "x")]
struct M;

fn main() {}
