# Changelog

[中文](CHANGELOG.md) | English

## [Unreleased]

## [0.1.2] - 2026-08-02

### Added

- **In-app updates**: shows release notes when a new version is found and lets the user choose whether to download it in the background; Tauri verifies the package signature before offering immediate or deferred installation

### Improved

- **Refresh performance**: monthly usage and OpenCode Go now share short-lived cross-window caches and coalesce concurrent requests; quota polling and manual refreshes reuse in-flight work, while panel initialization and all four data sources refresh concurrently
- **Rendering performance**: quota countdowns share clocks, cards and model trends avoid redundant renders, hovering the trend chart no longer recalculates static paths, and off-screen cards defer painting
- **UI details**: unified panel spacing, focus states, settings grouping, and numeric alignment, and fixed the model trend plot alignment with its legend text

## [0.1.1] - 2026-08-02

### Added

- **OpenCode Go subscription card**: validates a Workspace ID and `auth` cookie against the official Go quota page, then shows 5-hour, weekly, and monthly remaining percentage, USD spend, ECB-reference-rate CNY estimates, and reset countdown in one card; credentials stay in the system keyring, and both the card and its quota rows can be toggled and reordered

### Improved

- **Refresh performance**: Kimi quota, local usage, monthly usage, and OpenCode Go now refresh concurrently; CNY exchange rates use stale-while-revalidate, returning cached values immediately, updating expired data in the background, and waiting at most 2 seconds on the first uncached request
- **Kimi subscription card**: the separate weekly, 5-hour, monthly, and booster cards are combined into the same compact layout as OpenCode Go; existing card order migrates automatically while all four rows can be independently toggled and reordered
- **Desktop widget**: now directly reuses the combined Kimi and OpenCode Go subscription cards, sharing quota-row visibility and ordering with the main panel; legacy monthly, weekly, and 5-hour widget settings migrate automatically
- **Uninstall cleanup**: Windows releases now use NSIS consistently; uninstalling removes autostart residue, and selecting “Delete application data” also removes KimiCodeBar settings and Windows credentials while strictly preserving the Kimi CLI shared `~/.kimi-code`, `KIMI_CODE_HOME`, and session data

## [0.1.0] - 2026-07-31

First public release (Windows).

### Added

- **Tray monitoring**: left-click panel, right-click menu (Console / Refresh / Settings / Quit), hover tooltip with full quota info; tray icon turns red when any window drops below 20% remaining
- **Quota panel**: weekly and 5-hour usage (remaining percentage + progress bar + reset countdown), booster pack balance
- **Monthly usage**: via the web `kimi-auth` cookie (same endpoint as the official console), showing monthly used percentage and Kimi Code share
- **Login**: OAuth Device Flow browser authorization or API Key (switchable in settings); OAuth credentials encrypted with DPAPI, API key and web token stored in Windows Credential Manager
- **Local usage stats**: today / yesterday token consumption, 7-day bar chart, Top 5 models, cache hit rate (today / 7 days / per-day / per-model); resolves and labels `SECONDARY_MODEL` by its full provider-qualified config alias
- **Model trends card**: compare all models in one 7-day chart, switch between token usage and cache hit rate, and hover by day for details
- **Panel card ordering**: move any metric card up or down in Settings; order persists independently of visibility
- **Auto session archiving**: archive by threshold (1 day / 1 week / 1 month), with manual archive and unarchive
- **Skills browser**: view skill definitions under `~/.kimi-code/skills`
- **Update checks**: Kimi Code CLI version detection (`kimi --version` vs. official changelog) and app update checks (GitHub Releases)
- **Settings window**: login method, theme (dark / light / system), language (Chinese / English / system), launch at login, refresh interval, panel card visibility
- **UI experience**: staggered card entrance animations, progress bar / chart grow effects with continuous stripe motion (degraded under `prefers-reduced-motion`), shimmer loading on refresh, scroll-on-demand scrollbar, dark / light themes
