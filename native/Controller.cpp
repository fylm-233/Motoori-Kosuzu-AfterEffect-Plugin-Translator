#include "Json.h"
#include <tlhelp32.h>
#include <filesystem>
#include <fstream>
#include <vector>
#include <stdexcept>
namespace fs=std::filesystem;
struct Handle{HANDLE value=nullptr;explicit Handle(HANDLE h):value(h){}~Handle(){if(value&&value!=INVALID_HANDLE_VALUE)CloseHandle(value);}operator HANDLE()const{return value;}};
struct Remote{HANDLE process;void*pointer;Remote(HANDLE p,const std::wstring&s):process(p){pointer=VirtualAllocEx(p,nullptr,(s.size()+1)*2,MEM_COMMIT|MEM_RESERVE,PAGE_READWRITE);SIZE_T written;if(!pointer||!WriteProcessMemory(p,pointer,s.c_str(),(s.size()+1)*2,&written)){if(pointer)VirtualFreeEx(p,pointer,0,MEM_RELEASE);throw std::runtime_error("Cannot pass native path to AE");}}~Remote(){if(pointer)VirtualFreeEx(process,pointer,0,MEM_RELEASE);}};
std::uintptr_t module(DWORD pid,const std::wstring&name,bool full=false){Handle snap(CreateToolhelp32Snapshot(TH32CS_SNAPMODULE|TH32CS_SNAPMODULE32,pid));MODULEENTRY32W entry{sizeof(entry)};if(Module32FirstW(snap,&entry))do{if(!_wcsicmp(full?entry.szExePath:entry.szModule,name.c_str()))return reinterpret_cast<std::uintptr_t>(entry.modBaseAddr);}while(Module32NextW(snap,&entry));return 0;}
DWORD run(HANDLE process,std::uintptr_t fn,Remote&arg){Handle thread(CreateRemoteThread(process,nullptr,0,reinterpret_cast<LPTHREAD_START_ROUTINE>(fn),arg.pointer,0,nullptr));if(!thread.value)throw std::runtime_error("AE blocked native startup");if(WaitForSingleObject(thread,15000)!=WAIT_OBJECT_0){arg.pointer=nullptr;throw std::runtime_error("Native startup timed out; restart AE before retry");}DWORD code=0;if(!GetExitCodeThread(thread,&code))throw std::runtime_error("Cannot read startup result");return code;}
int WINAPI wWinMain(HINSTANCE,HINSTANCE,PWSTR,int){wchar_t path[32768];GetModuleFileNameW(nullptr,path,32768);fs::path root=fs::path(path).parent_path().parent_path();fs::path runtime=root/L"runtime";DWORD pid=0;std::string error;try{
 fs::create_directories(runtime);std::vector<DWORD> found;Handle snapshot(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS,0));PROCESSENTRY32W entry{sizeof(entry)};if(Process32FirstW(snapshot,&entry))do{if(!_wcsicmp(entry.szExeFile,L"AfterFX.exe"))found.push_back(entry.th32ProcessID);}while(Process32NextW(snapshot,&entry));
 if(found.size()!=1)throw std::runtime_error(found.empty()?"Open After Effects first":"Multiple AE processes found; open one AE process for this test build");pid=found[0];
 Handle process(OpenProcess(PROCESS_CREATE_THREAD|PROCESS_QUERY_INFORMATION|PROCESS_VM_OPERATION|PROCESS_VM_WRITE|PROCESS_VM_READ,FALSE,pid));if(!process.value)throw std::runtime_error("Cannot access AE; run AE with the same Windows account and privilege level");
 auto dll=root/L"backend"/L"KosuzuNative.dll";if(!fs::exists(dll))throw std::runtime_error("Native engine missing; reinstall full package");
 auto remoteBase=module(pid,dll.wstring(),true);
 // Never install a second version over a live native engine.
 auto any=module(pid,L"KosuzuNative.dll");if(any&&!remoteBase)throw std::runtime_error("Another Kosuzu version is loaded; restart AE after updating");
 if(!remoteBase){auto load=reinterpret_cast<void*>(GetProcAddress(GetModuleHandleW(L"kernel32.dll"),"LoadLibraryW"));HMODULE owner=nullptr;if(!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS|GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,static_cast<LPCWSTR>(load),&owner))throw std::runtime_error("Cannot resolve Windows loader");wchar_t ownerPath[32768];GetModuleFileNameW(owner,ownerPath,32768);auto remoteOwner=module(pid,fs::path(ownerPath).filename().wstring());if(!remoteOwner)throw std::runtime_error("Windows loader module unavailable");Remote argument(process,dll.wstring());run(process,remoteOwner+(reinterpret_cast<std::uintptr_t>(load)-reinterpret_cast<std::uintptr_t>(owner)),argument);remoteBase=module(pid,dll.wstring(),true);if(!remoteBase)throw std::runtime_error("AE could not load native engine");}
 // Map exports without running DLL initialization in the controller.
 auto local=LoadLibraryExW(dll.c_str(),nullptr,DONT_RESOLVE_DLL_REFERENCES);if(!local)throw std::runtime_error("Cannot read native exports");auto start=GetProcAddress(local,"LB_Start");auto offset=reinterpret_cast<std::uintptr_t>(start)-reinterpret_cast<std::uintptr_t>(local);FreeLibrary(local);if(!start)throw std::runtime_error("Native entry point missing");Remote argument(process,root.wstring());if(!run(process,remoteBase+offset,argument))throw std::runtime_error("Native worker failed to start");
 }catch(const std::exception&e){error=e.what();}
 try{FILETIME time;GetSystemTimeAsFileTime(&time);ULARGE_INTEGER tick;tick.LowPart=time.dwLowDateTime;tick.HighPart=time.dwHighDateTime;auto temp=runtime/L"connection.tmp";{std::ofstream f(temp,std::ios::binary);f<<"{\"created\":"<<(tick.QuadPart/10000-11644473600000ULL)<<",\"pid\":"<<pid<<",\"ok\":"<<(error.empty()?"true":"false")<<",\"error\":"<<lbjson::quote(error)<<"}";}MoveFileExW(temp.c_str(),(runtime/L"connection.json").c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH);}catch(...){}
 return error.empty()?0:1;
}
