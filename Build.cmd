@echo off
rem Build the native backend and place the binaries in .\backend, which is where
rem Install-AE-Panel.ps1 expects them.
setlocal
set "HERE=%~dp0"
pushd "%HERE%"

where cargo >nul 2>nul
if errorlevel 1 (
 echo Rust toolchain not found.
 echo Install Rust 1.75+ for x86_64-pc-windows-msvc from https://rustup.rs
 popd
 exit /b 1
)

echo Building release binaries with static CRT and panic=abort...
cargo build --release --locked
if errorlevel 1 (
 echo Build failed.
 popd
 exit /b 1
)

if not exist "%HERE%backend" mkdir "%HERE%backend"
copy /y "%HERE%target\release\KosuzuNative.dll"            "%HERE%backend\" >nul
copy /y "%HERE%target\release\KosuzuController.exe"        "%HERE%backend\" >nul
copy /y "%HERE%target\release\KosuzuClipboard.exe"         "%HERE%backend\" >nul
copy /y "%HERE%target\release\KosuzuDictionaryCheck.exe"   "%HERE%backend\" >nul
copy /y "%HERE%target\release\KosuzuSelfCheck.exe"         "%HERE%backend\" >nul

echo.
echo Built into %HERE%backend:
dir /b "%HERE%backend"
echo.
echo Next: run Verify.cmd to check behaviour, then Install.cmd to install into
echo After Effects. Close After Effects before installing.
popd
endlocal
