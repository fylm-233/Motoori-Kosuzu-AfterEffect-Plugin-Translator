#include "Json.h"
#include "Lookup.h"
#include "Renderer.h"
#include <dwrite.h>
#include <atomic>
#include <filesystem>
#include <fstream>
#include <vector>
#include <algorithm>
namespace fs=std::filesystem;
namespace {
HMODULE selfModule=nullptr;std::atomic<bool> started{false};
std::uint32_t mode=0;std::uint64_t generation=0;std::string lastError;size_t ruleCount=0;
IDWriteFactory* factory=nullptr;void**table=nullptr;void*savedLayout=nullptr;void*savedGdi=nullptr;bool hooked=false;
fs::path root,runtime;
std::uint64_t now(){FILETIME t;GetSystemTimeAsFileTime(&t);ULARGE_INTEGER n;n.LowPart=t.dwLowDateTime;n.HighPart=t.dwHighDateTime;return n.QuadPart/10000-11644473600000ULL;}
std::string read(const fs::path&p,size_t max=10*1024*1024){if(fs::file_size(p)>max)throw std::runtime_error("File too large");std::ifstream f(p,std::ios::binary);if(!f)throw std::runtime_error("Cannot read file");return {(std::istreambuf_iterator<char>(f)),{}};}
void write(const fs::path&p,const std::string&s){auto temp=p;temp+=L".native.tmp";{std::ofstream f(temp,std::ios::binary|std::ios::trunc);if(!f||!f.write(s.data(),s.size()))throw std::runtime_error("Cannot write response");}if(!MoveFileExW(temp.c_str(),p.c_str(),MOVEFILE_REPLACE_EXISTING|MOVEFILE_WRITE_THROUGH))throw std::runtime_error("Cannot commit response");}
std::string result(bool running=true){return "{\"running\":"+std::string(running?"true":"false")+",\"pid\":"+std::to_string(GetCurrentProcessId())+",\"heartbeat\":"+std::to_string(now())+",\"engine\":\"native-dwrite-0.8\",\"state\":{\"names\":"+((mode&1)?"true":"false")+",\"parameters\":"+((mode&2)?"true":"false")+"},\"rules\":"+std::to_string(ruleCount)+",\"generation\":"+std::to_string(generation)+",\"hits\":"+std::to_string(LB_SuccessfulLayouts())+",\"error\":"+lbjson::quote(lastError)+"}";}
void load(){
 struct TextRule{std::wstring source,target;std::uint32_t kind;};std::vector<TextRule> texts;
 for(auto kind:{std::string("names"),std::string("parameters")}){
  std::map<std::string,std::string> merged;
  for(bool base:{true,false}){auto map=lbjson::parse(read(root/L"dictionaries"/lbjson::wide("Kosuzu."+std::string(base?"base.":"")+kind+".json")));if(map.type!='o')throw std::runtime_error("Dictionary must be an object");
   for(auto&[key,value]:map.object){if(key.empty()||key=="__proto__"||key=="constructor"||key=="prototype"||value.type!='s')throw std::runtime_error("Invalid dictionary entry");if(!value.text.empty())merged[key]=value.text;}
  }
  for(auto&[source,target]:merged){auto a=lbjson::wide(source),b=lbjson::wide(target);if(a==b||b.find_first_not_of(L" \r\n\t")==std::wstring::npos)continue;texts.push_back({std::move(a),std::move(b),kind=="names"?1u:2u});}
 }
 std::vector<LBRule> rules;for(auto&t:texts)rules.push_back({t.source.data(),(std::uint32_t)t.source.size(),t.target.data(),(std::uint32_t)t.target.size(),t.kind});
 if(LB_SetRules(rules.data(),(std::uint32_t)rules.size()))throw std::runtime_error("Invalid dictionary text or length");ruleCount=rules.size();generation++;
}
void hook(){
 if(hooked)return;
 if(FAILED(DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED,__uuidof(IDWriteFactory),reinterpret_cast<IUnknown**>(&factory))))throw std::runtime_error("DirectWrite unavailable");
 table=*reinterpret_cast<void***>(factory);savedLayout=table[18];savedGdi=table[19];
 if(LB_SetOriginals(reinterpret_cast<LBLayoutFn>(savedLayout),reinterpret_cast<LBGdiLayoutFn>(savedGdi)))throw std::runtime_error("Invalid original renderer");
 DWORD old;if(!VirtualProtect(table+18,2*sizeof(void*),PAGE_READWRITE,&old))throw std::runtime_error("Cannot enable renderer");
 InterlockedExchangePointer(table+18,reinterpret_cast<void*>(LB_CreateTextLayout));InterlockedExchangePointer(table+19,reinterpret_cast<void*>(LB_CreateGdiCompatibleTextLayout));DWORD unused;VirtualProtect(table+18,2*sizeof(void*),old,&unused);hooked=true;
}
void unhook(){LB_SetModes(0);mode=0;if(!hooked)return;DWORD old;if(VirtualProtect(table+18,2*sizeof(void*),PAGE_READWRITE,&old)){auto layout=InterlockedCompareExchangePointer(table+18,savedLayout,reinterpret_cast<void*>(LB_CreateTextLayout));auto gdi=InterlockedCompareExchangePointer(table+19,savedGdi,reinterpret_cast<void*>(LB_CreateGdiCompatibleTextLayout));DWORD unused;VirtualProtect(table+18,2*sizeof(void*),old,&unused);hooked=layout!=reinterpret_cast<void*>(LB_CreateTextLayout)||gdi!=reinterpret_cast<void*>(LB_CreateGdiCompatibleTextLayout);}}
BOOL CALLBACK repaint(HWND h,LPARAM){DWORD pid;GetWindowThreadProcessId(h,&pid);if(pid==GetCurrentProcessId()&&IsWindowVisible(h)){InvalidateRect(h,nullptr,FALSE);RedrawWindow(h,nullptr,nullptr,RDW_INVALIDATE|RDW_ALLCHILDREN);}return TRUE;}
void requests(){size_t handled=0;for(const auto&entry:fs::directory_iterator(runtime/L"requests")){
 if(handled>=8)break;if(!entry.is_regular_file()||entry.path().extension()!=L".json")continue;handled++;
 std::string id=entry.path().stem().string();if(id.empty()||id.size()>100||id.find_first_not_of("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-")!=std::string::npos)continue;
 bool ok=false;std::string error;
 try{auto req=lbjson::parse(read(entry.path(),64*1024));if(req.at("id").type!='s'||req.at("id").text!=id)throw std::runtime_error("Request id mismatch");const auto&created=req.at("created");if(created.type!='n'||std::abs((double)now()-created.number)>30000)throw std::runtime_error("Expired request");auto method=req.at("method");if(method.type!='s')throw std::runtime_error("Invalid method");
  if(method.text=="reload"){load();lastError.clear();}
  else if(method.text=="set"){const auto&s=req.at("state");auto&n=s.at("names");auto&p=s.at("parameters");if(n.type!='b'||p.type!='b')throw std::runtime_error("Modes must be booleans");auto next=(n.boolean?1u:0u)|(p.boolean?2u:0u);if(next)hook();mode=next;LB_SetModes(mode);EnumWindows(repaint,0);}
  else if(method.text=="stop"){unhook();EnumWindows(repaint,0);}
  else if(method.text!="status")throw std::runtime_error("Unknown method");ok=true;
 }catch(const std::exception&e){error=e.what();}
 write(runtime/L"responses"/lbjson::wide(id+".json"),"{\"id\":"+lbjson::quote(id)+",\"ok\":"+(ok?"true":"false")+",\"error\":"+lbjson::quote(error)+",\"result\":"+result()+"}");std::error_code ec;fs::remove(entry.path(),ec);
}}
DWORD WINAPI worker(void*argument){root=static_cast<const wchar_t*>(argument);delete[] static_cast<wchar_t*>(argument);runtime=root/L"runtime"/(L"ae-"+std::to_wstring(GetCurrentProcessId()));try{
 fs::create_directories(runtime/L"requests");fs::create_directories(runtime/L"responses");load();std::uint64_t nextHeartbeat=0;
 for(;;){try{requests();if(now()>=nextHeartbeat){write(runtime/L"status.json",result());nextHeartbeat=now()+1000;}}catch(const std::exception&e){lastError=e.what();}Sleep(100);}
 }catch(const std::exception&e){lastError=e.what();try{write(runtime/L"status.json",result(false));}catch(...){}}
 return 0;
}
}
extern "C" __declspec(dllexport) DWORD WINAPI LB_Start(void*argument){if(!argument)return 0;if(started.exchange(true))return 1;try{auto s=static_cast<const wchar_t*>(argument);size_t length=wcsnlen_s(s,32768);if(length>=32768)throw 1;auto copy=new wchar_t[length+1];memcpy(copy,s,(length+1)*sizeof(wchar_t));HANDLE thread=CreateThread(nullptr,0,worker,copy,0,nullptr);if(!thread){delete[]copy;throw 1;}CloseHandle(thread);return 1;}catch(...){started=false;return 0;}}
BOOL WINAPI DllMain(HINSTANCE instance,DWORD reason,LPVOID){if(reason==DLL_PROCESS_ATTACH){selfModule=instance;DisableThreadLibraryCalls(instance);}return TRUE;}
