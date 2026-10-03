@echo off
:: Broom launcher - double-click me. Asks for admin rights, then starts the TUI.
:: Extra arguments are passed through, e.g.:  Broom.cmd -Mode Analyze
setlocal
title Broom
if not exist "%~dp0Broom.ps1" (
    echo Broom.ps1 not found next to this file.
    pause
    exit /b 1
)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Broom.ps1" %*
endlocal
