<#
    ____
   / __ )_________  ____  ____ ___
  / __  / ___/ __ \/ __ \/ __ `__ \
 / /_/ / /  / /_/ / /_/ / / / / / /
/_____/_/   \____/\____/_/ /_/ /_/

  Broom - a thorough junk, leftover and registry cleaner for Windows 11
  Works on x64 (Intel/AMD) and ARM64 (Snapdragon). Windows PowerShell 5.1 or PowerShell 7.

  USAGE
    Broom.cmd                                           interactive TUI (recommended)
    Broom.ps1 -Mode Analyze                             dry run, deletes nothing
    Broom.ps1 -Mode Quick|Deep|Nuclear [-Yes] [-NoRestorePoint]   unattended

  SAFETY NET
    * A System Restore point is created before every real run (if System Protection is on).
    * Every registry key/value is exported to %ProgramData%\Broom\Backups\<timestamp>\*.reg first.
    * Never touches Documents, Downloads, Pictures, Videos, Music or OneDrive.
      On the Desktop only broken .lnk shortcuts are removed.
    * Never follows junctions / symlinks while deleting. Files in use are skipped.
#>
[CmdletBinding()]
param(
    [ValidateSet('Menu', 'Quick', 'Deep', 'Nuclear', 'Analyze')]
    [string]$Mode = 'Menu',
    [switch]$Yes,
    [switch]$NoRestorePoint,
    [switch]$NoElevate   # testing only: run without admin rights (many items will be skipped)
)

Set-StrictMode -Off
$ErrorActionPreference = 'SilentlyContinue'
$ProgressPreference    = 'SilentlyContinue'
try { [Threading.Thread]::CurrentThread.CurrentCulture = [Globalization.CultureInfo]'en-US' } catch {}

$script:Self = $PSCommandPath

#region ---------------------------------------------------------------- Bootstrap
function Get-RelaunchArgs([switch]$Quoted) {
    $file = if ($Quoted) { '"{0}"' -f $script:Self } else { $script:Self }
    $a = @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $file, '-Mode', $Mode)
    if ($Yes)            { $a += '-Yes' }
    if ($NoRestorePoint) { $a += '-NoRestorePoint' }
    if ($NoElevate)      { $a += '-NoElevate' }
    $a
}

# A 32-bit host on a 64-bit OS sees a redirected file system and registry -> relaunch natively.
if ([Environment]::Is64BitOperatingSystem -and -not [Environment]::Is64BitProcess) {
    $native = Join-Path $env:windir 'Sysnative\WindowsPowerShell\v1.0\powershell.exe'
    if (Test-Path -LiteralPath $native) { & $native @(Get-RelaunchArgs); exit $LASTEXITCODE }
}

$script:IsAdmin = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)

if (-not $script:IsAdmin -and -not $NoElevate) {
    try {
        $hostExe = (Get-Process -Id $PID).Path
        Start-Process -FilePath $hostExe -ArgumentList (Get-RelaunchArgs -Quoted) -Verb RunAs -ErrorAction Stop
    } catch {
        Write-Host ''
        Write-Host '  Broom needs administrator rights and elevation was cancelled.' -ForegroundColor Red
        Start-Sleep -Seconds 4
    }
    exit
}
#endregion

#region ---------------------------------------------------------------- Globals
$script:Version      = '1.0.0'
$script:DataDir      = Join-Path $env:ProgramData 'Broom'
$script:LogDir       = Join-Path $script:DataDir 'Logs'
$script:BackupRoot   = Join-Path $script:DataDir 'Backups'
$script:BackupDir    = $null
$script:LogFile      = Join-Path $script:LogDir ('broom-{0}.log' -f (Get-Date -Format 'yyyyMMdd-HHmmss'))
$script:DryRun       = $false
$script:FilesRemoved = 0
$script:RegCount     = 0
$script:TaskNote     = $null
$script:RegBackupIdx = 0
$script:BackedUpKeys = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
$script:Results      = New-Object 'System.Collections.Generic.List[object]'

New-Item -ItemType Directory -Path $script:LogDir -Force | Out-Null

function Get-NativeArch {
    $a = (Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' -Name PROCESSOR_ARCHITECTURE).PROCESSOR_ARCHITECTURE
    if (-not $a) { $a = $env:PROCESSOR_ARCHITECTURE }
    switch ($a) { 'ARM64' { 'ARM64' } 'AMD64' { 'x64' } default { "$a" } }
}

$script:Arch = Get-NativeArch
$cv = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$script:OsName  = ((Get-CimInstance Win32_OperatingSystem).Caption -replace '^Microsoft\s+', '').Trim()
$script:OsBuild = '{0} (build {1}.{2})' -f $cv.DisplayVersion, $cv.CurrentBuild, $cv.UBR

# All real user profiles on this PC (so an admin elevating for a standard user cleans both)
$script:Profiles = @(Get-CimInstance Win32_UserProfile |
    Where-Object { -not $_.Special -and $_.LocalPath -and (Test-Path -LiteralPath $_.LocalPath) } |
    ForEach-Object { $_.LocalPath })
if (-not $script:Profiles) { $script:Profiles = @($env:USERPROFILE) }

# Paths that must never be emptied themselves
$script:Protected = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
foreach ($p in @($env:SystemDrive, $env:windir, "$env:windir\System32", "$env:windir\SysWOW64", "$env:windir\WinSxS",
                 $env:ProgramFiles, ${env:ProgramFiles(x86)}, "$env:SystemDrive\Program Files (Arm)",
                 $env:ProgramData, "$env:SystemDrive\Users", $env:PUBLIC)) {
    if ($p) { [void]$script:Protected.Add($p.TrimEnd('\')) }
}
foreach ($pr in $script:Profiles) {
    foreach ($s in @('', '\AppData', '\AppData\Local', '\AppData\Roaming', '\AppData\LocalLow', '\Desktop')) {
        [void]$script:Protected.Add(($pr + $s).TrimEnd('\'))
    }
}
#endregion

#region ---------------------------------------------------------------- Core helpers
function Write-Log([string]$Message) {
    try { Add-Content -LiteralPath $script:LogFile -Value ('[{0:HH:mm:ss}] {1}' -f (Get-Date), $Message) -Encoding UTF8 } catch {}
}

function Format-Size([double]$Bytes) {
    if ($Bytes -ge 1TB) { return '{0:N2} TB' -f ($Bytes / 1TB) }
    if ($Bytes -ge 1GB) { return '{0:N2} GB' -f ($Bytes / 1GB) }
    if ($Bytes -ge 1MB) { return '{0:N1} MB' -f ($Bytes / 1MB) }
    if ($Bytes -ge 1KB) { return '{0:N0} KB' -f ($Bytes / 1KB) }
    '{0:N0} B' -f $Bytes
}

function Get-FreeSpace([string]$Drive = $env:SystemDrive) {
    try { [int64](New-Object IO.DriveInfo($Drive)).AvailableFreeSpace } catch { [int64]0 }
}

# Expand the same relative path(s) for every user profile
function UserPaths([string[]]$Relative) {
    foreach ($pr in $script:Profiles) { foreach ($r in $Relative) { Join-Path $pr $r } }
}

# Expand env vars + wildcards; never returns reparse points (junctions/symlinks)
function Resolve-BroomPath([string]$Path) {
    $p = [Environment]::ExpandEnvironmentVariables($Path)
    if ($p -notmatch '[\*\?]') {
        if ([IO.File]::Exists($p) -or [IO.Directory]::Exists($p)) { $p }
        return
    }
    Get-Item -Path $p -Force -ErrorAction SilentlyContinue |
        Where-Object { -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) } |
        ForEach-Object { $_.FullName }
}

function Test-SafePath([string]$Path) {
    try { $full = [IO.Path]::GetFullPath($Path).TrimEnd('\') } catch { return $false }
    if ($full.Length -lt 4) { return $false }
    if ($script:Protected.Contains($full)) { return $false }
    if ($full -notmatch '\\AppData\\' -and
        $full -match '\\(Documents|Downloads|Pictures|Videos|Music|OneDrive[^\\]*|Favorites|Contacts|Saved Games|Searches|Links)(\\|$)') { return $false }
    $true
}

# Enumerate files below a root without ever descending into reparse points
function Get-BroomFiles {
    param([string]$Root, [string[]]$Filter = @('*'), [string[]]$Exclude = @(), [int]$OlderThanDays = 0, [switch]$NoRecurse)
    $rp  = [IO.FileAttributes]::ReparsePoint
    $cut = (Get-Date).AddDays(-$OlderThanDays)
    $any = ($Filter.Count -eq 1 -and $Filter[0] -eq '*')
    if ([IO.File]::Exists($Root)) {
        $fi = New-Object IO.FileInfo($Root)
        if (-not ($fi.Attributes -band $rp)) { $fi }
        return
    }
    if (-not [IO.Directory]::Exists($Root)) { return }
    $stack = New-Object 'System.Collections.Generic.Stack[string]'
    $stack.Push($Root)
    while ($stack.Count -gt 0) {
        $dir = $stack.Pop()
        try { $entries = (New-Object IO.DirectoryInfo($dir)).GetFileSystemInfos() } catch { continue }
        foreach ($e in $entries) {
            if ($e.Attributes -band $rp) { continue }
            if ($e -is [IO.DirectoryInfo]) { if (-not $NoRecurse) { $stack.Push($e.FullName) }; continue }
            if ($OlderThanDays -gt 0 -and $e.LastWriteTime -gt $cut) { continue }
            if ($Exclude.Count -gt 0 -and ($Exclude -contains $e.Name)) { continue }
            if ($any) { $e; continue }
            foreach ($f in $Filter) { if ($e.Name -like $f) { $e; break } }
        }
    }
}

function Remove-BroomFile($File) {
    [int64]$len = 0
    try { $len = $File.Length } catch {}
    if ($script:DryRun) { $script:FilesRemoved++; return $len }
    try {
        if ($File.Attributes -band ([IO.FileAttributes]::ReadOnly -bor [IO.FileAttributes]::Hidden -bor [IO.FileAttributes]::System)) {
            $File.Attributes = [IO.FileAttributes]::Normal
        }
        $File.Delete()
        $script:FilesRemoved++
        return $len
    } catch { return [int64]0 }
}

# Delete empty sub-folders (deepest first), never the root, never through reparse points
function Remove-EmptyDirs([string]$Root) {
    if ($script:DryRun -or -not [IO.Directory]::Exists($Root)) { return }
    $rp    = [IO.FileAttributes]::ReparsePoint
    $all   = New-Object 'System.Collections.Generic.List[string]'
    $stack = New-Object 'System.Collections.Generic.Stack[string]'
    $stack.Push($Root)
    while ($stack.Count -gt 0) {
        $d = $stack.Pop()
        try { $subs = (New-Object IO.DirectoryInfo($d)).GetDirectories() } catch { continue }
        foreach ($s in $subs) { if ($s.Attributes -band $rp) { continue }; $all.Add($s.FullName); $stack.Push($s.FullName) }
    }
    for ($i = $all.Count - 1; $i -ge 0; $i--) { try { [IO.Directory]::Delete($all[$i], $false) } catch {} }
}

# The workhorse: empties folders / deletes files, returns bytes freed (or would-be-freed in dry run)
function Clear-Path {
    param(
        [Parameter(Mandatory, Position = 0)][string[]]$Path,
        [string[]]$Filter = @('*'),
        [string[]]$Exclude = @(),
        [int]$OlderThanDays = 0,
        [switch]$RemoveRoot,
        [switch]$NoRecurse
    )
    [int64]$freed = 0
    foreach ($raw in $Path) {
        foreach ($p in @(Resolve-BroomPath $raw)) {
            if (-not (Test-SafePath $p)) { Write-Log "SKIP protected path: $p"; continue }
            if ([IO.File]::Exists($p)) {
                $fi = New-Object IO.FileInfo($p)
                if (-not ($fi.Attributes -band [IO.FileAttributes]::ReparsePoint)) { $freed += Remove-BroomFile $fi }
                continue
            }
            if ((New-Object IO.DirectoryInfo($p)).Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
            foreach ($f in @(Get-BroomFiles -Root $p -Filter $Filter -Exclude $Exclude -OlderThanDays $OlderThanDays -NoRecurse:$NoRecurse)) {
                $freed += Remove-BroomFile $f
            }
            if (-not $script:DryRun -and -not $NoRecurse) {
                Remove-EmptyDirs $p
                if ($RemoveRoot) { try { [IO.Directory]::Delete($p, $false) } catch {} }
            }
        }
    }
    $freed
}

function Get-PathSize([string[]]$Path) {
    [int64]$sum = 0
    foreach ($raw in $Path) {
        foreach ($p in @(Resolve-BroomPath $raw)) {
            foreach ($f in @(Get-BroomFiles -Root $p)) { try { $sum += $f.Length } catch {} }
        }
    }
    $sum
}

function Invoke-WithServicesStopped([string[]]$Names, [scriptblock]$Body) {
    $stopped = @()
    if (-not $script:DryRun) {
        foreach ($n in $Names) {
            $s = Get-Service -Name $n
            if ($s -and $s.Status -eq 'Running') { Stop-Service -Name $n -Force -NoWait; $stopped += $n }
        }
        foreach ($n in $stopped) { try { (Get-Service -Name $n).WaitForStatus('Stopped', [TimeSpan]::FromSeconds(30)) } catch {} }
    }
    try { & $Body } finally { foreach ($n in $stopped) { Start-Service -Name $n } }
}

# For folders owned by TrustedInstaller (e.g. leftovers of Windows.old). rd /s never follows junctions.
function Remove-ProtectedDir([string]$Dir) {
    if ($script:DryRun -or -not [IO.Directory]::Exists($Dir) -or -not (Test-SafePath $Dir)) { return }
    & icacls.exe "$Dir" /setowner '*S-1-5-32-544' /T /C /L /Q 2>&1 | Out-Null
    & icacls.exe "$Dir" /grant '*S-1-5-32-544:F' /T /C /L /Q 2>&1 | Out-Null
    & cmd.exe /c "rd /s /q `"$Dir`"" 2>&1 | Out-Null
}
#endregion

