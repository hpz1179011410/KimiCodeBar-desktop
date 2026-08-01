# KimiCodeBar-desktop

专为 [Kimi Code](https://github.com/MoonshotAI/kimi-code) 打造的跨平台桌面用量监控工具（**Windows 版已发布**，macOS / Linux 可从源码构建）。左键托盘图标弹出面板，周/5 小时/月度配额一目了然，额度不足托盘图标变红提醒。

> **本项目基于 macOS 版 [xifandev/KimiCodeBar](https://github.com/xifandev/KimiCodeBar) 重新开发**（非 fork，Windows 平台全新实现）。

中文 | [English](README_EN.md)

## 致谢

- **[xifandev/KimiCodeBar](https://github.com/xifandev/KimiCodeBar)**——macOS 原版项目，本产品创意与功能设计的来源，感谢作者 [@xifandev](https://github.com/xifandev) 的开源分享
- **[JYH1878/KimiCodeBar-Windows](https://github.com/JYH1878/KimiCodeBar-Windows)**——感谢作者 [@JYH1878](https://github.com/JYH1878)，本项目的 Kimi 接口协议细节（OAuth Device Flow、配额/月度接口的响应结构与字段口径）参考了该项目的公开实现与测试样本
- **[MoonshotAI/kimi-code](https://github.com/MoonshotAI/kimi-code)**——Kimi Code CLI 官方项目

## 功能

- **托盘实时监控**：左键弹出面板、右键菜单（控制台/刷新/设置/退出），悬停 tooltip 显示完整配额；任一窗口剩余 <20% 托盘图标变红
- **配额面板**：
    - Kimi 订阅组合卡片：本周、5 小时、本月三档用量与加油包集中展示（百分比 + 进度条 + 重置倒计时 / 加油包余额与月度上限）
    - 本月数据通过网页端 `kimi-auth` cookie 接入控制台同款接口，含 Kimi Code 占比
- **OpenCode Go 订阅**：读取 Workspace Go 额度页，集中显示 5 小时（$12）、周（$30）、月（$60）三档额度的剩余百分比、美元金额、按欧洲央行每日参考汇率折算的人民币估值与重置倒计时，三档额度可单独显隐
- **本机用量统计**：今日/昨日 Token 消耗、最近 7 天柱状图（悬停查看明细）、按模型 Top 5、**缓存命中率**（今日/7 天/每日/各模型）；支持按含供应商的完整配置别名识别并标注 `SECONDARY_MODEL`
- **模型趋势卡片**：同图对比多个模型最近 7 天的每日趋势，可切换 Token 用量 / 缓存命中率并按天悬停查看明细
- **面板自定义**：业务卡片均可独立显隐并调整顺序；Kimi / OpenCode Go 组合卡片的内部额度行也支持独立显隐与排序
- **会话自动归档**：按期限（一天/一周/一月）自动归档旧会话，可手动归档/取消归档
- **桌面小部件**：直接复用 Kimi / OpenCode Go 订阅组合卡片，主面板的额度行显隐与排序会同步生效；可拖拽调整位置并记忆，支持两张订阅卡显隐与排序
- **技能管理**：浏览 `~/.kimi-code/skills` 下的技能定义与全文
- **更新提醒**：自动探测 Kimi Code CLI 新版本与应用自身新版本
- **体验细节**：中英双语、深色/浅色/跟随系统主题、开机自启、卡片入场与进度条/柱状图动效（`prefers-reduced-motion` 自动降级）、刷新 shimmer 加载、滚动条按需显示

## 技术栈

- **后端**：Rust + Tauri 2（托盘、窗口、定时轮询、增量文件扫描）
- **前端**：React 19 + TypeScript + Vite + react-i18next
- **安全**：凭证全平台存系统钥匙串（keyring）：Windows 凭据管理器 / macOS Keychain / Linux Secret Service；OAuth 凭证在 Windows 上另经 **DPAPI**（CurrentUser 作用域）加密存本地文件
- **安装包**：Windows 为 NSIS（currentUser 安装，无需管理员权限）；macOS / Linux 按 tauri 默认产出各平台原生包

## 跨平台支持

| 平台          | 状态                                                                                                | 凭证存储（keyring 后端）                  | 配置目录                                                     |
| ------------- | --------------------------------------------------------------------------------------------------- | ----------------------------------------- | ------------------------------------------------------------ |
| Windows 10/11 | ✅ 完整支持，NSIS 安装包已发布                                                                      | 凭据管理器（OAuth 另走 DPAPI 密文文件）   | `%APPDATA%\KimiCodeBar`                                      |
| macOS         | 🚧 代码可编译（`cargo check --target aarch64-apple-darwin` 通过），签名 / 公证 / 打包与实机验证待做 | Keychain                                  | `~/Library/Application Support/KimiCodeBar`                  |
| Linux         | 🚧 代码可编译（`cargo check --target x86_64-unknown-linux-gnu` 通过），桌面环境实机验证待做         | Secret Service（gnome-keyring / KWallet） | `$XDG_CONFIG_HOME/KimiCodeBar`（或 `~/.config/KimiCodeBar`） |

**macOS 源码构建**：安装 Rust 与 Node.js 后 `npm install && npm run tauri build`，产出 `.app` / `.dmg`。分发需自行配置 Apple 开发者证书签名与公证（tauri.conf.json 未内置签名配置）。macOS 托盘当前使用彩色图标，未做明暗模板适配（后续可改 template image）。

**Linux 源码构建**：除 Rust 与 Node.js 外需系统依赖（以 Debian/Ubuntu 为例）：

```bash
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libgtk-3-dev libdbus-1-dev libayatana-appindicator3-dev
npm install && npm run tauri build    # 产出 .deb / AppImage 等
```

运行时凭证存储需要 D-Bus session bus 与 Secret Service 实现（gnome-keyring 或 KWallet），无桌面钥匙串的极简环境暂不支持。

## 数据与隐私

- 数据仅本地存储：配置与凭证位于各平台配置目录（Windows 为 `%APPDATA%\KimiCodeBar\`，见"跨平台支持"表；可用 `KIMICODEBAR_CONFIG_DIR` 覆盖）
- OAuth Device Flow 与 Kimi Code CLI 官方流程一致；凭证与 CLI 隔离存放，互不影响
- 本地用量统计**只读**扫描 `~/.kimi-code/sessions` 的 `wire.jsonl`，增量解析（字节偏移持久化）
- OpenCode Go 的 Workspace ID 与 `auth` Cookie 仅存系统钥匙串；额度刷新只请求用户自己的 `opencode.ai/workspace/{id}/go` 页面
- 网络请求只与 Kimi 官方（`api.kimi.com` / `auth.kimi.com` / `www.kimi.com`）、OpenCode 官方（`opencode.ai`）、欧洲央行（`ecb.europa.eu`，每日参考汇率）及 GitHub（更新检查）通信

## 开发

前置要求（Windows 开发）：

- [Rust](https://rustup.rs/)（≥ 1.77，MSVC 工具链）+ [Visual Studio C++ 生成工具](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- Node.js ≥ 18
- Windows 10/11（WebView2，系统一般自带）

macOS / Linux 的构建前置见上文"跨平台支持"一节。在 Windows 上验证 macOS / Linux 编译：

```bash
rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin
cd src-tauri && cargo check --target aarch64-apple-darwin   # macOS
bash scripts/cross-check/linux.sh                          # Linux（内置 pkg-config / 交叉 gcc 旁路）
```

```bash
npm install
npm run tauri dev      # 开发模式（前端热更新 + Rust 自动重编译）
npm run tauri build    # 产出 NSIS 安装包（src-tauri/target/release/bundle/nsis/）
```

## 测试

```bash
cd src-tauri && cargo test     # Kimi/OpenCode Go 配额解析 / OAuth 错误分类 / 增量扫描 / 月度接口解析
cargo clippy                   # 静态检查
npm run build                  # 前端类型检查 + 构建
```

## 项目结构

```
├── src/                  # React 前端
│   ├── views/            # 页面入口（panel.tsx 面板 / settings.tsx 设置 / widget.tsx 小部件）
│   ├── components/       # 卡片与登录覆盖层组件
│   ├── lib/              # ipc 封装、格式化与主题工具
│   ├── types/            # 与 Rust serde 输出对齐的 TS 类型
│   ├── i18n/             # 中英双语
│   └── styles/           # 全局样式
├── src-tauri/
│   ├── src/
│   │   ├── kimi/         # Kimi 接口层（OAuth Device Flow、配额、网页端月度）
│   │   ├── opencode_go.rs# OpenCode Go Workspace 额度读取与解析
│   │   ├── local_usage.rs# wire.jsonl 增量扫描与缓存命中率
│   │   ├── archive.rs    # 会话自动归档
│   │   ├── polling.rs    # 配额定时轮询
│   │   └── ...           # 托盘、设置存储、更新检查、命令层
│   └── tests/            # 单元测试 + 真实响应 fixtures
└── scripts/              # 图标生成等工具脚本
```

## License

MIT
