# Contributing to Broom

Thanks for helping keep computers tidy! 🧹

## Setup

```bash
git clone https://github.com/philppplik/broom && cd broom
cargo run -- --dry-run          # TUI, nothing is changed
cargo test -- --test-threads=1  # includes real deletion/junction/undo tests in temp folders
cargo clippy && cargo fmt
```

Windows needs the Visual Studio C++ build tools; macOS needs the Xcode command line tools.
Cross-check other platforms without running them: `rustup target add x86_64-unknown-linux-gnu aarch64-apple-darwin`
then `cargo check --target <triple>`.

Look at any screen without a terminal: `cargo run -- --dry-run snapshot --tab tweaks --keys "Right,b"`.

## Ground rules

Broom runs as administrator/root and deletes things. Every contribution must keep these guarantees:

1. **Dry run first.** Everything must respect `util::fs::dry()` and change nothing when it is true. Use the helpers
   (`fs::clear`, `fs::remove_file`, `fs::quarantine`, `reg::set`, `reg::delete_*`) - they already do.
2. **Never follow links.** Walk with `fs::walk_files` / delete with `fs::clear`. No `remove_dir_all` on folders that
   might contain junctions or symlinks.
3. **Registry changes go through `util::reg`** so they are exported to `.reg` first.
4. **Judge "missing" paths with `clean::windows_paths::path_missing`**, never with a bare `exists()`.
5. **No user data.** Documents, Downloads, Pictures, Videos, Music and cloud folders are off limits.
6. **No telemetry, no network** except what the user starts explicitly.
7. **Licenses.** Broom is MIT. Do not paste code, data or text from GPL / CC-BY-SA projects. MIT/Apache sources need an
   entry in `THIRD-PARTY-NOTICES.md`.

## Adding a cleaning item

In `src/clean/<os>.rs` (or `common.rs` if it works everywhere):

```rust
fn zoom_cache() -> Outcome {
    let mut o = Outcome::default();
    for h in util::profiles() {
        o += bfs::clear_all(h.join("AppData/Roaming/Zoom/data/Cache"));
    }
    o
}
// in items():
mk("zoom", "Browsers & apps", "Zoom cache", 2, Risk::Safe, "Rebuilt automatically.", zoom_cache),
```

Choose the tier honestly: **1** Quick = nothing anyone would miss · **2** Deep = rebuilt automatically or a small
convenience lost · **3** Nuclear = removes a way back.

## Adding a tweak

In `src/tweaks/catalog_<os>.rs`. Prefer registry / service / `defaults` / `gsettings` / `sysctl` ops: they get **exact undo**
for free. A PowerShell or command op makes the tweak "Script" or "None" undo - add an `undo` list where possible.

## Before opening a PR

- `cargo fmt --check`, `cargo clippy`, `cargo test -- --test-threads=1`
- Run your change once with `--dry-run` and once for real in a VM or on a test machine
- Regenerate docs if you changed catalogs (see the generator notes at the top of `docs/CLEAN.md` / `docs/TWEAKS.md`)
- Add a line to `CHANGELOG.md` under *Unreleased*

## Releases

Bump `version` in `Cargo.toml`, update `CHANGELOG.md`, push a tag `vX.Y.Z`. The release workflow builds Windows
(x64/ARM64) zips, a universal macOS DMG, Linux tarballs and .deb/.rpm packages, writes `SHA256SUMS.txt` and publishes.
