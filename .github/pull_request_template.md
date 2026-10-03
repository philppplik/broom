## What & why

<!-- Short description. Link the issue if there is one: Fixes #123 -->

## Checklist

- [ ] Works in dry run (`-Mode Analyze`) and changes nothing there
- [ ] Uses `Clear-Path` / `Get-BroomFiles` / `Remove-Reg*` / `Test-PathMissing` (no reparse-point following, registry backed up)
- [ ] `./tools/Test-Broom.ps1` passes
- [ ] Tested on: <!-- x64 / ARM64, PowerShell 5.1 / 7 -->
- [ ] `docs/ITEMS.md`, README table and `CHANGELOG.md` updated (if user-visible)
