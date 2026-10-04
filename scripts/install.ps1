# Broom installer for Windows:  irm https://raw.githubusercontent.com/philppplik/broom/main/scripts/install.ps1 | iex
$ErrorActionPreference = 'Stop'
$repo = 'philppplik/broom'
$arch = (Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment').PROCESSOR_ARCHITECTURE
$asset = if ($arch -eq 'ARM64') { 'broom-windows-arm64.zip' } else { 'broom-windows-x64.zip' }
$dest = Join-Path $env:LOCALAPPDATA 'Programs\Broom'
$tmp = Join-Path $env:TEMP ("broom-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $tmp, $dest -Force | Out-Null
Write-Host "Downloading $asset ..."
Invoke-WebRequest "https://github.com/$repo/releases/latest/download/$asset" -OutFile "$tmp\broom.zip" -UseBasicParsing
Invoke-WebRequest "https://github.com/$repo/releases/latest/download/SHA256SUMS.txt" -OutFile "$tmp\sums.txt" -UseBasicParsing
$expected = ((Get-Content "$tmp\sums.txt") | Where-Object { $_ -match " $([regex]::Escape($asset))$" }) -split ' ' | Select-Object -First 1
$actual = (Get-FileHash "$tmp\broom.zip" -Algorithm SHA256).Hash.ToLower()
if ($expected -ne $actual) { throw "Checksum mismatch - download aborted" }
Expand-Archive "$tmp\broom.zip" $dest -Force
Remove-Item $tmp -Recurse -Force
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($userPath -split ';') -notcontains $dest) { [Environment]::SetEnvironmentVariable('Path', "$userPath;$dest", 'User') }
$lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Broom.lnk'
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk); $s.TargetPath = "$dest\broom.exe"; $s.Save()
Write-Host "Installed to $dest (checksum verified). Start menu: Broom. New terminals: broom"
