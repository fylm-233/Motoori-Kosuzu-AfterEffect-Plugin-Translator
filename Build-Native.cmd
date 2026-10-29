@echo off
setlocal
set "LB_SOURCE=%~dp0native"
set "LB_BACKEND=%~dp0backend"
set "LB_VSWHERE=%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe"
if not exist "%LB_VSWHERE%" (
 echo This source-build step requires Visual Studio C++ tools. End users do not need to run it.
 exit /b 1
)
for /f "usebackq tokens=*" %%i in (`"%LB_VSWHERE%" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "LB_VS=%%i"
call "%LB_VS%\VC\Auxiliary\Build\vcvars64.bat"
if errorlevel 1 exit /b 1
set "LB_BUILD=%TEMP%\Kosuzu-build-%RANDOM%"
mkdir "%LB_BUILD%"
pushd "%LB_BUILD%"
cl /nologo /O2 /MT /EHsc /utf-8 /std:c++20 "%LB_SOURCE%\DictionaryCheck.cpp" /Fe:"%LB_BACKEND%\KosuzuDictionaryCheck.exe"
if errorlevel 1 exit /b 1
cl /nologo /O2 /MT /EHsc /utf-8 /std:c++20 "%LB_SOURCE%\Clipboard.cpp" /Fe:"%LB_BACKEND%\KosuzuClipboard.exe" /link /SUBSYSTEM:WINDOWS shell32.lib user32.lib
if errorlevel 1 exit /b 1
cl /nologo /O2 /MT /EHsc /utf-8 /std:c++20 /LD "%LB_SOURCE%\Lookup.cpp" "%LB_SOURCE%\Renderer.cpp" "%LB_SOURCE%\Backend.cpp" /link /OUT:"%LB_BACKEND%\KosuzuNative.dll" dwrite.lib user32.lib
if errorlevel 1 exit /b 1
cl /nologo /O2 /MT /EHsc /utf-8 /std:c++20 "%LB_SOURCE%\Controller.cpp" /Fe:"%LB_BACKEND%\KosuzuController.exe" /link /SUBSYSTEM:WINDOWS
if errorlevel 1 exit /b 1
popd
echo Native offline runtime built successfully.