#region ---------------------------------------------------------------- Registry helpers
function Get-UserSids {
    @(Get-ChildItem -LiteralPath 'Registry::HKEY_USERS' |
        Where-Object { $_.PSChildName -match '^S-1-5-21-[\d-]+$' } |
        ForEach-Object { $_.PSChildName })
}

function Open-RegKey([string]$Name, [switch]$Writable) {
    $parts = $Name.Split('\', 2)
    $hive = switch ($parts[0]) {
        'HKEY_LOCAL_MACHINE' { [Microsoft.Win32.Registry]::LocalMachine }
        'HKEY_CURRENT_USER'  { [Microsoft.Win32.Registry]::CurrentUser }
        'HKEY_USERS'         { [Microsoft.Win32.Registry]::Users }
        'HKEY_CLASSES_ROOT'  { [Microsoft.Win32.Registry]::ClassesRoot }
    }
    if (-not $hive) { return $null }
    if ($parts.Count -lt 2) { return $hive }
    try { $hive.OpenSubKey($parts[1], [bool]$Writable) } catch { $null }
}

function Backup-RegKey([string]$KeyName) {
    if ($script:BackedUpKeys.Contains($KeyName)) { return }
    if (-not $script:BackupDir) {
        $script:BackupDir = Join-Path $script:BackupRoot (Get-Date -Format 'yyyyMMdd-HHmmss')
    }
    New-Item -ItemType Directory -Path $script:BackupDir -Force | Out-Null
    $script:RegBackupIdx++
    $file = Join-Path $script:BackupDir ('{0:D4}.reg' -f $script:RegBackupIdx)
    & reg.exe export "$KeyName" "$file" /y 2>&1 | Out-Null
    [void]$script:BackedUpKeys.Add($KeyName)
}

# $Name like 'HKEY_LOCAL_MACHINE\SOFTWARE\...'
function Remove-RegKey([string]$Name, [string]$Why = '') {
    $script:RegCount++
    Write-Log ("REG key   {0}  {1}" -f $Name, $Why)
    if ($script:DryRun) { return }
    Backup-RegKey $Name
    Remove-Item -LiteralPath "Registry::$Name" -Recurse -Force
}

function Remove-RegValue([string]$KeyName, [string]$ValueName, [string]$Why = '') {
    $script:RegCount++
    Write-Log ("REG value {0} :: {1}  {2}" -f $KeyName, $ValueName, $Why)
    if ($script:DryRun) { return }
    Backup-RegKey $KeyName
    $k = Open-RegKey $KeyName -Writable
    if ($k) { try { $k.DeleteValue($ValueName, $false) } catch {}; $k.Close() }
}

# True only if the path is absolute, on a drive that exists, and really gone
# (also checks System32 <-> SysWOW64 / SysNative / SysArm32 redirection variants).
function Test-PathMissing([string]$Path) {
    if ([string]::IsNullOrWhiteSpace($Path)) { return $false }
    $p = [Environment]::ExpandEnvironmentVariables($Path.Trim().Trim('"').Trim())
    if ($p -match '%' -or $p -notmatch '^[A-Za-z]:\\' -or $p -match '\\WindowsApps\\') { return $false }
    try {
        if (-not [IO.Directory]::Exists($p.Substring(0, 3))) { return $false }
        if ([IO.File]::Exists($p) -or [IO.Directory]::Exists($p)) { return $false }
        # 32-bit registrations say "System32"/"Program Files" but mean SysWOW64/"Program Files (x86)"
        $alts = @()
        foreach ($s in @('System32', 'SysWOW64', 'SysNative', 'SysArm32')) {
            $alts += $p -replace '\\(System32|SysWOW64|SysNative|SysArm32)\\', "\$s\"
        }
        foreach ($s in @('Program Files', 'Program Files (x86)', 'Program Files (Arm)')) {
            $alts += $p -replace '\\Program Files( \(x86\)| \(Arm\))?\\', "\$s\"
        }
        foreach ($q in $alts) { if ([IO.File]::Exists($q) -or [IO.Directory]::Exists($q)) { return $false } }
        # Exists() also says "no" when access is denied. Only trust it if the nearest
        # existing parent folder can actually be listed.
        $parent = [IO.Path]::GetDirectoryName($p.TrimEnd('\'))
        while ($parent -and -not [IO.Directory]::Exists($parent)) { $parent = [IO.Path]::GetDirectoryName($parent) }
        if (-not $parent) { return $false }
        [void][IO.Directory]::GetFileSystemEntries($parent)
    } catch { return $false }
    $true
}

# Pull the executable out of a command line like  "C:\x\y.exe" /uninstall  or  C:\x y\z.exe -arg
function Get-ExeFromCommand([string]$Cmd) {
    if ([string]::IsNullOrWhiteSpace($Cmd)) { return $null }
    $c = [Environment]::ExpandEnvironmentVariables($Cmd).Trim()
    if ($c.StartsWith('"')) {
        $end = $c.IndexOf('"', 1)
        if ($end -gt 1) { return $c.Substring(1, $end - 1) }
        return $null
    }
    $m = [regex]::Match($c, '^(?<p>[A-Za-z]:\\.*?\.(exe|com|bat|cmd|msc|cpl|lnk))(\s|,|$)', 'IgnoreCase')
    if ($m.Success) { return $m.Groups['p'].Value }
    $null
}
#endregion

#region ---------------------------------------------------------------- Task registry
$script:Tasks = New-Object 'System.Collections.Generic.List[object]'
$script:Categories = @('System junk', 'Updates & installers', 'Browsers & apps', 'Registry')

function Add-Task {
    param(
        [string]$Id, [string]$Category, [string]$Name,
        [int]$Tier,                       # 1 = Quick, 2 = Deep, 3 = Nuclear
        [ValidateSet('Safe', 'Moderate', 'Aggressive')][string]$Risk,
        [string]$Description,
        [string[]]$Processes = @(),       # apps that lock these files
        [switch]$Delta,                   # measure by free-space difference instead of counted bytes
        [switch]$Slow,
        [scriptblock]$Action
    )
    $script:Tasks.Add([pscustomobject]@{
        Id = $Id; Category = $Category; Name = $Name; Tier = $Tier; Risk = $Risk; Description = $Description
        Processes = $Processes; Delta = [bool]$Delta; Slow = [bool]$Slow; Action = $Action; Selected = $false
    })
}

# Runs Windows' own Disk Cleanup silently with the given handlers.
# 'DownloadsFolder' is ALWAYS excluded - it would wipe your Downloads.
function Invoke-CleanMgr([string[]]$Only = @(), [string[]]$Except = @()) {
    if ($script:DryRun) { return }
    $exe = Join-Path $env:windir 'System32\cleanmgr.exe'
    if (-not (Test-Path -LiteralPath $exe)) { return }
    $Except += 'DownloadsFolder'
    $root = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\VolumeCaches'
    $flag = 'StateFlags0777'
    $set  = @()
    foreach ($k in @(Get-ChildItem -Path $root)) {
        $n = $k.PSChildName
        if ($Except -contains $n) { continue }
        if ($Only.Count -gt 0 -and ($Only -notcontains $n)) { continue }
        New-ItemProperty -LiteralPath $k.PSPath -Name $flag -Value 2 -PropertyType DWord -Force | Out-Null
        $set += $k.PSPath
    }
    # Make sure no stale flag from an older run selects Downloads
    Remove-ItemProperty -LiteralPath "$root\DownloadsFolder" -Name $flag -Force
    if ($set.Count -gt 0) {
        $p = Start-Process -FilePath $exe -ArgumentList '/sagerun:777' -PassThru -WindowStyle Minimized
        if ($p) { [void]$p.WaitForExit(45 * 60 * 1000) }
    }
    foreach ($s in $set) { Remove-ItemProperty -LiteralPath $s -Name $flag -Force }
}
#endregion

#region ---------------------------------------------------------------- Tasks: System junk
$C = 'System junk'

Add-Task -Id usertemp -Category $C -Name 'User temp files' -Tier 1 -Risk Safe `
    -Description 'Everything in AppData\Local\Temp of every user profile. Files that are in use are skipped.' -Action {
    Clear-Path (UserPaths 'AppData\Local\Temp')
}

Add-Task -Id wintemp -Category $C -Name 'Windows temp files' -Tier 1 -Risk Safe `
    -Description 'C:\Windows\Temp - leftovers from installers, updates and services.' -Action {
    Clear-Path "$env:windir\Temp"
}

Add-Task -Id wer -Category $C -Name 'Error reports (WER)' -Tier 1 -Risk Safe `
    -Description 'Windows Error Reporting archives and queues that were already sent or never will be.' -Action {
    Clear-Path @("$env:ProgramData\Microsoft\Windows\WER\ReportArchive",
                 "$env:ProgramData\Microsoft\Windows\WER\ReportQueue",
                 "$env:ProgramData\Microsoft\Windows\WER\Temp")
    Clear-Path (UserPaths 'AppData\Local\Microsoft\Windows\WER')
}

Add-Task -Id dumps -Category $C -Name 'Crash & memory dumps' -Tier 1 -Risk Safe `
    -Description 'MEMORY.DMP, minidumps, live kernel reports and app crash dumps. Only needed for debugging crashes.' -Action {
    Clear-Path @("$env:windir\MEMORY.DMP", "$env:windir\Minidump", "$env:windir\LiveKernelReports")
    Clear-Path (UserPaths 'AppData\Local\CrashDumps')
}

Add-Task -Id logs -Category $C -Name 'Windows log files' -Tier 1 -Risk Safe `
    -Description 'CBS, DISM, setup, upgrade and update logs. Active logs are skipped automatically.' -Action {
    Clear-Path @("$env:windir\Logs\CBS", "$env:windir\Logs\DISM", "$env:windir\Logs\MoSetup",
                 "$env:windir\Logs\WindowsUpdate", "$env:windir\Logs\waasmedic", "$env:windir\Logs\SIH",
                 "$env:windir\Logs\NetSetup", "$env:windir\System32\LogFiles\setupcln",
                 "$env:windir\SoftwareDistribution\DataStore\Logs")
    Clear-Path @("$env:windir\Panther", "$env:windir\debug") -Filter '*.log', '*.etl', '*.txt'
    Clear-Path $env:windir -Filter '*.log' -NoRecurse
}

Add-Task -Id recycle -Category $C -Name 'Recycle Bin (all drives, all users)' -Tier 1 -Risk Moderate `
    -Description 'Empties the Recycle Bin on every fixed drive. Deleted files can no longer be restored.' -Action {
    foreach ($d in [IO.DriveInfo]::GetDrives()) {
        if ($d.DriveType -ne 'Fixed' -or -not $d.IsReady) { continue }
        $rb = Join-Path $d.RootDirectory.FullName '$Recycle.Bin'
        if (-not [IO.Directory]::Exists($rb)) { continue }
        foreach ($sid in @(Get-ChildItem -LiteralPath $rb -Force -Directory)) {
            Clear-Path $sid.FullName -Exclude 'desktop.ini'
        }
    }
    if (-not $script:DryRun) { Clear-RecycleBin -Force -ErrorAction SilentlyContinue }
}

Add-Task -Id thumbs -Category $C -Name 'Thumbnail & icon cache' -Tier 2 -Risk Moderate `
    -Description 'Rebuilt automatically. The taskbar/desktop (Explorer) restarts for a second.' -Action {
    $dirs = @(UserPaths 'AppData\Local\Microsoft\Windows\Explorer')
    $pats = @('thumbcache_*.db', 'iconcache_*.db')
    if ($script:DryRun) {
        Clear-Path $dirs -Filter $pats -NoRecurse
        Clear-Path (UserPaths 'AppData\Local\IconCache.db')
        return
    }
    Stop-Process -Name explorer -Force
    Start-Sleep -Milliseconds 600
    Clear-Path $dirs -Filter $pats -NoRecurse
    Clear-Path (UserPaths 'AppData\Local\IconCache.db')
    Start-Sleep -Seconds 3
    if (-not (Get-Process -Name explorer)) { Start-Process -FilePath "$env:windir\explorer.exe" }
}

Add-Task -Id netcache -Category $C -Name 'DNS, ARP & NetBIOS caches' -Tier 1 -Risk Safe `
    -Description 'Flushes stale name-resolution caches. Fixes some "site not loading" issues, frees no disk space.' -Action {
    if ($script:DryRun) { $script:TaskNote = 'would flush'; return }
    Clear-DnsClientCache
    & ipconfig.exe /flushdns 2>&1 | Out-Null
    & arp.exe -d '*' 2>&1 | Out-Null
    & nbtstat.exe -R 2>&1 | Out-Null
    $script:TaskNote = 'flushed'
}

Add-Task -Id prefetch -Category $C -Name 'Prefetch data' -Tier 3 -Risk Moderate `
    -Description 'App launch traces. Rebuilt automatically, but apps start a bit slower the first time.' -Action {
    Clear-Path "$env:windir\Prefetch" -Filter '*.pf' -NoRecurse
}
#endregion

#region ---------------------------------------------------------------- Tasks: Updates & installers
$C = 'Updates & installers'

Add-Task -Id wucache -Category $C -Name 'Windows Update download cache' -Tier 1 -Risk Safe `
    -Description 'Already-installed update packages in SoftwareDistribution\Download. Update services are paused briefly.' -Action {
    Invoke-WithServicesStopped @('wuauserv', 'bits') { Clear-Path "$env:windir\SoftwareDistribution\Download" }
}

Add-Task -Id dopt -Category $C -Name 'Delivery Optimization cache' -Tier 1 -Risk Safe -Delta `
    -Description 'Update pieces Windows keeps to share with other PCs.' -Action {
    $dir = "$env:windir\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache"
    if ($script:DryRun) { return (Get-PathSize $dir) }
    if (Get-Command Delete-DeliveryOptimizationCache) { Delete-DeliveryOptimizationCache -Force | Out-Null }
    else { Invoke-WithServicesStopped @('dosvc') { Clear-Path $dir } | Out-Null }
}

Add-Task -Id upgrade -Category $C -Name 'Old Windows installations (Windows.old...)' -Tier 2 -Risk Moderate -Delta -Slow `
    -Description 'Windows.old, $Windows.~BT/~WS, $WinREAgent, ESD files. You lose the option to roll back to the previous Windows version.' -Action {
    $dirs = @('Windows.old', '$Windows.~BT', '$Windows.~WS', '$WinREAgent', '$GetCurrent', '$SysReset', 'ESD\Windows', 'ESD\Download') |
        ForEach-Object { Join-Path "$env:SystemDrive\" $_ }
    $present = @($dirs | Where-Object { [IO.Directory]::Exists($_) })
    if ($present.Count -eq 0) { $script:TaskNote = 'nothing found'; return }
    if ($script:DryRun) { return (Get-PathSize $present) }
    Invoke-CleanMgr -Only @('Previous Installations', 'Temporary Setup Files', 'Windows Upgrade Log Files',
                            'Windows ESD installation files', 'Setup Log Files')
    foreach ($d in $present) { Remove-ProtectedDir $d }
}

Add-Task -Id cleanmgr -Category $C -Name 'Windows Disk Cleanup (every category)' -Tier 2 -Risk Moderate -Delta -Slow `
    -Description 'Runs the built-in Disk Cleanup silently with all categories (update cleanup, Defender, setup files...). Downloads is always excluded.' -Action {
    if ($script:DryRun) { $script:TaskNote = 'measured on a real run'; return }
    Invoke-CleanMgr
}

Add-Task -Id winsxs -Category $C -Name 'Component store cleanup (WinSxS /ResetBase)' -Tier 2 -Risk Moderate -Delta -Slow `
    -Description 'Removes superseded system components via DISM. Takes 5-20 min. Installed updates can no longer be uninstalled.' -Action {
    if ($script:DryRun) { $script:TaskNote = 'measured on a real run'; return }
    & dism.exe /Online /English /Cleanup-Image /StartComponentCleanup /ResetBase 2>&1 | Out-Null
    if ($LASTEXITCODE -ne 0) { $script:TaskNote = "DISM code $LASTEXITCODE (restart pending?)" }
}

Add-Task -Id leftovers -Category $C -Name 'Installer & updater leftovers' -Tier 2 -Risk Moderate `
    -Description 'C:\AMD, C:\NVIDIA driver extracts, Edge/Google updater downloads, MSI patch cache, MSOCache, old Squirrel app packages (Discord, Slack...).' -Action {
    Clear-Path @("$env:SystemDrive\AMD", "$env:SystemDrive\NVIDIA", "$env:SystemDrive\MSOCache") -RemoveRoot
    Clear-Path @("$env:SystemDrive\Intel\Logs",
                 "$env:ProgramData\NVIDIA Corporation\Downloader",
                 "${env:ProgramFiles(x86)}\Microsoft\EdgeUpdate\Download",
                 "${env:ProgramFiles(x86)}\Google\Update\Download",
                 "$env:ProgramFiles\Google\Update\Download",
                 (Join-Path $env:windir 'Installer\$PatchCache$'))
    Clear-Path (UserPaths 'AppData\Local\Downloaded Installations', 'AppData\Local\Microsoft\EdgeUpdate\Download')
    # Squirrel-based apps keep every old full installer package; keep only the newest one
    foreach ($pr in $script:Profiles) {
        foreach ($app in @(Get-ChildItem -LiteralPath (Join-Path $pr 'AppData\Local') -Directory -Force)) {
            if (-not [IO.File]::Exists((Join-Path $app.FullName 'Update.exe'))) { continue }
            $pk = @(Get-BroomFiles -Root (Join-Path $app.FullName 'packages') -Filter '*.nupkg' -NoRecurse |
                    Sort-Object LastWriteTime -Descending)
            if ($pk.Count -gt 1) { foreach ($f in $pk[1..($pk.Count - 1)]) { Remove-BroomFile $f } }
        }
    }
}

Add-Task -Id msiorphans -Category $C -Name 'Orphaned Windows Installer packages' -Tier 3 -Risk Aggressive `
    -Description 'Unreferenced .msi/.msp files in C:\Windows\Installer, cross-checked against Windows Installer AND the registry.' -Action {
    $used = New-Object 'System.Collections.Generic.HashSet[string]' ([StringComparer]::OrdinalIgnoreCase)
    $ud = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\UserData'
    foreach ($sidKey in @(Get-ChildItem -Path $ud)) {
        foreach ($p in @(Get-ChildItem -LiteralPath (Join-Path $sidKey.PSPath 'Products'))) {
            $lp = (Get-ItemProperty -LiteralPath (Join-Path $p.PSPath 'InstallProperties')).LocalPackage
            if ($lp) { [void]$used.Add([IO.Path]::GetFileName($lp)) }
        }
        foreach ($p in @(Get-ChildItem -LiteralPath (Join-Path $sidKey.PSPath 'Patches'))) {
            $lp = (Get-ItemProperty -LiteralPath $p.PSPath).LocalPackage
            if ($lp) { [void]$used.Add([IO.Path]::GetFileName($lp)) }
        }
    }
    $apiOk = $false
    try {
        $wi = New-Object -ComObject WindowsInstaller.Installer
        $t  = $wi.GetType()
        foreach ($prod in $t.InvokeMember('ProductsEx', 'GetProperty', $null, $wi, @('', 's-1-1-0', 7))) {
            try {
                $lp = $prod.GetType().InvokeMember('InstallProperty', 'GetProperty', $null, $prod, @('LocalPackage'))
                if ($lp) { [void]$used.Add([IO.Path]::GetFileName($lp)) }
            } catch {}
        }
        $apiOk = $true
        foreach ($pt in $t.InvokeMember('PatchesEx', 'GetProperty', $null, $wi, @('', 's-1-1-0', 7, 15))) {
            try {
                $lp = $pt.GetType().InvokeMember('PatchProperty', 'GetProperty', $null, $pt, @('LocalPackage'))
                if ($lp) { [void]$used.Add([IO.Path]::GetFileName($lp)) }
            } catch {}
        }
    } catch {}
    if (-not $apiOk -or $used.Count -lt 3) { $script:TaskNote = 'could not verify - skipped for safety'; return }
    $n = 0
    foreach ($f in @(Get-BroomFiles -Root (Join-Path $env:windir 'Installer') -Filter '*.msi', '*.msp' -NoRecurse)) {
        if ($used.Contains($f.Name)) { continue }
        Write-Log "Orphaned installer package: $($f.FullName)"
        $n++
        Remove-BroomFile $f
    }
    $script:TaskNote = "$n packages"
}

Add-Task -Id olddrivers -Category $C -Name 'Old driver versions (DriverStore)' -Tier 3 -Risk Aggressive -Slow `
    -Description 'Removes superseded third-party driver packages and keeps the newest of each. Drivers in use are never removed.' -Action {
    $drv = @(Get-WindowsDriver -Online | Where-Object { $_.Driver -like 'oem*.inf' -and $_.OriginalFileName })
    $n = 0
    $groups = $drv | Group-Object { '{0}|{1}|{2}' -f [IO.Path]::GetFileName($_.OriginalFileName), $_.ProviderName, $_.ClassName }
    foreach ($g in $groups) {
        if ($g.Count -lt 2) { continue }
        $sorted = @($g.Group | Sort-Object -Property @{ Expression = { try { [version]$_.Version } catch { [version]'0.0' } } }, Date -Descending)
        foreach ($old in $sorted[1..($sorted.Count - 1)]) {
            $size = Get-PathSize (Split-Path -Parent $old.OriginalFileName)
            if ($script:DryRun) { $n++; $size; continue }
            & pnputil.exe /delete-driver $old.Driver 2>&1 | Out-Null
            if ($LASTEXITCODE -eq 0) {
                $n++
                Write-Log "Removed driver $($old.Driver) $($old.ProviderName) $($old.ClassName) $($old.Version)"
                $size
            }
        }
    }
    $script:TaskNote = "$n packages"
}

Add-Task -Id shadows -Category $C -Name 'Old restore points & shadow copies' -Tier 3 -Risk Aggressive -Delta `
    -Description 'Deletes every restore point except the newest one per drive (the one Broom creates before cleaning).' -Action {
    $old = @()
    foreach ($g in (@(Get-CimInstance Win32_ShadowCopy) | Group-Object VolumeName)) {
        $s = @($g.Group | Sort-Object InstallDate -Descending)
        if ($s.Count -gt 1) { $old += $s[1..($s.Count - 1)] }
    }
    $script:TaskNote = "$($old.Count) old snapshots"
    if ($script:DryRun -or $old.Count -eq 0) { return }
    foreach ($o in $old) { Remove-CimInstance -InputObject $o }
}

Add-Task -Id hibernate -Category $C -Name 'Hibernation file (hiberfil.sys)' -Tier 3 -Risk Aggressive -Delta `
    -Description 'Turns hibernation off and deletes hiberfil.sys (often 5-30 GB). Also disables Fast Startup. Re-enable: powercfg /h on' -Action {
    $fi = Get-Item -LiteralPath (Join-Path "$env:SystemDrive\" 'hiberfil.sys') -Force
    if (-not $fi) { $script:TaskNote = 'already off'; return }
    if ($script:DryRun) { return [int64]$fi.Length }
    & powercfg.exe /hibernate off 2>&1 | Out-Null
}

Add-Task -Id reserved -Category $C -Name 'Reserved storage' -Tier 3 -Risk Aggressive -Delta `
    -Description 'Space Windows reserves for updates (~7 GB). Updates may need more free space afterwards. Fully freed after a restart.' -Action {
    if ($script:DryRun) { $script:TaskNote = 'up to ~7 GB after restart'; return }
    & dism.exe /Online /English /Set-ReservedStorageState /State:Disabled 2>&1 | Out-Null
    $script:TaskNote = if ($LASTEXITCODE -eq 0) { 'disabled (freed after restart)' } else { 'not possible right now (update pending?)' }
}

Add-Task -Id eventlogs -Category $C -Name 'Event logs (all channels)' -Tier 3 -Risk Aggressive -Delta `
    -Description 'Clears every Windows event log. Removes history you might need for troubleshooting.' -Action {
    $logs = @(& wevtutil.exe el)
    if ($script:DryRun) {
        $script:TaskNote = "$($logs.Count) logs"
        [int64]$est = 0
        foreach ($f in @(Get-BroomFiles -Root "$env:windir\System32\winevt\Logs" -Filter '*.evtx')) { $est += [Math]::Max(0, $f.Length - 69632) }
        return $est
    }
    foreach ($l in $logs) { & wevtutil.exe cl "$l" 2>&1 | Out-Null }
    $script:TaskNote = "$($logs.Count) logs cleared"
}
#endregion

#region ---------------------------------------------------------------- Tasks: Browsers & apps
$C = 'Browsers & apps'

$script:ChromiumRootDirs    = @('ShaderCache', 'GrShaderCache', 'GraphiteDawnCache', 'component_crx_cache', 'Crashpad\reports', 'BrowserMetrics')
$script:ChromiumProfileDirs = @('Cache', 'Code Cache', 'GPUCache', 'DawnCache', 'DawnGraphiteCache', 'DawnWebGPUCache',
                                'Service Worker\CacheStorage', 'Service Worker\ScriptCache', 'Application Cache', 'Media Cache')
$script:BrowserRoots = @(
    'AppData\Local\Microsoft\Edge\User Data', 'AppData\Local\Microsoft\Edge Beta\User Data', 'AppData\Local\Microsoft\Edge Dev\User Data',
    'AppData\Local\Google\Chrome\User Data', 'AppData\Local\Google\Chrome Beta\User Data',
    'AppData\Local\BraveSoftware\Brave-Browser\User Data', 'AppData\Local\Vivaldi\User Data', 'AppData\Local\Chromium\User Data',
    'AppData\Roaming\Opera Software\Opera Stable', 'AppData\Roaming\Opera Software\Opera GX Stable',
    'AppData\Local\Opera Software\Opera Stable', 'AppData\Local\Opera Software\Opera GX Stable')
$script:BrowserPattern = '\\(Microsoft\\Edge[^\\]*|Google\\Chrome[^\\]*|BraveSoftware|Vivaldi|Chromium|Opera Software)\\'

# Empties only the cache folders of a Chromium "User Data" / Electron / WebView2 folder
function Clear-ChromiumData([string]$Root) {
    if (-not [IO.Directory]::Exists($Root)) { return }
    foreach ($d in $script:ChromiumRootDirs) { Clear-Path (Join-Path $Root $d) }
    $profiles = @($Root) + @(Get-ChildItem -LiteralPath $Root -Directory -Force |
        Where-Object { $_.Name -eq 'Default' -or $_.Name -like 'Profile *' -or $_.Name -eq 'Guest Profile' } |
        ForEach-Object { $_.FullName })
    foreach ($p in $profiles) { foreach ($d in $script:ChromiumProfileDirs) { Clear-Path (Join-Path $p $d) } }
}

# Finds folders that look like Chromium/Electron data roots (contain GPUCache / Code Cache / Default\Cache)
function Find-ChromiumRoots([string]$Base, [int]$MaxDepth) {
    $rp = [IO.FileAttributes]::ReparsePoint
    $queue = New-Object 'System.Collections.Generic.Queue[object]'
    $queue.Enqueue(@($Base, 0))
    while ($queue.Count -gt 0) {
        $item = $queue.Dequeue()
        try { $subs = (New-Object IO.DirectoryInfo($item[0])).GetDirectories() } catch { continue }
        foreach ($s in $subs) {
            if ($s.Attributes -band $rp) { continue }
            $f = $s.FullName
            if ([IO.Directory]::Exists("$f\GPUCache") -or [IO.Directory]::Exists("$f\Code Cache") -or
                [IO.Directory]::Exists("$f\Default\Cache")) { $f; continue }
            if ($item[1] + 1 -lt $MaxDepth -and $s.Name -notin @('Packages', 'Temp', 'node_modules')) {
                $queue.Enqueue(@($f, $item[1] + 1))
            }
        }
    }
}

Add-Task -Id browsers -Category $C -Name 'Browser caches (Edge, Chrome, Brave, Opera, Vivaldi)' -Tier 1 -Risk Safe `
    -Processes msedge, chrome, brave, opera, vivaldi, chromium `
    -Description 'Web, code, GPU and shader caches of every browser profile. Passwords, cookies, history and logins are NOT touched.' -Action {
    foreach ($r in @(UserPaths $script:BrowserRoots)) { Clear-ChromiumData $r }
}

Add-Task -Id firefox -Category $C -Name 'Firefox-family caches (Firefox, LibreWolf, Zen...)' -Tier 1 -Risk Safe `
    -Processes firefox, librewolf, waterfox, zen `
    -Description 'cache2, startup cache, thumbnails and crash reports. Passwords, cookies and history stay.' -Action {
    foreach ($b in @('Mozilla\Firefox', 'librewolf', 'Waterfox', 'zen')) {
        foreach ($sub in @('cache2', 'startupCache', 'thumbnails', 'jumpListCache', 'shader-cache')) {
            Clear-Path (UserPaths "AppData\Local\$b\Profiles\*\$sub")
        }
        Clear-Path (UserPaths "AppData\Roaming\$b\Crash Reports", "AppData\Roaming\$b\Profiles\*\minidumps",
                              "AppData\Roaming\$b\Profiles\*\saved-telemetry-pings")
    }
}

Add-Task -Id appcache -Category $C -Name 'App caches (Discord, Teams, Slack, VS Code, Steam...)' -Tier 1 -Risk Safe `
    -Processes Discord, DiscordPTB, DiscordCanary, slack, Teams, ms-teams, Code, Spotify, Notion, Signal `
    -Description 'Auto-detects every Electron / Chromium / WebView2 app and empties its web, code and GPU caches. App data and logins stay.' -Action {
    foreach ($pr in $script:Profiles) {
        $roots  = @(Find-ChromiumRoots (Join-Path $pr 'AppData\Roaming') 4)
        $roots += @(Find-ChromiumRoots (Join-Path $pr 'AppData\Local') 4)
        foreach ($pkg in @(Get-Item -Path (Join-Path $pr 'AppData\Local\Packages\*\LocalCache') -Force)) {
            $roots += @(Find-ChromiumRoots $pkg.FullName 4)
        }
        foreach ($r in $roots) { if ($r -notmatch $script:BrowserPattern) { Clear-ChromiumData $r } }
        foreach ($code in @('Code', 'Code - Insiders', 'Cursor', 'VSCodium', 'Windsurf')) {
            $base = Join-Path $pr "AppData\Roaming\$code"
            Clear-Path @("$base\CachedData", "$base\CachedExtensionVSIXs", "$base\logs")
        }
    }
}

Add-Task -Id gpucache -Category $C -Name 'GPU shader caches (DirectX, NVIDIA, AMD, Intel)' -Tier 2 -Risk Safe `
    -Description 'Compiled shader caches. Rebuilt automatically - games may stutter briefly on first launch.' -Action {
    Clear-Path (UserPaths 'AppData\Local\D3DSCache', 'AppData\Local\NVIDIA\DXCache', 'AppData\Local\NVIDIA\GLCache',
        'AppData\Local\NVIDIA Corporation\NV_Cache', 'AppData\LocalLow\NVIDIA\PerDriverVersion\DXCache',
        'AppData\Local\AMD\DxCache', 'AppData\Local\AMD\DxcCache', 'AppData\Local\AMD\VkCache', 'AppData\Local\AMD\GLCache',
        'AppData\Local\AMD\OglCache', 'AppData\Local\Intel\ShaderCache', 'AppData\LocalLow\Intel\ShaderCache')
    Clear-Path "$env:ProgramData\NVIDIA Corporation\NV_Cache"
}

Add-Task -Id uwp -Category $C -Name 'Store app temp & web caches' -Tier 2 -Risk Safe `
    -Description 'TempState, AC\Temp and AC\INetCache of Microsoft Store apps, plus the legacy IE/WebView cache.' -Action {
    foreach ($pr in $script:Profiles) {
        $pk = Join-Path $pr 'AppData\Local\Packages'
        Clear-Path @("$pk\*\AC\INetCache", "$pk\*\AC\Temp", "$pk\*\TempState")
    }
    Clear-Path (UserPaths 'AppData\Local\Microsoft\Windows\INetCache')
}

Add-Task -Id devcache -Category $C -Name 'Developer package caches' -Tier 2 -Risk Moderate `
    -Description 'npm, Yarn, pnpm, pip, uv, Poetry, NuGet, Go, Cargo, Composer, Electron, Scoop, Chocolatey caches. Re-downloaded when needed.' -Action {
    Clear-Path (UserPaths 'AppData\Local\npm-cache', 'AppData\Roaming\npm-cache', 'AppData\Local\Yarn\Cache',
        'AppData\Local\pnpm-cache', 'AppData\Local\pip\Cache', 'AppData\Local\uv\cache', 'AppData\Local\pypoetry\Cache',
        'AppData\Local\NuGet\v3-cache', 'AppData\Local\NuGet\http-cache', 'AppData\Local\NuGet\plugins-cache',
        'AppData\Local\go-build', '.cargo\registry\cache', 'scoop\cache', 'AppData\Local\Composer\files',
        'AppData\Local\Composer\repo', 'AppData\Local\Composer\vcs', 'AppData\Local\electron\Cache',
        'AppData\Local\electron-builder\Cache')
    Clear-Path @("$env:ProgramData\scoop\cache", "$env:ProgramData\chocolatey\lib-bkp", "$env:ProgramData\chocolatey\lib-bad")
}

Add-Task -Id shortcuts -Category $C -Name 'Broken shortcuts (Start menu & Desktop)' -Tier 2 -Risk Safe `
    -Description '.lnk files whose target no longer exists - classic uninstaller leftovers.' -Action {
    $sh = New-Object -ComObject WScript.Shell
    $startRoots = @("$env:ProgramData\Microsoft\Windows\Start Menu") + @(UserPaths 'AppData\Roaming\Microsoft\Windows\Start Menu')
    $keep = @('Start Menu', 'Programs', 'Startup', 'Administrative Tools', 'Windows Tools', 'Accessibility',
              'Accessories', 'System Tools', 'Maintenance', 'Windows PowerShell')
    $n = 0
    foreach ($r in ($startRoots + @("$env:PUBLIC\Desktop") + @(UserPaths 'Desktop'))) {
        foreach ($lnk in @(Get-BroomFiles -Root $r -Filter '*.lnk')) {
            try { $t = $sh.CreateShortcut($lnk.FullName).TargetPath } catch { continue }
            if (-not $t -or $t -like "$env:windir\Installer\*" -or -not (Test-PathMissing $t)) { continue }
            Write-Log "Broken shortcut: $($lnk.FullName) -> $t"
            $n++
            Remove-BroomFile $lnk
            $parent = $lnk.DirectoryName
            $inStart = @($startRoots | Where-Object { $parent.StartsWith($_, [StringComparison]::OrdinalIgnoreCase) }).Count -gt 0
            if (-not $script:DryRun -and $inStart -and ($keep -notcontains (Split-Path $parent -Leaf))) {
                try { [IO.Directory]::Delete($parent, $false) } catch {}
            }
        }
    }
    $script:TaskNote = "$n shortcuts"
}

function Test-TreeEmpty([string]$Dir) {
    $rp = [IO.FileAttributes]::ReparsePoint
    $stack = New-Object 'System.Collections.Generic.Stack[string]'
    $stack.Push($Dir)
    $seen = 0
    while ($stack.Count -gt 0) {
        if (++$seen -gt 200) { return $false }
        try { $entries = (New-Object IO.DirectoryInfo($stack.Pop())).GetFileSystemInfos() } catch { return $false }
        foreach ($e in $entries) {
            if (($e.Attributes -band $rp) -or ($e -isnot [IO.DirectoryInfo])) { return $false }
            $stack.Push($e.FullName)
        }
    }
    $true
}

Add-Task -Id emptydirs -Category $C -Name 'Empty leftover folders of uninstalled apps' -Tier 2 -Risk Moderate `
    -Description 'Top-level folders in Program Files, ProgramData and AppData that contain no files at all.' -Action {
    $roots = @($env:ProgramFiles, ${env:ProgramFiles(x86)}, "$env:SystemDrive\Program Files (Arm)", $env:ProgramData) +
             @(UserPaths 'AppData\Local', 'AppData\Roaming', 'AppData\LocalLow', 'AppData\Local\Programs')
    $skip = @('Temp', 'Packages', 'Microsoft', 'Programs', 'WindowsApps', 'ModifiableWindowsApps', 'ssh', 'Windows Defender',
              'Windows Defender Advanced Threat Protection', 'Windows NT', 'Common Files', 'Internet Explorer',
              'Reference Assemblies', 'Windows Mail', 'Windows Media Player', 'Windows Photo Viewer', 'Windows Sidebar',
              'WindowsPowerShell', 'Windows Security', 'Uninstall Information', 'Microsoft Update Health Tools',
              'PackageManagement', 'Package Cache', 'Comms', 'ConnectedDevicesPlatform', 'Desktop', 'Documents',
              'Start Menu', 'Templates', 'Application Data', 'History', 'Temporary Internet Files', 'VirtualStore',
              'Publishers', 'PeerDistRepub', 'PlaceholderTileLogoFolder', 'regid.1991-06.com.microsoft', 'USOPrivate',
              'USOShared', 'SoftwareDistribution', 'dotnet', 'MSBuild', 'Broom')
    $rp = [IO.FileAttributes]::ReparsePoint
    $n = 0
    foreach ($r in $roots) {
        if (-not $r -or -not [IO.Directory]::Exists($r)) { continue }
        try { $subs = (New-Object IO.DirectoryInfo($r)).GetDirectories() } catch { continue }
        foreach ($d in $subs) {
            if (($d.Attributes -band $rp) -or ($skip -contains $d.Name) -or -not (Test-TreeEmpty $d.FullName)) { continue }
            if ($script:DryRun) { $n++; Write-Log "Empty folder: $($d.FullName)"; continue }
            try { [IO.Directory]::Delete($d.FullName, $true); $n++; Write-Log "Removed empty folder: $($d.FullName)" } catch {}
        }
    }
    $script:TaskNote = "$n folders"
}

Add-Task -Id privacy -Category $C -Name 'Recent files, jump lists & Explorer history' -Tier 3 -Risk Moderate `
    -Description 'Recent items, jump lists, Run box / address bar / search history and Open-Save dialog history. Traces only, no real files.' -Action {
    $b = $script:RegCount
    Clear-Path (UserPaths 'AppData\Roaming\Microsoft\Windows\Recent\AutomaticDestinations',
                          'AppData\Roaming\Microsoft\Windows\Recent\CustomDestinations')
    Clear-Path (UserPaths 'AppData\Roaming\Microsoft\Windows\Recent') -Filter '*.lnk' -NoRecurse
    $rel = @('RecentDocs', 'RunMRU', 'TypedPaths', 'WordWheelQuery', 'ComDlg32\OpenSavePidlMRU',
             'ComDlg32\LastVisitedPidlMRU', 'ComDlg32\LastVisitedPidlMRULegacy')
    foreach ($sid in Get-UserSids) {
        foreach ($r in $rel) {
            $name = "HKEY_USERS\$sid\Software\Microsoft\Windows\CurrentVersion\Explorer\$r"
            $k = Open-RegKey $name
            if ($k) { $k.Close(); Remove-RegKey $name 'privacy trace' }
        }
    }
    $script:TaskNote = "$($script:RegCount - $b) history keys"
}
#endregion

#region ---------------------------------------------------------------- Tasks: Registry
$C = 'Registry'

function Remove-MissingPathValues([string]$KeyName, [string]$Why) {
    $k = Open-RegKey $KeyName
    if (-not $k) { return }
    $names = $k.GetValueNames()
    $k.Close()
    foreach ($vn in $names) { if (Test-PathMissing $vn) { Remove-RegValue $KeyName $vn $Why } }
}

Add-Task -Id reguninstall -Category $C -Name 'Ghost entries in "Installed apps"' -Tier 2 -Risk Moderate `
    -Description 'Uninstall entries whose uninstaller AND install folder are both gone. MSI-based entries are never touched.' -Action {
    $b = $script:RegCount
    $roots = @('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall',
               'HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall') +
             @(Get-UserSids | ForEach-Object { "HKEY_USERS\$_\Software\Microsoft\Windows\CurrentVersion\Uninstall" })
    foreach ($root in $roots) {
        $rk = Open-RegKey $root
        if (-not $rk) { continue }
        foreach ($sub in $rk.GetSubKeyNames()) {
            $k = $rk.OpenSubKey($sub)
            if (-not $k) { continue }
            $u   = [string]$k.GetValue('UninstallString')
            $wi  = $k.GetValue('WindowsInstaller')
            $sc  = $k.GetValue('SystemComponent')
            $loc = [string]$k.GetValue('InstallLocation')
            $k.Close()
            if (-not $u -or $wi -eq 1 -or $sc -eq 1 -or $u -match 'msiexec') { continue }
            $exe = Get-ExeFromCommand $u
            if (-not $exe -or -not (Test-PathMissing $exe)) { continue }
            if ($loc -and -not (Test-PathMissing $loc)) { continue }
            Remove-RegKey "$root\$sub" "uninstaller missing: $exe"
        }
        $rk.Close()
    }
    $script:TaskNote = "$($script:RegCount - $b) entries"
}

Add-Task -Id regapppaths -Category $C -Name 'Dead "App Paths" registrations' -Tier 2 -Risk Safe `
    -Description 'Win+R / Start shortcuts (App Paths) that point to programs that no longer exist.' -Action {
    $b = $script:RegCount
    $roots = @('HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths',
               'HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths') +
             @(Get-UserSids | ForEach-Object { "HKEY_USERS\$_\Software\Microsoft\Windows\CurrentVersion\App Paths" })
    foreach ($root in $roots) {
        $rk = Open-RegKey $root
        if (-not $rk) { continue }
        foreach ($sub in $rk.GetSubKeyNames()) {
            $k = $rk.OpenSubKey($sub)
            if (-not $k) { continue }
            $d = [string]$k.GetValue('')
            $k.Close()
            if (-not $d) { continue }
            $exe = Get-ExeFromCommand $d
            if (-not $exe) { $exe = $d }
            if (Test-PathMissing $exe) { Remove-RegKey "$root\$sub" "target missing: $exe" }
        }
        $rk.Close()
    }
    $script:TaskNote = "$($script:RegCount - $b) entries"
}

Add-Task -Id regrun -Category $C -Name 'Dead startup entries (Run / RunOnce)' -Tier 2 -Risk Moderate `
    -Description 'Autostart entries whose program was uninstalled. Also removes their Task Manager "Startup apps" toggle.' -Action {
    $b = $script:RegCount
    $rels = @('Software\Microsoft\Windows\CurrentVersion\Run', 'Software\Microsoft\Windows\CurrentVersion\RunOnce',
              'Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run', 'Software\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce')
    $hives = @('HKEY_LOCAL_MACHINE') + @(Get-UserSids | ForEach-Object { "HKEY_USERS\$_" })
    foreach ($h in $hives) {
        foreach ($rel in $rels) {
            $name = "$h\$rel"
            $k = Open-RegKey $name
            if (-not $k) { continue }
            $vals = @($k.GetValueNames() | ForEach-Object { , @($_, [string]$k.GetValue($_)) })
            $k.Close()
            foreach ($v in $vals) {
                $exe = Get-ExeFromCommand $v[1]
                if (-not $exe -or -not (Test-PathMissing $exe)) { continue }
                Remove-RegValue $name $v[0] "missing: $exe"
                $sa = "$h\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\" + $(if ($rel -like '*WOW6432Node*') { 'Run32' } else { 'Run' })
                $sk = Open-RegKey $sa
                if ($sk) {
                    $has = $sk.GetValueNames() -contains $v[0]
                    $sk.Close()
                    if ($has) { Remove-RegValue $sa $v[0] 'startup toggle of removed entry' }
                }
            }
        }
    }
    $script:TaskNote = "$($script:RegCount - $b) entries"
}

Add-Task -Id regmui -Category $C -Name 'MUI cache of deleted programs' -Tier 2 -Risk Safe `
    -Description 'Cached display names of programs that no longer exist.' -Action {
    $b = $script:RegCount
    foreach ($sid in Get-UserSids) {
        $name = "HKEY_USERS\$($sid)_Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache"
        $k = Open-RegKey $name
        if (-not $k) { continue }
        $names = $k.GetValueNames()
        $k.Close()
        foreach ($vn in $names) {
            $p = ($vn -replace '\.(FriendlyAppName|ApplicationCompany)$', '') -replace '^@', '' -replace ',-?\d+$', ''
            if (Test-PathMissing $p) { Remove-RegValue $name $vn 'program missing' }
        }
    }
    $script:TaskNote = "$($script:RegCount - $b) entries"
}

Add-Task -Id regshared -Category $C -Name 'Missing shared DLL & installer folder references' -Tier 2 -Risk Moderate `
    -Description 'SharedDLLs counters and Windows Installer folder references for files/folders that no longer exist.' -Action {
    $b = $script:RegCount
    Remove-MissingPathValues 'HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\SharedDLLs' 'file missing'
    Remove-MissingPathValues 'HKEY_LOCAL_MACHINE\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\SharedDLLs' 'file missing'
    Remove-MissingPathValues 'HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\Folders' 'folder missing'
    $script:TaskNote = "$($script:RegCount - $b) entries"
}

Add-Task -Id regcom -Category $C -Name 'Orphaned COM / ActiveX registrations' -Tier 3 -Risk Aggressive -Slow `
    -Description 'CLSID entries whose every server DLL/EXE is gone. Only absolute paths on present, readable drives are judged.' -Action {
    $b = $script:RegCount
    $roots = @('HKEY_LOCAL_MACHINE\SOFTWARE\Classes\CLSID', 'HKEY_LOCAL_MACHINE\SOFTWARE\Classes\WOW6432Node\CLSID') +
             @(Get-UserSids | ForEach-Object { "HKEY_USERS\$($_)_Classes\CLSID" })
    foreach ($root in $roots) {
        $rk = Open-RegKey $root
        if (-not $rk) { continue }
        foreach ($clsid in $rk.GetSubKeyNames()) {
            $ck = $rk.OpenSubKey($clsid)
            if (-not $ck) { continue }
            $servers = @()
            foreach ($srv in @('InprocServer32', 'LocalServer32', 'InprocHandler32')) {
                $sk = $ck.OpenSubKey($srv)
                if ($sk) { $servers += [string]$sk.GetValue(''); $sk.Close() }
            }
            $ck.Close()
            if ($servers.Count -eq 0) { continue }
            $orphan = $true
            foreach ($s in $servers) {
                $p = Get-ExeFromCommand $s
                if (-not $p) { $p = $s }
                if (-not (Test-PathMissing $p)) { $orphan = $false; break }
            }
            if ($orphan) { Remove-RegKey "$root\$clsid" ("server missing: " + ($servers -join ' | ')) }
        }
        $rk.Close()
    }
    $script:TaskNote = "$($script:RegCount - $b) entries"
}
#endregion

#region ---------------------------------------------------------------- TUI primitives
$script:LogoLines = @(
    '    ____                            '
    '   / __ )_________  ____  ____ ___  '
    '  / __  / ___/ __ \/ __ \/ __ `__ \ '
    ' / /_/ / /  / /_/ / /_/ / / / / / / '
    '/_____/_/   \____/\____/_/ /_/ /_/  '
)
$script:BroomLines = @(
    '        ||        '
    '        ||        '
    '       /||\       '
    '      //||\\   .  '
    '     ///||\\\ .:: '
)

function Get-Width  { try { [Math]::Max(60, [Console]::WindowWidth) } catch { 100 } }
function Get-Height { try { [Math]::Max(20, [Console]::WindowHeight) } catch { 30 } }

# Write-Row 'text' Color 'more text' 'Black/Cyan' ...   -> one full-width line, colors as Fg or Fg/Bg
function Write-Row {
    $w = (Get-Width) - 1
    $len = 0
    for ($i = 0; $i -lt $args.Count; $i += 2) {
        $t = [string]$args[$i]
        $c = if ($i + 1 -lt $args.Count) { [string]$args[$i + 1] } else { 'Gray' }
        if ($len + $t.Length -gt $w) { $t = $t.Substring(0, [Math]::Max(0, $w - $len)) }
        if ($t.Length -eq 0) { continue }
        $fg, $bg = $c.Split('/')
        if ($bg) { Write-Host $t -NoNewline -ForegroundColor $fg -BackgroundColor $bg }
        else     { Write-Host $t -NoNewline -ForegroundColor $fg }
        $len += $t.Length
    }
    if ($len -lt $w) { Write-Host (' ' * ($w - $len)) -NoNewline }
    Write-Host ''
}

function Write-Rule { Write-Host ('  ' + ('-' * ((Get-Width) - 5))) -ForegroundColor DarkGray }

function Get-WrappedLines([string]$Text, [int]$Width, [int]$Max = 2) {
    $lines = @()
    $line = ''
    foreach ($word in ($Text -split '\s+')) {
        if ($line.Length -gt 0 -and ($line.Length + $word.Length + 1) -gt $Width) { $lines += $line; $line = $word }
        else { $line = ($line + ' ' + $word).Trim() }
    }
    $lines += $line
    while ($lines.Count -lt $Max) { $lines += '' }
    $lines[0..($Max - 1)]
}

function Write-Header([string]$Subtitle = '') {
    try { Clear-Host } catch {}   # throws without a real console (CI, redirected output)
    Write-Host ''
    for ($i = 0; $i -lt $script:LogoLines.Count; $i++) {
        Write-Host ('  ' + $script:LogoLines[$i]) -NoNewline -ForegroundColor $(if ($i -lt 3) { 'Cyan' } else { 'DarkCyan' })
        Write-Host $script:BroomLines[$i] -ForegroundColor $(if ($i -lt 2) { 'DarkYellow' } else { 'Yellow' })
    }
    $drive = New-Object IO.DriveInfo($env:SystemDrive)
    Write-Row '  sweeps Windows clean  ' DarkGray "v$script:Version" DarkGray
    Write-Row '  ' Gray $script:OsName White " $script:OsBuild" Gray '  |  ' DarkGray $script:Arch Cyan '  |  ' DarkGray `
              "$env:SystemDrive " Gray (Format-Size $drive.AvailableFreeSpace) Green ' free of ' Gray (Format-Size $drive.TotalSize) Gray
    if (-not $script:IsAdmin) { Write-Row '  ! Not running as administrator - many items will be skipped.' Red }
    Write-Rule
    if ($Subtitle) { Write-Row "  $Subtitle" White; Write-Host '' }
}

function Read-YesNo([string]$Question, [bool]$Default = $true) {
    Write-Host "  $Question " -NoNewline -ForegroundColor White
    Write-Host $(if ($Default) { '[Y/n] ' } else { '[y/N] ' }) -NoNewline -ForegroundColor DarkGray
    while ($true) {
        $k = [Console]::ReadKey($true)
        if ($k.Key -eq 'Enter')  { Write-Host $(if ($Default) { 'Yes' } else { 'No' }); return $Default }
        if ($k.Key -eq 'Escape') { Write-Host 'No'; return $false }
        $c = [string]$k.KeyChar
        if ($c -eq 'y') { Write-Host 'Yes'; return $true }
        if ($c -eq 'n') { Write-Host 'No';  return $false }
    }
}

function Wait-Key {
    Write-Host ''
    Write-Row '  Press any key to return to the menu' DarkGray
    [void][Console]::ReadKey($true)
}

function Select-Tier([int]$Tier) { foreach ($t in $script:Tasks) { $t.Selected = ($t.Tier -le $Tier) } }
function Get-TierCount([int]$Tier) { @($script:Tasks | Where-Object { $_.Tier -le $Tier }).Count }
#endregion

#region ---------------------------------------------------------------- Screens
$script:MainItems = @(
    [pscustomobject]@{ Key = '1'; Label = 'Quick Sweep';    Action = 'Quick';   Extra = "$(Get-TierCount 1) items"
        Hint = 'Safe everyday junk: temp files, caches, logs, update downloads, crash dumps, recycle bin.' }
    [pscustomobject]@{ Key = '2'; Label = 'Deep Clean';     Action = 'Deep';    Extra = "$(Get-TierCount 2) items"
        Hint = 'Quick + Windows.old, WinSxS, Disk Cleanup, installer leftovers, app caches, broken shortcuts, registry.' }
    [pscustomobject]@{ Key = '3'; Label = 'Nuclear';        Action = 'Nuclear'; Extra = "$(Get-TierCount 3) items"
        Hint = 'EVERYTHING: + old drivers, old restore points, hibernation, reserved storage, event logs, COM registry, history.' }
    [pscustomobject]@{ Key = '4'; Label = 'Custom';         Action = 'Custom';  Extra = 'you pick'
        Hint = 'Choose exactly what gets swept, item by item. Can also run as a dry run.' }
    [pscustomobject]@{ Key = '5'; Label = 'Analyze';        Action = 'Analyze'; Extra = 'dry run'
        Hint = 'Checks everything and shows how much space you would get back. Deletes nothing.' }
    [pscustomobject]@{ Key = '6'; Label = 'Backups & Logs'; Action = 'Backups'; Extra = ''
        Hint = 'Restore a registry backup, open the logs, or open Windows System Restore.' }
    [pscustomobject]@{ Key = 'Q'; Label = 'Quit';           Action = 'Quit';    Extra = ''
        Hint = 'Leave the dust where it is.' }
)

function Show-MainMenu {
    $idx = 0
    $n = $script:MainItems.Count
    Write-Header
    $top = [Console]::CursorTop
    while ($true) {
        [Console]::SetCursorPosition(0, $top)
        Write-Row ''
        for ($i = 0; $i -lt $n; $i++) {
            $it = $script:MainItems[$i]
            $label = ' [{0}]  {1,-16} ' -f $it.Key, $it.Label
            $labelColor = if ($it.Action -eq 'Nuclear') { 'Red' } else { 'White' }
            if ($i -eq $idx) { Write-Row '   > ' Yellow $label 'Black/Cyan' "  $($it.Extra)" DarkGray }
            else             { Write-Row '     ' Gray $label $labelColor "  $($it.Extra)" DarkGray }
            if ($it.Key -eq '3' -or $it.Key -eq '6') { Write-Row '' }
        }
        Write-Rule
        foreach ($l in (Get-WrappedLines $script:MainItems[$idx].Hint ((Get-Width) - 6) 2)) { Write-Row "  $l" Gray }
        Write-Row ''
        Write-Row '  [Up/Down] move   [Enter] select   [1-6] jump   [Q] quit' DarkGray
        $k = [Console]::ReadKey($true)
        switch ($k.Key) {
            'UpArrow'   { $idx = ($idx - 1 + $n) % $n }
            'DownArrow' { $idx = ($idx + 1) % $n }
            'Enter'     { return $script:MainItems[$idx].Action }
            'Escape'    { return 'Quit' }
            default {
                $ch = ([string]$k.KeyChar).ToUpper()
                $hit = @($script:MainItems | Where-Object { $_.Key -eq $ch })
                if ($hit.Count -gt 0) { return $hit[0].Action }
            }
        }
    }
}

function Show-TaskPicker {
    $rows = New-Object 'System.Collections.Generic.List[object]'
    foreach ($cat in $script:Categories) {
        $rows.Add([pscustomobject]@{ Header = $cat; Task = $null })
        foreach ($t in $script:Tasks) { if ($t.Category -eq $cat) { $rows.Add([pscustomobject]@{ Header = $null; Task = $t }) } }
    }
    $taskRows = @(for ($i = 0; $i -lt $rows.Count; $i++) { if ($rows[$i].Task) { $i } })
    $riskColor = @{ Safe = 'Green'; Moderate = 'Yellow'; Aggressive = 'Red' }
    $cur = 0; $scroll = 0; $dry = $false
    Write-Header 'CUSTOM SWEEP - choose what to clean'
    $top = [Console]::CursorTop
    while ($true) {
        $view = [Math]::Max(5, (Get-Height) - $top - 7)
        $rowIdx = $taskRows[$cur]
        $want = if ($rowIdx -gt 0 -and $rows[$rowIdx - 1].Header) { $rowIdx - 1 } else { $rowIdx }
        if ($want -lt $scroll) { $scroll = $want }
        if ($rowIdx -ge $scroll + $view) { $scroll = $rowIdx - $view + 1 }
        $nameW = [Math]::Min(58, (Get-Width) - 24)

        [Console]::SetCursorPosition(0, $top)
        for ($v = 0; $v -lt $view; $v++) {
            $ri = $scroll + $v
            if ($ri -ge $rows.Count) { Write-Row ''; continue }
            $r = $rows[$ri]
            if ($r.Header) { Write-Row ('  ' + $r.Header.ToUpper() + ' ') Cyan ('-' * 30) DarkGray; continue }
            $t = $r.Task
            $box = if ($t.Selected) { '[x]' } else { '[ ]' }
            $boxColor = if ($t.Selected) { 'Green' } else { 'DarkGray' }
            $name = " $($t.Name)"
            if ($name.Length -gt $nameW) { $name = $name.Substring(0, $nameW - 3) + '...' }
            $name = $name.PadRight($nameW)
            $risk = '  ' + $t.Risk.ToUpper()
            if ($ri -eq $rowIdx) { Write-Row '  > ' Yellow $box $boxColor $name 'Black/Cyan' $risk $riskColor[$t.Risk] }
            else                 { Write-Row '    ' Gray   $box $boxColor $name 'White'      $risk $riskColor[$t.Risk] }
        }
        Write-Rule
        $t = $rows[$rowIdx].Task
        foreach ($l in (Get-WrappedLines $t.Description ((Get-Width) - 6) 2)) { Write-Row "  $l" Gray }
        $selCount = @($script:Tasks | Where-Object { $_.Selected }).Count
        Write-Row '  Selected ' DarkGray "$selCount/$($script:Tasks.Count)" White '     Mode ' DarkGray `
                  $(if ($dry) { 'ANALYZE (dry run, deletes nothing)' } else { 'CLEAN' }) $(if ($dry) { 'Cyan' } else { 'Yellow' })
        Write-Row '  [Up/Dn] move [Space] toggle [A]ll [N]one [1][2][3] Quick/Deep/Nuclear [D]ry run [Enter] go [Esc] back' DarkGray

        $k = [Console]::ReadKey($true)
        switch ($k.Key) {
            'UpArrow'   { if ($cur -gt 0) { $cur-- } }
            'DownArrow' { if ($cur -lt $taskRows.Count - 1) { $cur++ } }
            'PageUp'    { $cur = [Math]::Max(0, $cur - $view) }
            'PageDown'  { $cur = [Math]::Min($taskRows.Count - 1, $cur + $view) }
            'Home'      { $cur = 0 }
            'End'       { $cur = $taskRows.Count - 1 }
            'Spacebar'  { $t.Selected = -not $t.Selected }
            'Enter'     { if ($selCount -gt 0) { $script:DryRun = $dry; return $true } }
            'Escape'    { return $false }
            default {
                switch ([string]$k.KeyChar) {
                    'a' { foreach ($x in $script:Tasks) { $x.Selected = $true } }
                    'n' { foreach ($x in $script:Tasks) { $x.Selected = $false } }
                    '1' { Select-Tier 1 }
                    '2' { Select-Tier 2 }
                    '3' { Select-Tier 3 }
                    'd' { $dry = -not $dry }
                }
            }
        }
    }
}

function Show-Confirm([string]$Title, [switch]$Danger) {
    $sel = @($script:Tasks | Where-Object { $_.Selected })
    Write-Header $Title
    Write-Row '  Broom is about to sweep ' Gray "$($sel.Count) items" White ':' Gray
    Write-Host ''
    foreach ($cat in $script:Categories) {
        $items = @($sel | Where-Object { $_.Category -eq $cat })
        if ($items.Count -eq 0) { continue }
        Write-Row ('   {0,-24}' -f $cat) Cyan ('{0,3} items' -f $items.Count) White
    }
    $aggr = @($sel | Where-Object { $_.Risk -eq 'Aggressive' })
    if ($aggr.Count -gt 0) {
        Write-Host ''
        Write-Row '  Aggressive items (no way back):' Red
        foreach ($a in $aggr) { Write-Row '   ! ' Red $a.Name Gray }
    }
    Write-Host ''
    Write-Row '  Safety net' White
    if ($NoRestorePoint) { Write-Row '   - System Restore point: skipped (-NoRestorePoint)' Yellow }
    else                 { Write-Row '   + System Restore point is created first' Green }
    Write-Row '   + Every registry change is exported to ' Green $script:BackupRoot Gray
    Write-Row '   + Documents, Downloads, Pictures, Videos, Music and OneDrive are never touched' Green
    Write-Row '   + Files in use are skipped, nothing is followed through junctions' Green
    Write-Host ''
    if ($Danger) { Write-Row '  NUCLEAR also removes rollback options: old restore points, Windows.old, update uninstall, hibernation.' Red; Write-Host '' }
    Read-YesNo 'Start sweeping now?' (-not $Danger)
}
#endregion

#region ---------------------------------------------------------------- Runner
function New-BroomRestorePoint {
    $key  = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'
    $name = 'SystemRestorePointCreationFrequency'
    $old  = (Get-ItemProperty -Path $key -Name $name).$name
    New-ItemProperty -Path $key -Name $name -Value 0 -PropertyType DWord -Force | Out-Null   # lift the 1-per-24h limit
    $ok = $false
    try {
        $r = Invoke-CimMethod -Namespace root/default -ClassName SystemRestore -MethodName CreateRestorePoint -ErrorAction Stop -Arguments @{
            Description = "Broom $(Get-Date -Format 'yyyy-MM-dd HH:mm')"; RestorePointType = [uint32]12; EventType = [uint32]100 }
        $ok = ($r.ReturnValue -eq 0)
    } catch {}
    if ($null -eq $old) { Remove-ItemProperty -Path $key -Name $name -Force }
    else { Set-ItemProperty -Path $key -Name $name -Value $old }
    $ok
}

function Close-RunningApps {
    $names = @($script:Tasks | Where-Object { $_.Selected } | ForEach-Object { $_.Processes } | Select-Object -Unique)
    if ($names.Count -eq 0) { return }
    $running = @(Get-Process -Name $names -ErrorAction SilentlyContinue)
    if ($running.Count -eq 0) { return }
    $list = ($running | Select-Object -ExpandProperty ProcessName -Unique) -join ', '
    Write-Row '  Running apps lock their caches: ' Yellow $list White
    if ($Yes) { Write-Row '  (unattended mode: leaving them open, locked files are skipped)' DarkGray; return }
    if (-not (Read-YesNo 'Close them now? Unsaved work in them may be lost.' $true)) { return }
    foreach ($p in $running) { try { [void]$p.CloseMainWindow() } catch {} }
    Start-Sleep -Seconds 3
    Get-Process -Name $names -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Milliseconds 800
}

function Invoke-Sweep {
    $sel = @($script:Tasks | Where-Object { $_.Selected })
    $script:Results.Clear()
    $script:FilesRemoved = 0; $script:RegCount = 0; $script:RegBackupIdx = 0
    $script:BackupDir = $null; $script:BackedUpKeys.Clear()

    $title = if ($script:DryRun) { 'ANALYZING - dry run, nothing is deleted' } else { 'SWEEPING' }
    Write-Header $title
    Write-Log "=== Broom $script:Version | $title | $($sel.Count) items | $script:OsName $script:OsBuild $script:Arch"
    if (-not $script:DryRun) {
        Close-RunningApps
        if (-not $NoRestorePoint) {
            Write-Host '  Creating restore point ... ' -NoNewline -ForegroundColor Gray
            if (New-BroomRestorePoint) { Write-Host 'done' -ForegroundColor Green }
            else { Write-Host 'not possible (System Protection is off) - registry backups still apply' -ForegroundColor Yellow }
        }
        Write-Host ''
    }
    $script:FreeBefore = Get-FreeSpace

    $w = [Math]::Max(40, [Math]::Min(70, (Get-Width) - 40))
    $i = 0
    foreach ($t in $sel) {
        $i++
        $label = '  [{0,2}/{1}] {2} ' -f $i, $sel.Count, $t.Name
        if ($label.Length -gt $w - 2) { $label = $label.Substring(0, $w - 5) + '... ' }
        $line = $label + ('.' * ($w - $label.Length))
        $hint = if ($t.Slow -and -not $script:DryRun) { ' working - can take several minutes' } else { ' working' }
        Write-Host $line -NoNewline -ForegroundColor White
        Write-Host $hint -NoNewline -ForegroundColor DarkGray

        $script:TaskNote = $null
        $err = $null
        [int64]$bytes = 0
        $before = Get-FreeSpace
        $sw = [Diagnostics.Stopwatch]::StartNew()
        try {
            foreach ($o in @(& $t.Action)) {
                if ($o -is [int64] -or $o -is [int32] -or $o -is [double]) { $bytes += [int64]$o }
            }
        } catch { $err = $_.Exception.Message }
        if ($t.Delta -and -not $script:DryRun) { $bytes = [Math]::Max([int64]0, (Get-FreeSpace) - $before) }
        $sw.Stop()

        $size = ' {0,10}' -f (Format-Size $bytes)
        $sizeColor = if ($err) { 'Red' } elseif ($bytes -ge 100MB) { 'Green' } elseif ($bytes -gt 0) { 'DarkGreen' } else { 'DarkGray' }
        if ($err) { $size = ' {0,10}' -f 'error' }
        $tail = '  {0:N1}s' -f $sw.Elapsed.TotalSeconds
        if ($script:TaskNote) { $tail = "  $script:TaskNote" + $tail }
        $pad = [Math]::Max(0, $hint.Length - $size.Length - $tail.Length)
        Write-Host "`r$line" -NoNewline -ForegroundColor White
        Write-Host $size -NoNewline -ForegroundColor $sizeColor
        Write-Host ($tail + (' ' * $pad)) -ForegroundColor DarkGray

        $script:Results.Add([pscustomobject]@{ Name = $t.Name; Bytes = $bytes; Note = $script:TaskNote; Error = $err })
        Write-Log ('{0}: {1} {2} {3}' -f $t.Name, (Format-Size $bytes), $script:TaskNote, $err)
    }
}

function Show-Summary {
    $total = [int64](($script:Results | Measure-Object -Property Bytes -Sum).Sum)
    Write-Host ''
    Write-Rule
    Write-Host ''
    if ($script:DryRun) { Write-Row '  ' Gray ' ANALYSIS DONE ' 'Black/Cyan' '   Broom could free about ' Gray (Format-Size $total) Green }
    else                { Write-Row '  ' Gray ' ALL SWEPT '     'Black/Green' '   Freed ' Gray (Format-Size $total) Green }
    Write-Host ''
    $verb = if ($script:DryRun) { 'to remove' } else { 'removed' }
    Write-Row ('  Files {0}   ' -f $verb) DarkGray ('{0:N0}' -f $script:FilesRemoved) White `
              ('      Registry entries {0}   ' -f $verb) DarkGray ('{0:N0}' -f $script:RegCount) White
    if (-not $script:DryRun) {
        Write-Row "  $env:SystemDrive free before   " DarkGray (Format-Size $script:FreeBefore) White '      after   ' DarkGray (Format-Size (Get-FreeSpace)) Green
    }
    $top = @($script:Results | Where-Object { $_.Bytes -gt 0 } | Sort-Object Bytes -Descending | Select-Object -First 6)
    if ($top.Count -gt 0) {
        Write-Host ''
        Write-Row '  Biggest wins' White
        $max = [double]$top[0].Bytes
        foreach ($r in $top) {
            $bar = '#' * [Math]::Max(1, [int](24 * $r.Bytes / $max))
            Write-Row ('   {0,10}  ' -f (Format-Size $r.Bytes)) Green $bar.PadRight(26) Cyan $r.Name Gray
        }
    }
    $fails = @($script:Results | Where-Object { $_.Error })
    if ($fails.Count -gt 0) {
        Write-Host ''
        foreach ($f in $fails) { Write-Row '  ! ' Red "$($f.Name): $($f.Error)" Gray }
    }
    Write-Host ''
    Write-Row '  Log              ' DarkGray $script:LogFile Gray
    if ($script:BackupDir) { Write-Row '  Registry backup  ' DarkGray $script:BackupDir Gray }
    if (-not $script:DryRun) { Write-Row '  Tip: restart Windows to finish (WinSxS, hibernation, reserved storage, locked files).' Yellow }
    Write-Log ('=== Total {0} | files {1} | registry {2}' -f (Format-Size $total), $script:FilesRemoved, $script:RegCount)
}

function Show-Backups {
    while ($true) {
        $sets = @(Get-ChildItem -LiteralPath $script:BackupRoot -Directory | Sort-Object Name -Descending | Select-Object -First 9)
        Write-Header 'BACKUPS & LOGS'
        Write-Row '  Registry backups in ' DarkGray $script:BackupRoot Gray
        Write-Host ''
        if ($sets.Count -eq 0) { Write-Row '   (none yet - they are created on the first real run that touches the registry)' DarkGray }
        for ($i = 0; $i -lt $sets.Count; $i++) {
            $n = @(Get-ChildItem -LiteralPath $sets[$i].FullName -Filter '*.reg').Count
            $when = try { [datetime]::ParseExact($sets[$i].Name, 'yyyyMMdd-HHmmss', $null).ToString('yyyy-MM-dd  HH:mm') } catch { $sets[$i].Name }
            Write-Row ('   [{0}]  ' -f ($i + 1)) Yellow $when White ("    $n keys") DarkGray
        }
        Write-Host ''
        Write-Row '  [1-9] restore that backup   [O] open backups   [L] open logs   [S] System Restore   [Esc] back' DarkGray
        $k = [Console]::ReadKey($true)
        if ($k.Key -eq 'Escape') { return }
        $c = ([string]$k.KeyChar).ToUpper()
        if ($c -eq 'O') { New-Item -ItemType Directory -Path $script:BackupRoot -Force | Out-Null; Start-Process explorer.exe -ArgumentList "`"$script:BackupRoot`"" }
        elseif ($c -eq 'L') { Start-Process explorer.exe -ArgumentList "`"$script:LogDir`"" }
        elseif ($c -eq 'S') { Start-Process (Join-Path $env:windir 'System32\rstrui.exe') }
        elseif ($c -match '^[1-9]$' -and [int]$c -le $sets.Count) {
            $set = $sets[[int]$c - 1]
            Write-Host ''
            if (Read-YesNo "Re-import every registry key from $($set.Name)?" $false) {
                $files = @(Get-ChildItem -LiteralPath $set.FullName -Filter '*.reg' | Sort-Object Name)
                foreach ($f in $files) { & reg.exe import "$($f.FullName)" 2>&1 | Out-Null }
                Write-Log "Restored registry backup $($set.FullName) ($($files.Count) files)"
                Write-Row "  Restored $($files.Count) keys." Green
                Start-Sleep -Seconds 2
            }
        }
    }
}
#endregion

#region ---------------------------------------------------------------- Main
function Initialize-Console {
    try { $Host.UI.RawUI.WindowTitle = "Broom $script:Version" } catch {}
    try { [Console]::CursorVisible = $false } catch {}
    try {   # classic console window: make room for the TUI (Windows Terminal ignores this)
        $raw = $Host.UI.RawUI
        if ($raw.WindowSize.Width -lt 110 -or $raw.WindowSize.Height -lt 36) {
            $buf = $raw.BufferSize
            if ($buf.Width -lt 110) { $buf.Width = 110; $raw.BufferSize = $buf }
            $ws = $raw.WindowSize
            $ws.Width  = [Math]::Min(110, $raw.MaxPhysicalWindowSize.Width)
            $ws.Height = [Math]::Min(36,  $raw.MaxPhysicalWindowSize.Height)
            $raw.WindowSize = $ws
        }
    } catch {}
}

function Start-Broom {
    Initialize-Console
    if ($Mode -ne 'Menu') {
        switch ($Mode) {
            'Quick'   { Select-Tier 1 }
            'Deep'    { Select-Tier 2 }
            'Nuclear' { Select-Tier 3 }
            'Analyze' { Select-Tier 3; $script:DryRun = $true }
        }
        if (-not $script:DryRun -and -not $Yes) {
            if (-not (Show-Confirm $Mode.ToUpper() -Danger:($Mode -eq 'Nuclear'))) { return }
        }
        Invoke-Sweep
        Show-Summary
        if (-not $Yes) { Wait-Key }
        return
    }

    $customReady = $false
    while ($true) {
        $script:DryRun = $false
        $action = Show-MainMenu
        if ($action -eq 'Quit') { break }
        switch ($action) {
            'Quick'   { Select-Tier 1; if (Show-Confirm 'QUICK SWEEP') { Invoke-Sweep; Show-Summary; Wait-Key } }
            'Deep'    { Select-Tier 2; if (Show-Confirm 'DEEP CLEAN')  { Invoke-Sweep; Show-Summary; Wait-Key } }
            'Nuclear' { Select-Tier 3; if (Show-Confirm 'NUCLEAR' -Danger) { Invoke-Sweep; Show-Summary; Wait-Key } }
            'Custom'  {
                if (-not $customReady) { Select-Tier 2; $customReady = $true }
                if (Show-TaskPicker) {
                    if ($script:DryRun -or (Show-Confirm 'CUSTOM SWEEP')) { Invoke-Sweep; Show-Summary; Wait-Key }
                }
            }
            'Analyze' { Select-Tier 3; $script:DryRun = $true; Invoke-Sweep; Show-Summary; Wait-Key }
            'Backups' { Show-Backups }
        }
    }
    try { Clear-Host } catch {}   # throws without a real console (CI, redirected output)
    Write-Host ''
    Write-Host '  Swept clean. See you next time!' -ForegroundColor Cyan
    Write-Host ''
}

try { Start-Broom } finally { try { [Console]::CursorVisible = $true } catch {} }
#endregion
