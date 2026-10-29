#pragma once
#include <cstdint>
struct LBRule {const wchar_t* source;std::uint32_t sourceLength;const wchar_t* target;std::uint32_t targetLength;std::uint32_t category;};
extern "C" {
__declspec(dllexport) int LB_SetRules(const LBRule*,std::uint32_t);
__declspec(dllexport) void LB_SetModes(std::uint32_t);
// 0 original, 1 translated, 2 ambiguous, 3 buffer too small, -1 invalid input.
__declspec(dllexport) int LB_Translate(const wchar_t*,std::uint32_t,wchar_t*,std::uint32_t);
}
