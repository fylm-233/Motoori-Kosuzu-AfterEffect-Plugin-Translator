#include <windows.h>
#include <shellapi.h>
#include <filesystem>
#include <fstream>
#include <string>
namespace fs=std::filesystem;
int WINAPI wWinMain(HINSTANCE instance,HINSTANCE,PWSTR,int){try{
 auto resource=FindResourceW(instance,MAKEINTRESOURCEW(101),MAKEINTRESOURCEW(10));if(!resource)throw 1;auto size=SizeofResource(instance,resource);auto data=LockResource(LoadResource(instance,resource));if(!data||!size)throw 1;
 wchar_t temporary[32768];if(!GetTempPathW(32768,temporary))throw 1;auto root=fs::path(temporary)/(L"Kosuzu-0.8-"+std::to_wstring(GetCurrentProcessId())+L"-"+std::to_wstring(GetTickCount64()));fs::create_directories(root);auto zip=root/L"payload.zip";{std::ofstream file(zip,std::ios::binary);if(!file.write(static_cast<const char*>(data),size))throw 1;}
 auto script=root/L"bootstrap.ps1";{std::ofstream file(script,std::ios::binary);file<<"$ErrorActionPreference='Stop'\r\ntry{Expand-Archive -LiteralPath (Join-Path $PSScriptRoot 'payload.zip') -DestinationPath (Join-Path $PSScriptRoot 'files'); & (Join-Path $PSScriptRoot 'files\\Install-AE-Panel.ps1'); exit $LASTEXITCODE}catch{Add-Type -AssemblyName System.Windows.Forms; [Windows.Forms.MessageBox]::Show($_.Exception.Message,'Kosuzu Setup'); exit 1}\r\n";if(!file)throw 1;}
 wchar_t system[32768];GetSystemDirectoryW(system,32768);auto powershell=fs::path(system)/L"WindowsPowerShell"/L"v1.0"/L"powershell.exe";std::wstring command=L"\""+powershell.wstring()+L"\" -STA -NoProfile -ExecutionPolicy Bypass -File \""+script.wstring()+L"\"";STARTUPINFOW startup{sizeof(startup)};startup.dwFlags=0;startup.wShowWindow=SW_SHOWNORMAL;PROCESS_INFORMATION process{};if(!CreateProcessW(powershell.c_str(),command.data(),nullptr,nullptr,FALSE,CREATE_NO_WINDOW,nullptr,root.c_str(),&startup,&process))throw 1;WaitForSingleObject(process.hProcess,INFINITE);DWORD code=1;GetExitCodeProcess(process.hProcess,&code);CloseHandle(process.hThread);CloseHandle(process.hProcess);return code;
 }catch(...){MessageBoxW(nullptr,L"Cannot extract the offline package. Check free disk space and the complete installer file.",L"Kosuzu Setup",MB_OK|MB_ICONERROR);return 1;}}
