# KimiCodeBar-desktop

A cross-platform desktop usage monitor for [Kimi Code](https://github.com/MoonshotAI/kimi-code) (**Windows release available**, macOS / Linux buildable from source). Click the tray icon to open the panel and see your weekly / 5-hour / monthly quota at a glance; the tray icon turns red when you're running low.

> **This project is a re-development based on the macOS version [xifandev/KimiCodeBar](https://github.com/xifandev/KimiCodeBar)** (not a fork — a fresh implementation for Windows).

[中文](README.md) | English

## Acknowledgments

- **[xifandev/KimiCodeBar](https://github.com/xifandev/KimiCodeBar)** — the original macOS project and the source of this product's concept and feature design. Thanks to [@xifandev](https://github.com/xifandev) for open-sourcing it.
- **[JYH1878/KimiCodeBar-Windows](https://github.com/JYH1878/KimiCodeBar-Windows)** — thanks to [@JYH1878](https://github.com/JYH1878). The Kimi API protocol details in this project (OAuth Device Flow, response structures and field semantics of the quota / monthly endpoints) reference this project's public implementation and test fixtures.
- **[MoonshotAI/kimi-code](https://github.com/MoonshotAI/kimi-code)** — the official Kimi Code CLI project.

## Features

- **Tray monitoring**: left-click to open the panel, right-click for the menu (Console / Refresh / Settings / Quit), hover tooltip with full quota info; the tray icon turns red when any window drops below 20% remaining
- **Quota panel**:
    - Combined Kimi subscription card for weekly, 5-hour, and monthly usage plus the booster pack (percentage + progress bar + reset countdown / booster balance and monthly cap)
    - Monthly data uses the web `kimi-auth` cookie and the same endpoint as the official console, including the Kimi Code share
- **OpenCode Go subscription**: reads the Workspace Go quota page and shows the 5-hour ($12), weekly ($30), and monthly ($60) quota in one card, including remaining percentage, USD spend, CNY estimates converted with the ECB daily reference rate, and reset countdown; each quota row can be toggled independently
- **Local usage stats**: today / yesterday token consumption, 7-day bar chart (hover for details), Top 5 models, **cache hit rate** (today / 7 days / per-day / per-model); resolves and labels `SECONDARY_MODEL` by its full provider-qualified config alias
- **Model trends card**: compare all models in one 7-day line chart, switch between token usage and cache hit rate, and hover by day for details
- **Panel customization**: independently toggle and reorder all metric cards; quota rows inside the combined Kimi / OpenCode Go cards can also be toggled and reordered
- **Auto session archiving**: archive old sessions by threshold (1 day / 1 week / 1 month), with manual archive / unarchive
- **Desktop widget**: directly reuses the combined Kimi / OpenCode Go subscription cards and follows the panel's quota-row visibility and ordering; draggable with position memory, with both subscription cards toggleable and reorderable
- **Skills browser**: view skill definitions under `~/.kimi-code/skills`
- **In-app updates**: detects new Kimi Code CLI and app versions, shows release notes before offering a background download, verifies the signature, then lets the user install now or later
- **Experience**: Chinese / English UI, dark / light / system theme, launch at login, card entrance animations with `prefers-reduced-motion` support, shimmer loading on refresh, scroll-on-demand scrollbar

## Tech Stack

- **Backend**: Rust + Tauri 2 (tray, windows, scheduled polling, incremental file scanning)
- **Frontend**: React 19 + TypeScript + Vite + react-i18next
- **Security**: credentials live in the system keyring on every platform (keyring): Windows Credential Manager / macOS Keychain / Linux Secret Service; on Windows, OAuth credentials are additionally encrypted with **DPAPI** (CurrentUser scope) into a local file
- **Installer**: NSIS on Windows (per-user install, no admin rights required); macOS / Linux produce their native bundles via tauri defaults

## Cross-Platform Support

| Platform      | Status                                                                                                                                       | Credential storage (keyring backend)                                | Config directory                                            |
| ------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- | ----------------------------------------------------------- |
| Windows 10/11 | ✅ Fully supported, NSIS installer released                                                                                                  | Credential Manager (OAuth additionally uses a DPAPI-encrypted file) | `%APPDATA%\KimiCodeBar`                                     |
| macOS         | 🚧 Code compiles (`cargo check --target aarch64-apple-darwin` passes); signing / notarization / packaging and on-device verification pending | Keychain                                                            | `~/Library/Application Support/KimiCodeBar`                 |
| Linux         | 🚧 Code compiles (`cargo check --target x86_64-unknown-linux-gnu` passes); desktop environment verification pending                          | Secret Service (gnome-keyring / KWallet)                            | `$XDG_CONFIG_HOME/KimiCodeBar` (or `~/.config/KimiCodeBar`) |

**Building on macOS**: with Rust and Node.js installed, run `npm install && npm run tauri build` to produce a `.app` / `.dmg`. Distribution requires your own Apple Developer certificate signing and notarization (no signing config is bundled in tauri.conf.json). The tray currently uses a colored icon and does not adapt to light/dark menu bar templates yet (template image is a possible follow-up).

**Building on Linux**: besides Rust and Node.js you need system dependencies (Debian/Ubuntu example):

```bash
sudo apt install libwebkit2gtk-4.1-dev libsoup-3.0-dev libgtk-3-dev libdbus-1-dev libayatana-appindicator3-dev
npm install && npm run tauri build    # produces .deb / AppImage etc.
```

At runtime, credential storage requires a D-Bus session bus and a Secret Service implementation (gnome-keyring or KWallet); minimal environments without a desktop keyring are not supported yet.

## Data & Privacy

- All data stays local: config and credentials live in the platform config directory (Windows: `%APPDATA%\KimiCodeBar\`; see the "Cross-Platform Support" table; overridable via `KIMICODEBAR_CONFIG_DIR`)
- OAuth Device Flow follows the official Kimi Code CLI flow; credentials are stored separately from the CLI and never interfere with it
- Local usage scanning **reads only** `~/.kimi-code/sessions/**/wire.jsonl`, with incremental parsing (persisted byte offsets)
- The OpenCode Go Workspace ID and `auth` cookie are stored only in the system keyring; refreshes access only the user's own `opencode.ai/workspace/{id}/go` page
- Network requests go only to Kimi (`api.kimi.com` / `auth.kimi.com` / `www.kimi.com`), OpenCode (`opencode.ai`), the ECB (`ecb.europa.eu`, daily reference rates), and GitHub (update checks and signed installer downloads)

## Development

Prerequisites (Windows development):

- [Rust](https://rustup.rs/) (≥ 1.77, MSVC toolchain) + [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)
- Node.js ≥ 18
- Windows 10/11 (WebView2, usually preinstalled)

For macOS / Linux build prerequisites see "Cross-Platform Support" above. To verify macOS / Linux compilation from Windows:

```bash
rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin
cd src-tauri && cargo check --target aarch64-apple-darwin   # macOS
bash scripts/cross-check/linux.sh                          # Linux (bundled pkg-config / cross-gcc shims)
```

```bash
npm install
npm run tauri dev      # Dev mode (Vite on :1420 + Rust auto-rebuild)
npm run tauri build    # Produce the NSIS installer (src-tauri/target/release/bundle/nsis/)
```

## Testing

```bash
cd src-tauri && cargo test     # Kimi/OpenCode Go quota parsing / OAuth errors / incremental scanning / monthly API parsing
cargo clippy                   # Lint
npm run build                  # Frontend type check + build
npm run format:check           # Prettier style check (4-space indent)
```

## Project Structure

```
├── src/                  # React frontend
│   ├── views/            # Page entries (panel.tsx panel / settings.tsx settings / widget.tsx widget)
│   ├── components/       # Cards and login overlay
│   ├── lib/              # IPC wrappers, formatting and theme utilities
│   ├── types/            # TS types aligned with Rust serde output
│   ├── i18n/             # Chinese / English
│   └── styles/           # Global styles
├── src-tauri/
│   ├── src/
│   │   ├── kimi/         # Kimi API layer (OAuth Device Flow, quota, web monthly)
│   │   ├── opencode_go.rs# OpenCode Go Workspace quota fetching and parsing
│   │   ├── local_usage.rs# Incremental wire.jsonl scanning & cache hit rate
│   │   ├── archive.rs    # Session auto-archiving
│   │   ├── polling.rs    # Quota polling
│   │   └── ...           # Tray, settings storage, update checks, commands
│   └── tests/            # Unit tests + real-response fixtures
└── scripts/              # Icon generation and other tooling
```

## License

MIT
