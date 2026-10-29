#pragma once
#include <windows.h>
#include <dwrite.h>
#include <cstdint>
using LBLayoutFn=HRESULT(STDMETHODCALLTYPE*)(IDWriteFactory*,WCHAR const*,UINT32,IDWriteTextFormat*,FLOAT,FLOAT,IDWriteTextLayout**);
using LBGdiLayoutFn=HRESULT(STDMETHODCALLTYPE*)(IDWriteFactory*,WCHAR const*,UINT32,IDWriteTextFormat*,FLOAT,FLOAT,FLOAT,DWRITE_MATRIX const*,BOOL,IDWriteTextLayout**);
extern "C" {
__declspec(dllexport) int LB_SetOriginals(LBLayoutFn,LBGdiLayoutFn);
__declspec(dllexport) HRESULT STDMETHODCALLTYPE LB_CreateTextLayout(IDWriteFactory*,WCHAR const*,UINT32,IDWriteTextFormat*,FLOAT,FLOAT,IDWriteTextLayout**);
__declspec(dllexport) HRESULT STDMETHODCALLTYPE LB_CreateGdiCompatibleTextLayout(IDWriteFactory*,WCHAR const*,UINT32,IDWriteTextFormat*,FLOAT,FLOAT,FLOAT,DWRITE_MATRIX const*,BOOL,IDWriteTextLayout**);
__declspec(dllexport) std::uint64_t LB_SuccessfulLayouts();
}
