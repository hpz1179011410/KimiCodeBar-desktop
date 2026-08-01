// 最小的 pkg-config 替身（仅供本机对 x86_64-unknown-linux-gnu 跑 `cargo check`）。
//
// 背景：linux 目标下 libdbus-sys（tauri/tao 与 keyring 的 secret-service 后端都会引入）
// 的 build.rs 调用 pkg-config 探测 dbus-1；gtk 系 sys crate（system-deps）可用
// SYSTEM_DEPS_*_NO_PKG_CONFIG 环境变量跳过，唯独 libdbus-sys 没有旁路。
// cargo check 只做类型检查、不链接，因此这里只需让探测"成功"：
//   - `--modversion` 输出一个 >= 1.6 的版本号（libdbus-sys 要求 atleast 1.6）
//   - `--libs --cflags` 输出空（check 不需要真实库路径）
//
// 用法：rustc -O pkg-config.rs（产出 pkg-config.exe），cross-check.sh 已封装。

use std::env;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--modversion") {
        println!("1.14.10");
    }
    // --libs / --cflags：输出空即可
}
