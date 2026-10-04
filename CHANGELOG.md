# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [2.0.0] - 2026-10-04

Broom 2.0 is a rewrite in Rust with a real terminal UI and support for **Windows, macOS and Linux** (x64 and ARM64).

### Added
- **Terminal UI** (Ratatui): Home dashboard, tabs, sub-views, mouse support, search, help overlay, live progress,
  background jobs that never freeze the UI, global dry-run toggle (F2).
- **Clean**: 40 Windows items (all 1.x items plus font cache, stale project build output, Docker/Podman cache,
  AI download leftovers and more package managers), 20 macOS items (caches, logs, Xcode, simulators, Homebrew, Trash…),
  17 Linux items (thumbnails, journal, apt/dnf/pacman caches, orphans, old snaps, unused Flatpak runtimes…).
- **Uninstall**: Win32, MSI and Store apps; macOS `.app` bundles and Homebrew; Linux deb/rpm/pacman/Flatpak/Snap.
  Measured sizes, silent uninstall detection (MSI, Inno Setup, NSIS, Squirrel), leftover scan with confidence levels,
  quarantine with one-key restore, startup manager (Task-Manager-compatible on Windows, LaunchAgents, XDG autostart, systemd).
- **Tweaks**: tweak engine with a journal that restores the exact previous value; winutil's catalog embedded (71 tweaks
  and optional features, presets); 47 own Windows tweaks (privacy, gaming, performance, Explorer, services, network, power);
  22 macOS and 12 Linux tweaks; debloat for preinstalled Store apps with reinstall; DNS manager with real benchmark.
  Undo class (exact / script / none) shown everywhere, one-way changes flagged red.
- **Doctor**: hardware detection, ~40 checks per OS, health score, hardware-aware recommendations with one-key fixes,
  repair toolbox (SFC, DISM, network & Windows Update reset and more), JSON report export.
- **Backups** tab: undo tweaks, re-import registry backups, restore or purge quarantine, credits.
- **CLI**: `clean`, `uninstall`, `tweaks list/apply/undo`, `doctor`, `dns`, all with `--json` and `--dry-run`.
- Headless `snapshot` command that renders the real UI to text/SVG (used for tests and README screenshots).
- Packages: Windows zip (x64, ARM64), macOS universal DMG with Broom.app, Linux tar.gz/.deb/.rpm (x64, ARM64),
  one-line installers with checksum verification.
- CI on Windows x64/ARM64, Linux x64/ARM64 and macOS with unit tests for junction safety, quarantine and exact undo.

### Changed
- Broom 1.x (PowerShell) moved to `legacy/`.


## [1.0.0] - 2026-10-03

### Added
- Interactive TUI with ASCII logo, main menu, item picker, confirm screen, live progress and summary with a "biggest wins" chart.
- Modes: Quick Sweep (12 items), Deep Clean (28), Nuclear (36), Custom, Analyze (dry run).
- 36 cleaning items across system junk, updates & installers, browsers & apps and registry.
- Auto-detection of Electron / Chromium / WebView2 app caches.
- Restore point before every real run; `.reg` backup of every touched registry key; restore screen.
- Unattended mode: `-Mode`, `-Yes`, `-NoRestorePoint`.
- Native support for Windows 11 x64 and ARM64; WOW64-aware orphan detection.

[Unreleased]: https://github.com/philppplik/broom/compare/v2.0.0...HEAD
[2.0.0]: https://github.com/philppplik/broom/compare/v1.0.0...v2.0.0
[1.0.0]: https://github.com/philppplik/broom/releases/tag/v1.0.0
