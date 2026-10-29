#include "Lookup.h"
#include "Renderer.h"
#include <iostream>
#include <string>
#include <cmath>
#include <cstdlib>
void expect(bool value){if(!value)std::abort();}
std::wstring observed;FLOAT observedWidth=0,observedPixels=0;BOOL observedNatural=FALSE;DWRITE_MATRIX const*observedMatrix=nullptr;
HRESULT STDMETHODCALLTYPE sample(IDWriteFactory*,WCHAR const*s,UINT32 n,IDWriteTextFormat*,FLOAT w,FLOAT,IDWriteTextLayout**){observed.assign(s,n);observedWidth=w;return S_OK;}
HRESULT STDMETHODCALLTYPE sampleGdi(IDWriteFactory*,WCHAR const*s,UINT32 n,IDWriteTextFormat*,FLOAT w,FLOAT,FLOAT pixels,DWRITE_MATRIX const*m,BOOL natural,IDWriteTextLayout**){observed.assign(s,n);observedWidth=w;observedPixels=pixels;observedMatrix=m;observedNatural=natural;return S_OK;}
int main(){
 const wchar_t original[]=L"Intensity",translated[]=L"\x5f3a\x5ea6";LBRule rule={original,9,translated,2,2};expect(LB_SetRules(&rule,1)==0);expect(LB_SetOriginals(sample,sampleGdi)==0);
 LB_SetModes(2);expect(SUCCEEDED(LB_CreateTextLayout(nullptr,original,9,nullptr,500,100,nullptr)));expect(observed==translated&&observedWidth==500);expect(std::wstring(original)==L"Intensity");
 LB_SetModes(0);LB_CreateTextLayout(nullptr,original,9,nullptr,500,100,nullptr);expect(observed==original);
 DWRITE_MATRIX matrix={1,0,0,1,10,20};LB_SetModes(2);LB_CreateGdiCompatibleTextLayout(nullptr,original,9,nullptr,500,100,1.25f,&matrix,TRUE,nullptr);expect(observed==translated&&observedPixels==1.25f&&observedMatrix==&matrix&&observedNatural==TRUE);
 IDWriteFactory*factory=nullptr;expect(SUCCEEDED(DWriteCreateFactory(DWRITE_FACTORY_TYPE_ISOLATED,__uuidof(IDWriteFactory),reinterpret_cast<IUnknown**>(&factory))));
 auto vt=*reinterpret_cast<void***>(factory);expect(LB_SetOriginals(reinterpret_cast<LBLayoutFn>(vt[18]),reinterpret_cast<LBGdiLayoutFn>(vt[19]))==0);
 IDWriteTextFormat*format=nullptr;expect(SUCCEEDED(factory->CreateTextFormat(L"Microsoft YaHei",nullptr,DWRITE_FONT_WEIGHT_NORMAL,DWRITE_FONT_STYLE_NORMAL,DWRITE_FONT_STRETCH_NORMAL,16,L"zh-CN",&format)));
 FLOAT widths[3]={};for(int i=0;i<3;i++){LB_SetModes(i==1?2:0);IDWriteTextLayout*layout=nullptr;expect(SUCCEEDED(LB_CreateTextLayout(factory,original,9,format,500,100,&layout)));DWRITE_TEXT_METRICS metrics={};expect(SUCCEEDED(layout->GetMetrics(&metrics)));widths[i]=metrics.width;layout->Release();}
 expect(std::abs(widths[0]-widths[2])<0.001f);expect(std::abs(widths[0]-widths[1])>0.1f);
 format->Release();factory->Release();LB_SetModes(0);
 std::cout<<"PASS native callbacks preserve input/arguments, off-on-off layouts; widths "<<widths[0]<<" -> "<<widths[1]<<" -> "<<widths[2]<<"\nStandalone DirectWrite only; not injected into AE.\n";
}
