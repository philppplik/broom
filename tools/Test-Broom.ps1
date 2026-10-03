<#
    Static checks for Broom. Used by CI and before every PR.
      1. PowerShell parser: zero syntax errors
      2. Broom.ps1 is pure ASCII (Windows PowerShell 5.1 reads BOM-less UTF-8 as ANSI)
      3. PSScriptAnalyzer: zero errors (settings in PSScriptAnalyzerSettings.psd1)
#>
[CmdletBinding()]
param([switch]$SkipAnalyzer)

$ErrorActionPreference = 'Stop'
$root   = Split-Path -Parent $PSScriptRoot
$script = Join-Path $root 'Broom.ps1'
$failed = $false

Write-Host '> Parser' -ForegroundColor Cyan
$tokens = $null; $errors = $null
[void][System.Management.Automation.Language.Parser]::ParseFile($script, [ref]$tokens, [ref]$errors)
if ($errors) { $errors | ForEach-Object { Write-Host "  L$($_.Extent.StartLineNumber): $($_.Message)" -ForegroundColor Red }; $failed = $true }
else { Write-Host '  OK' -ForegroundColor Green }

Write-Host '> ASCII only' -ForegroundColor Cyan
$bytes = [IO.File]::ReadAllBytes($script)
$bad = 0; foreach ($b in $bytes) { if ($b -gt 127) { $bad++ } }
if ($bad) { Write-Host "  $bad non-ASCII bytes found" -ForegroundColor Red; $failed = $true }
else { Write-Host '  OK' -ForegroundColor Green }

if (-not $SkipAnalyzer) {
    Write-Host '> PSScriptAnalyzer' -ForegroundColor Cyan
    if (-not (Get-Module -ListAvailable PSScriptAnalyzer)) {
        Write-Host '  not installed: Install-Module PSScriptAnalyzer -Scope CurrentUser' -ForegroundColor Yellow
    } else {
        $settings = Join-Path $root 'PSScriptAnalyzerSettings.psd1'
        $issues = @(Invoke-ScriptAnalyzer -Path $root -Recurse -Settings $settings)
        $issues | Format-Table -AutoSize Severity, RuleName, ScriptName, Line, Message | Out-String -Width 200 | Write-Host
        if (@($issues | Where-Object Severity -eq 'Error').Count) { $failed = $true }
        else { Write-Host "  OK ($($issues.Count) warnings)" -ForegroundColor Green }
    }
}

if ($failed) { exit 1 }
