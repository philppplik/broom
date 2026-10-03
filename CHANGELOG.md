# Changelog

All notable changes to this project are documented here.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [1.0.0] - 2026-10-03

### Added
- Interactive TUI with ASCII logo, main menu, item picker, confirm screen, live progress and summary with a "biggest wins" chart.
- Modes: Quick Sweep (12 items), Deep Clean (28), Nuclear (36), Custom, Analyze (dry run).
- 36 cleaning items across system junk, updates & installers, browsers & apps and registry.
- Auto-detection of Electron / Chromium / WebView2 app caches.
- Restore point before every real run; `.reg` backup of every touched registry key; restore screen.
- Unattended mode: `-Mode`, `-Yes`, `-NoRestorePoint`.
- Native support for Windows 11 x64 and ARM64; WOW64-aware orphan detection.

[Unreleased]: https://github.com/philppplik/broom/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/philppplik/broom/releases/tag/v1.0.0
