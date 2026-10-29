#include "Json.h"
#include <dwrite.h>
#include <psapi.h>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <thread>
#include <atomic>
#include <chrono>
namespace fs=std::filesystem;
void check(bool ok,const char*message){if(!ok)throw std::runtime_error(message);}
void write(const fs::path&p,const std::string&s){std::ofstream f(p,std::ios::binary);f<<s;check(f.good(),"write");}
lbjson::Value read(const fs::path&p){std::ifstream f(p,std::ios::binary);return lbjson::parse(std::string((std::istreambuf_iterator<char>(f)),{}));}
std::uint64_t now(){FILETIME t;GetSystemTimeAsFileTime(&t);ULARGE_INTEGER n;n.LowPart=t.dwLowDateTime;n.HighPart=t.dwHighDateTime;return n.QuadPart/10000-11644473600000ULL;}
double width(IDWriteFactory*f,IDWriteTextFormat*format){IDWriteTextLayout*layout=nullptr;check(SUCCEEDED(f->CreateTextLayout(L"Intensity",9,format,1000,100,&layout)),"create layout");DWRITE_TEXT_METRICS m{};check(SUCCEEDED(layout->GetMetrics(&m)),"metrics");layout->Release();return m.width;}
int wmain(int argc,wchar_t**argv){try{check(argc==3,"usage BackendCheck DLL TEST-ROOT");fs::path root=fs::absolute(argv[2]);fs::create_directories(root/L"dictionaries");
 for(auto kind:{"names","parameters"}){write(root/L"dictionaries"/lbjson::wide(std::string("Kosuzu.base.")+kind+".json"),kind==std::string("parameters")?"{\"Intensity\":\"强度\"}":"{}");write(root/L"dictionaries"/lbjson::wide(std::string("Kosuzu.")+kind+".json"),"{}");}
 IDWriteFactory*f=nullptr;check(SUCCEEDED(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,__uuidof(IDWriteFactory),reinterpret_cast<IUnknown**>(&f))),"factory");IDWriteTextFormat*format=nullptr;check(SUCCEEDED(f->CreateTextFormat(L"Microsoft YaHei",nullptr,DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,16,L"zh-cn",&format)),"format");auto original=width(f,format);
 PROCESS_MEMORY_COUNTERS_EX before{sizeof(before)},after{sizeof(after)};GetProcessMemoryInfo(GetCurrentProcess(),reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&before),sizeof(before));auto begin=std::chrono::steady_clock::now();
 auto dll=LoadLibraryW(argv[1]);check(dll,"load native DLL");auto start=reinterpret_cast<DWORD(WINAPI*)(void*)>(GetProcAddress(dll,"LB_Start"));auto path=root.wstring();check(start&&start(path.data()),"start worker");auto runtime=root/L"runtime"/(L"ae-"+std::to_wstring(GetCurrentProcessId()));
 for(int i=0;i<100&&!fs::exists(runtime/L"status.json");i++)Sleep(20);auto s=read(runtime/L"status.json");check(s.at("running").boolean&&s.at("rules").number==1,"initial load");auto elapsed=std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::steady_clock::now()-begin).count();
 int seq=0;auto command=[&](const std::string&method,const std::string&state="null",bool expected=true){auto id="test-"+std::to_string(++seq);auto req=runtime/L"requests"/lbjson::wide(id+".json");auto temp=req;temp+=L".tmp";write(temp,"{\"id\":\""+id+"\",\"created\":"+std::to_string(now())+",\"method\":"+lbjson::quote(method)+",\"state\":"+state+"}");fs::rename(temp,req);auto resp=runtime/L"responses"/lbjson::wide(id+".json");for(int i=0;i<100&&!fs::exists(resp);i++)Sleep(20);auto answer=read(resp);check(answer.at("ok").boolean==expected,"command result");fs::remove(resp);return answer;};
 command("set","{\"names\":false,\"parameters\":true}");auto translated=width(f,format);check(translated!=original,"actual shared factory translation");
 write(root/L"dictionaries"/L"Kosuzu.parameters.json","{broken");command("reload","null",false);check(width(f,format)==translated,"invalid reload preserves old snapshot");write(root/L"dictionaries"/L"Kosuzu.parameters.json","{\"Intensity\":\"强度调整\"}");command("reload");check(width(f,format)!=translated,"custom override reload");
 std::atomic<bool> bad{false};std::vector<std::thread> readers;for(int i=0;i<4;i++)readers.emplace_back([&]{for(int j=0;j<500;j++){try{check(width(f,format)>0,"positive width");}catch(...){bad=true;}}});for(int i=0;i<10;i++)command("reload");for(auto&t:readers)t.join();check(!bad,"concurrent renderer reload");
 command("stop");check(width(f,format)==original,"stop restores original");command("set","{\"names\":false,\"parameters\":true}");check(width(f,format)!=original,"restart after stop");command("set","{\"names\":false,\"parameters\":false}");check(width(f,format)==original,"off restores original");
 GetProcessMemoryInfo(GetCurrentProcess(),reinterpret_cast<PROCESS_MEMORY_COUNTERS*>(&after),sizeof(after));std::cout<<"PASS shared DirectWrite hook, original/translated/off widths="<<original<<"/"<<translated<<"/"<<width(f,format)<<", startup="<<elapsed<<"ms, private_delta="<<static_cast<long long>(after.PrivateUsage)-static_cast<long long>(before.PrivateUsage)<<" bytes; invalid reload rollback, custom override, 2000 layouts during 10 reloads, stop/re-enable\n";std::cout.flush();ExitProcess(0);
 }catch(const std::exception&e){std::cerr<<"FAIL "<<e.what()<<std::endl;ExitProcess(1);}}
