#include "Json.h"
#include <filesystem>
#include <fstream>
#include <iostream>
#include <chrono>
#include <psapi.h>
namespace fs=std::filesystem;
int wmain(int argc,wchar_t**argv){try{if(argc!=3)throw std::runtime_error("usage DLL TEST-ROOT");
 for(auto text:{"01","1.","1e","NaN","{\"x\":true,}","\"\\ud800\""}){bool rejected=false;try{lbjson::parse(text);}catch(...){rejected=true;}if(!rejected)throw std::runtime_error("malformed JSON accepted");}
 auto root=fs::absolute(argv[2]);auto status=root/L"runtime"/(L"ae-"+std::to_wstring(GetCurrentProcessId()))/L"status.json";
 PROCESS_MEMORY_COUNTERS_EX before{sizeof(before)},after{sizeof(after)};GetProcessMemoryInfo(GetCurrentProcess(),reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&before),sizeof(before));auto begin=std::chrono::steady_clock::now();auto dll=LoadLibraryW(argv[1]);if(!dll)throw std::runtime_error("DLL missing");auto start=reinterpret_cast<DWORD(WINAPI*)(void*)>(GetProcAddress(dll,"LB_Start"));auto path=root.wstring();if(!start||!start(path.data()))throw std::runtime_error("start failed");for(int i=0;i<250&&!fs::exists(status);i++)Sleep(10);std::ifstream f(status,std::ios::binary);auto s=lbjson::parse(std::string((std::istreambuf_iterator<char>(f)),{}));if(!s.at("running").boolean)throw std::runtime_error(s.at("error").text);auto elapsed=std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now()-begin).count();GetProcessMemoryInfo(GetCurrentProcess(),reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&after),sizeof(after));FILETIME ct,et,kt,ut;GetProcessTimes(GetCurrentProcess(),&ct,&et,&kt,&ut);auto ticks=[](FILETIME t){ULARGE_INTEGER v;v.LowPart=t.dwLowDateTime;v.HighPart=t.dwHighDateTime;return v.QuadPart;};auto cpu=ticks(kt)+ticks(ut);Sleep(3000);GetProcessTimes(GetCurrentProcess(),&ct,&et,&kt,&ut);auto idle=ticks(kt)+ticks(ut)-cpu;
 std::cout<<"PASS full dictionary: rules="<<s.at("rules").number<<", startup="<<elapsed<<"ms, private_delta="<<static_cast<long long>(after.PrivateUsage)-static_cast<long long>(before.PrivateUsage)<<" bytes, idle_cpu_3s="<<idle/10000.0<<"ms (local fixture only)"<<std::endl;ExitProcess(0);
 }catch(const std::exception&e){std::cerr<<"FAIL "<<e.what()<<std::endl;ExitProcess(1);}}
