// 假的交叉 C 工具链替身（cc / ar 二合一，仅供本机跑 linux 目标 `cargo check`）。
//
// 背景：ring（rustls 的加密后端）的 build.rs 用 cc-rs 编译 C/汇编源码，
// 需要 x86_64-linux-gnu-gcc；本机（Windows）没有交叉 gcc。
// `cargo check` 只做类型检查、从不链接，因此编译产物无需真实：
//   - cc 模式：对 `-o <file>` 写出空目标文件，全部参数忽略、退出码 0；
//     `--version` 输出伪装版本串（cc-rs 的工具族检测走 -E 预处理，失败会回退 GNU，不影响）；
//   - ar 模式（cc-rs 打包静态库 `ar crus lib.a *.o`）：对 `.a` 参数写出空归档头。
// 通过环境变量注入：CC_x86_64_unknown_linux_gnu / AR_x86_64_unknown_linux_gnu。
//
// 注意：这只服务 `cargo check`。真实 linux 构建必须在有交叉/原生工具链的机器上进行。

use std::env;
use std::fs;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();

    // --version / -v：伪装 GNU 版本串
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("gcc (fake cross tool for cargo check) 13.0.0");
        return;
    }

    // cc 模式：-o <path> → 空目标文件
    if let Some(pos) = args.iter().position(|a| a == "-o") {
        if let Some(out) = args.get(pos + 1) {
            let _ = fs::write(out, b"");
        }
    }

    // ar 模式：参数里的 .a → 最小归档头
    for a in &args {
        if a.ends_with(".a") {
            let _ = fs::write(a, b"!<arch>\n");
        }
    }
}
