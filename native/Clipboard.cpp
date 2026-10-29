#include <windows.h>
#include <shellapi.h>
#include <fstream>
#include <filesystem>
#include <string>
std::string utf8(const std::wstring&s){if(s.empty())return {};int n=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),nullptr,0,nullptr,nullptr);if(!n)throw 1;std::string out(n,0);WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),out.data(),n,nullptr,nullptr);return out;}
std::wstring wide(const std::string&s){if(s.empty())return {};int n=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,s.data(),(int)s.size(),nullptr,0);if(!n)throw 1;std::wstring out(n,0);MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,s.data(),(int)s.size(),out.data(),n);return out;}
int WINAPI wWinMain(HINSTANCE,HINSTANCE,PWSTR,int){int argc=0;auto argv=CommandLineToArgvW(GetCommandLineW(),&argc);if(argc!=4)return 1;bool ok=false;HWND owner=nullptr;bool opened=false;try{
 auto data=std::filesystem::path(argv[2]),status=std::filesystem::path(argv[3]);
 owner=CreateWindowExW(0,L"STATIC",L"Kosuzu Clipboard",0,0,0,0,0,HWND_MESSAGE,nullptr,GetModuleHandleW(nullptr),nullptr);
 for(int i=0;i<5;i++){if(OpenClipboard(owner)){opened=true;break;}Sleep(15);}if(!opened)throw 1;
 if(std::wstring(argv[1])==L"copy"){
  if(std::filesystem::file_size(data)>10*1024*1024)throw 1;std::ifstream f(data,std::ios::binary);std::string bytes((std::istreambuf_iterator<char>(f)),{});auto text=wide(bytes);if(!text.empty()&&text[0]==0xfeff)text.erase(0,1);
  HGLOBAL memory=GlobalAlloc(GMEM_MOVEABLE,(text.size()+1)*sizeof(wchar_t));if(!memory)throw 1;auto pointer=GlobalLock(memory);if(!pointer){GlobalFree(memory);throw 1;}memcpy(pointer,text.c_str(),(text.size()+1)*sizeof(wchar_t));GlobalUnlock(memory);
  if(!EmptyClipboard()||!SetClipboardData(CF_UNICODETEXT,memory)){GlobalFree(memory);throw 1;}ok=true;
 }else if(std::wstring(argv[1])==L"paste"){
  HANDLE h=GetClipboardData(CF_UNICODETEXT);if(!h)throw 1;auto bytes=GlobalSize(h);if(bytes<2||bytes>10*1024*1024||bytes%sizeof(wchar_t))throw 1;auto p=static_cast<const wchar_t*>(GlobalLock(h));if(!p)throw 1;auto length=wcsnlen_s(p,bytes/sizeof(wchar_t));if(length==bytes/sizeof(wchar_t)){GlobalUnlock(h);throw 1;}std::wstring text(p,length);GlobalUnlock(h);
  auto encoded=utf8(text);std::ofstream f(data,std::ios::binary);f.write(encoded.data(),encoded.size());ok=f.good();
 }
 std::ofstream result(status,std::ios::binary);result<<(ok?"OK":"ERROR");
 }catch(...){try{std::ofstream result{std::filesystem::path(argv[3])};result<<"ERROR";}catch(...){}}
 if(opened)CloseClipboard();if(owner)DestroyWindow(owner);LocalFree(argv);return ok?0:1;
}
