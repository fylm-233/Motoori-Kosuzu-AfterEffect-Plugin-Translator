#include "Json.h"
#include <filesystem>
#include <fstream>
int wmain(int argc,wchar_t**argv){try{if(argc!=2)return 2;auto path=std::filesystem::path(argv[1]);if(std::filesystem::file_size(path)>10*1024*1024)return 3;std::ifstream f(path,std::ios::binary);if(!f)return 4;auto map=lbjson::parse(std::string((std::istreambuf_iterator<char>(f)),{}));if(map.type!='o')return 5;for(auto&[key,value]:map.object){if(key.empty()||key=="__proto__"||key=="constructor"||key=="prototype"||value.type!='s')return 6;auto source=lbjson::wide(key),target=lbjson::wide(value.text);if(source.size()>8192||target.size()>8192||source.find(L'\0')!=std::wstring::npos||target.find(L'\0')!=std::wstring::npos)return 7;}return 0;}catch(...){return 1;}}
