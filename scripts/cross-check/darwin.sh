#!/usr/bin/env bash
# 本机交叉编译检查辅助：对 macOS (Apple Silicon) 目标跑 `cargo check`。
#
# macOS 目标大多无需系统探测（objc / Security.framework 均为纯 Rust 绑定），
# 唯一障碍是 ring（rustls 加密后端）的 build.rs 需要交叉 C 工具链——
# 与 linux 相同，用 fake-cc.exe 替身（check 不链接，空产物即可）。
# 真实 macOS 打包需在 macOS 上签名、公证并构建，见 README"跨平台支持"一节。
# 用法：bash scripts/cross-check/darwin.sh

set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/../.."  # 仓库根目录

if [ ! -f scripts/cross-check/fake-cc.exe ]; then
    rustc -O scripts/cross-check/fake-cc.rs -o scripts/cross-check/fake-cc.exe
fi

CROSS_DIR="$(pwd -W 2>/dev/null || pwd)/scripts/cross-check"
export CC_aarch64_apple_darwin="$CROSS_DIR/fake-cc.exe"
export AR_aarch64_apple_darwin="$CROSS_DIR/fake-cc.exe"

cd src-tauri
exec cargo check --target aarch64-apple-darwin "$@"
