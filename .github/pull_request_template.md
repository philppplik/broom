## What & why

<!-- Short description. Link the issue: Fixes #123 -->

## Checklist

- [ ] Respects dry run (`--dry-run` / F2) and changes nothing there
- [ ] Uses the safe helpers (`util::fs`, `util::reg`, `path_missing`) - no link following, registry backed up
- [ ] `cargo fmt --check`, `cargo clippy`, `cargo test -- --test-threads=1` pass
- [ ] Tested on: <!-- Windows x64 / ARM64, macOS, Linux distro -->
- [ ] Docs and `CHANGELOG.md` updated (if user-visible)
- [ ] No code/data/text from GPL or CC-BY-SA projects
