@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0Install-AE-Panel.ps1"
if errorlevel 1 (
 echo Install failed or administrator approval was cancelled.
 pause
 exit /b 1
)
echo Kosuzu panel installed.
pause
