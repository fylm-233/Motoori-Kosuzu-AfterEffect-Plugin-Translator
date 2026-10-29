@echo off
rem Unit tests plus the four end-to-end checks that used to live in separate
rem C++ test programs (LookupCheck / RendererCheck / BackendCheck / CapacityCheck).
rem Usage: Verify.cmd [path-to-KosuzuNative.dll] [package-root-with-dictionaries]
setlocal enabledelayedexpansion
set "HERE=%~dp0"
set "DLL=%~1"
if "%DLL%"=="" set "DLL=%HERE%backend\KosuzuNative.dll"
set "PKGROOT=%~2"
if "%PKGROOT%"=="" set "PKGROOT=%HERE%"

if not exist "%DLL%" (
 echo Native DLL not found: %DLL%
 echo Run Build.cmd first.
 exit /b 1
)
if not exist "%PKGROOT%\dictionaries" (
 echo No dictionaries folder under: %PKGROOT%
 echo Pass the package root as the second argument.
 exit /b 1
)

set "CHECK=%HERE%backend\KosuzuSelfCheck.exe"
if not exist "%CHECK%" set "CHECK=%HERE%target\release\KosuzuSelfCheck.exe"

set "TMPROOT=%TEMP%\kosuzu-verify-%RANDOM%"
set "FAILED=0"

echo === unit tests: kosuzu-core ===
pushd "%HERE%"
cargo test --release -p kosuzu-core --quiet
if errorlevel 1 set "FAILED=1"
popd

echo.
echo === lookup ===
"%CHECK%" lookup "%DLL%"
if errorlevel 1 set "FAILED=1"

echo === renderer ===
"%CHECK%" renderer "%DLL%"
if errorlevel 1 set "FAILED=1"

echo === backend ===
"%CHECK%" backend "%DLL%" "%TMPROOT%\backend"
if errorlevel 1 set "FAILED=1"

echo === capacity: real dictionary copied to a temp root ===
mkdir "%TMPROOT%\capacity" 2>nul
xcopy /e /i /q /y "%PKGROOT%\dictionaries" "%TMPROOT%\capacity\dictionaries" >nul
"%CHECK%" capacity "%DLL%" "%TMPROOT%\capacity"
if errorlevel 1 set "FAILED=1"

rmdir /s /q "%TMPROOT%" 2>nul

echo.
if "%FAILED%"=="1" (
 echo RESULT: FAILED
 exit /b 1
)
echo RESULT: all checks passed against %DLL%
endlocal
