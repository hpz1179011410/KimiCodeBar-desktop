#!/usr/bin/env bash
# 本机交叉编译检查辅助：对 linux 目标跑 `cargo check`。
#
# linux 目标的 sys crate 需要 pkg-config 探测 GTK / D-Bus 等系统库，本机（Windows）
# 没有这些库。由于 cargo check 不链接，只需让探测通过：
#   - gtk 系（system-deps）：SYSTEM_DEPS_*_NO_PKG_CONFIG=1 + SYSTEM_DEPS_*_LIB 占位
#   - libdbus-sys（无旁路）：用 scripts/cross-check/pkg-config.exe 替身
#
# 真实 linux 打包仍需安装系统依赖（见 README"跨平台支持"一节）。
# 用法：bash scripts/cross-check/linux.sh

set -euo pipefail
export PATH="$HOME/.cargo/bin:$PATH"
cd "$(dirname "$0")/../.."  # 仓库根目录

# 1. 构建 pkg-config 替身与假交叉 C 工具链（已存在则跳过）
if [ ! -f scripts/cross-check/pkg-config.exe ]; then
    rustc -O scripts/cross-check/pkg-config.rs -o scripts/cross-check/pkg-config.exe
fi
if [ ! -f scripts/cross-check/fake-cc.exe ]; then
    rustc -O scripts/cross-check/fake-cc.rs -o scripts/cross-check/fake-cc.exe
fi

# 2. 环境变量
CROSS_DIR="$(pwd -W 2>/dev/null || pwd)/scripts/cross-check"
export PKG_CONFIG="$CROSS_DIR/pkg-config.exe"
export PKG_CONFIG_ALLOW_CROSS=1
# ring（rustls）的 build.rs 需要交叉 C 工具链；check 不链接，用替身生成空产物
export CC_x86_64_unknown_linux_gnu="$CROSS_DIR/fake-cc.exe"
export AR_x86_64_unknown_linux_gnu="$CROSS_DIR/fake-cc.exe"

# system-deps 旁路（SYSTEM_DEPS_<NAME>_NO_PKG_CONFIG，NAME 为 shouty-snake 的库名）
for name in GLIB_2_0 GOBJECT_2_0 GIO_2_0 GTK_3_0 GDK_3_0 GDK_PIXBUF_2_0 GDK_X11_3_0 \
            ATK CAIRO CAIRO_GOBJECT PANGO JAVASCRIPTCOREGTK_4_1 LIBSOUP_3_0 WEBKIT2GTK_4_1; do
    export "SYSTEM_DEPS_${name}_NO_PKG_CONFIG=1"
    export "SYSTEM_DEPS_${name}_LIB=${name,,}"  # 占位链接名，check 不链接
done

# 3. 检查
cd src-tauri
exec cargo check --target x86_64-unknown-linux-gnu "$@"
