# Contributing to Broom

Thanks for helping keep Windows tidy! 🧹

## Ground rules

Broom runs as administrator and deletes things. Every contribution must keep these guarantees:

1. **Dry run first.** Every item must work with `$script:DryRun = $true` and change nothing in that mode, including services, processes, registry and files. Use `Clear-Path`, `Remove-BroomFile`, `Remove-RegKey` and `Remove-RegValue`; they already respect dry run.
2. **Never follow reparse points.** Enumerate with `Get-BroomFiles` and delete with `Clear-Path`. Don't use `Remove-Item -Recurse` on folders that might contain junctions.
3. **Registry changes go through `Remove-RegKey` / `Remove-RegValue`.** They export a `.reg` backup first.
4. **Judge paths with `Test-PathMissing`.** Never use a bare `Test-Path` to decide something is orphaned.
5. **No user data.** Documents, Downloads, Pictures, Videos, Music, OneDrive, passwords, cookies and history are off limits.
6. **No network access, no telemetry.**
7. **ASCII only** in `Broom.ps1`. Windows PowerShell 5.1 reads BOM-less files as ANSI.
8. Must run on **Windows PowerShell 5.1 and PowerShell 7**, on **x64 and ARM64**.

## Adding a cleaning item

```powershell
Add-Task -Id myapp -Category 'Browsers & apps' -Name 'MyApp cache' -Tier 2 -Risk Safe `
    -Processes myapp `
    -Description 'One sentence a non-expert understands.' -Action {
    Clear-Path (UserPaths 'AppData\Local\MyApp\Cache')
}
```

- The action returns bytes as `[int64]`. All numeric output is summed, so wrap other calls in `| Out-Null` or `[void]`.
- Set `$script:TaskNote` for a short status such as `"12 entries"`.
- Use `-Delta` when only Windows tools can do the work; size is then measured by the free-space difference.
- Use `-Slow` when the item can take minutes.
- Choose the tier honestly: **1** Quick = nothing anyone would miss, **2** Deep = rebuilt automatically or a small convenience lost, **3** Nuclear = removes a way back.
- Document it in [`docs/ITEMS.md`](docs/ITEMS.md) and the README table.

## Before opening a PR

```powershell
# Syntax + lint (same as CI)
.\tools\Test-Broom.ps1

# Full dry run on your machine
.\Broom.ps1 -Mode Analyze -Yes
```

Check the log in `C:\ProgramData\Broom\Logs` and make sure everything flagged is really gone from disk.

## Commit style

Short imperative subject (`Add Zoom cache item`, `Fix WOW64 path check`), with details in the body if needed. Update `CHANGELOG.md` under *Unreleased*.

## Releases

Maintainers bump `$script:Version` in `Broom.ps1`, update `CHANGELOG.md`, and push a tag `vX.Y.Z`. The release workflow builds the ZIP, checksums and GitHub release.
