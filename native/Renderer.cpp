#include "Lookup.h"
#include "Renderer.h"
#include <atomic>
#include <cwchar>
namespace {
std::atomic<LBLayoutFn> originalLayout{nullptr};std::atomic<LBGdiLayoutFn>originalGdi{nullptr};std::atomic<std::uint64_t>success{0};
}
int LB_SetOriginals(LBLayoutFn layout,LBGdiLayoutFn gdi){if(!layout||!gdi||layout==LB_CreateTextLayout||gdi==LB_CreateGdiCompatibleTextLayout)return -1;originalLayout.store(layout,std::memory_order_release);originalGdi.store(gdi,std::memory_order_release);return 0;}
HRESULT STDMETHODCALLTYPE LB_CreateTextLayout(IDWriteFactory* factory,WCHAR const*text,UINT32 length,IDWriteTextFormat*format,FLOAT width,FLOAT height,IDWriteTextLayout**output){
 auto original=originalLayout.load(std::memory_order_acquire);if(!original)return E_UNEXPECTED;
 wchar_t translated[8193];const int result=LB_Translate(text,length,translated,8193);
 const HRESULT hr=original(factory,result==1?translated:text,result==1?static_cast<UINT32>(std::wcslen(translated)):length,format,width,height,output);
 if(result==1&&SUCCEEDED(hr))success.fetch_add(1,std::memory_order_relaxed);return hr;
}
HRESULT STDMETHODCALLTYPE LB_CreateGdiCompatibleTextLayout(IDWriteFactory*factory,WCHAR const*text,UINT32 length,IDWriteTextFormat*format,FLOAT width,FLOAT height,FLOAT pixels,DWRITE_MATRIX const*matrix,BOOL natural,IDWriteTextLayout**output){
 auto original=originalGdi.load(std::memory_order_acquire);if(!original)return E_UNEXPECTED;
 wchar_t translated[8193];const int result=LB_Translate(text,length,translated,8193);
 const HRESULT hr=original(factory,result==1?translated:text,result==1?static_cast<UINT32>(std::wcslen(translated)):length,format,width,height,pixels,matrix,natural,output);
 if(result==1&&SUCCEEDED(hr))success.fetch_add(1,std::memory_order_relaxed);return hr;
}
std::uint64_t LB_SuccessfulLayouts(){return success.load(std::memory_order_relaxed);}
