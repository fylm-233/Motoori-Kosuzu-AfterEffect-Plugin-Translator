#include "Lookup.h"
#include <iostream>
#include <chrono>
#include <thread>
#include <atomic>
#include <string>
#include <cstdlib>
void check(bool result){if(!result)std::abort();}
int main(){
 LBRule rules[]={{L"Intensity",9,L"Strength",8,2},{L"Effect",6,L"Name",4,1}};
 check(LB_SetRules(rules,2)==0);wchar_t out[64]={};
 LB_SetModes(0);check(LB_Translate(L"Intensity",9,out,64)==0);
 LB_SetModes(2);check(LB_Translate(L"Intensity",9,out,64)==1&&std::wstring(out)==L"Strength");
 check(LB_Translate(L"Effect",6,out,64)==0);check(LB_Translate(L"intensity",9,out,64)==0);check(LB_Translate(L"Intensity",9,out,2)==3);
 LB_SetModes(1);check(LB_Translate(L"Effect",6,out,64)==1);check(LB_Translate(L"Intensity",9,out,64)==0);
 LB_SetModes(2);std::atomic<bool>running{true};std::atomic<int>errors{0};
 std::thread reader([&]{wchar_t b[64];while(running){if(LB_Translate(L"Intensity",9,b,64)!=1)errors++;}});
 for(int i=0;i<1000;i++)check(LB_SetRules(rules,2)==0);running=false;reader.join();check(errors==0);
 constexpr int iterations=1000000;auto start=std::chrono::steady_clock::now();int total=0;
 for(int i=0;i<iterations;i++)total+=LB_Translate(L"Intensity",9,out,64);
 auto us=std::chrono::duration_cast<std::chrono::microseconds>(std::chrono::steady_clock::now()-start).count();
 check(total==iterations);std::cout<<"PASS native modes, exact matching, buffers, 1000 concurrent reloads\n"<<iterations<<" local lookups: "<<us<<" us (not an AE or reference-plugin benchmark)\n";
}
