#include "Lookup.h"
#include <atomic>
#include <memory>
#include <string>
#include <string_view>
#include <unordered_map>
#include <algorithm>
#include <cwchar>
#include <vector>
#include <mutex>
namespace {
struct Hash{using is_transparent=void;size_t operator()(std::wstring_view s)const noexcept{return std::hash<std::wstring_view>{}(s);}};
struct Equal{using is_transparent=void;bool operator()(std::wstring_view a,std::wstring_view b)const noexcept{return a==b;}};
struct Entry{std::wstring name,param;};
using Table=std::unordered_map<std::wstring,Entry,Hash,Equal>;
std::shared_ptr<const Table> active=std::make_shared<Table>();std::atomic<std::uint32_t> modes{0};
std::mutex writer;std::vector<std::shared_ptr<const Table>> retired;
}
int LB_SetRules(const LBRule* rules,std::uint32_t count){
 if((count&&!rules)||count>100000)return -1;
 try{auto next=std::make_shared<Table>();next->reserve(count);
 for(std::uint32_t i=0;i<count;i++){const auto&r=rules[i];if(!r.source||!r.target||!r.sourceLength||!r.targetLength||r.sourceLength>8192||r.targetLength>8192||(r.category!=1&&r.category!=2))return -1;
  if(std::wmemchr(r.source,0,r.sourceLength)||std::wmemchr(r.target,0,r.targetLength))return -1;
  auto&entry=(*next)[std::wstring(r.source,r.sourceLength)];auto value=std::wstring(r.target,r.targetLength);if(r.category==1)entry.name=std::move(value);else entry.param=std::move(value);
 }std::lock_guard guard(writer);retired.push_back(std::atomic_load_explicit(&active,std::memory_order_acquire));std::atomic_store_explicit(&active,std::shared_ptr<const Table>(next),std::memory_order_release);
 // Keep old snapshots alive until the writer can reclaim them, never in a paint callback.
 std::erase_if(retired,[](const auto&snapshot){return snapshot.use_count()==1;});return 0;
 }catch(...){return -1;}
}
void LB_SetModes(std::uint32_t value){modes.store(value&3,std::memory_order_release);}
int LB_Translate(const wchar_t* source,std::uint32_t length,wchar_t* output,std::uint32_t capacity){
 const auto enabled=modes.load(std::memory_order_acquire);if(!enabled)return 0;
 if(!source||length>8192||!output||output==source)return -1;
 auto snapshot=std::atomic_load_explicit(&active,std::memory_order_acquire);
 auto found=snapshot->find(std::wstring_view(source,length));if(found==snapshot->end())return 0;
 const auto&e=found->second;if(!e.name.empty()&&!e.param.empty()&&e.name!=e.param)return 2;
 const std::wstring*value=nullptr;if((enabled&1)&&!e.name.empty())value=&e.name;else if((enabled&2)&&!e.param.empty())value=&e.param;if(!value)return 0;
 if(value->size()+1>capacity)return 3;std::copy(value->begin(),value->end(),output);output[value->size()]=0;return 1;
}
