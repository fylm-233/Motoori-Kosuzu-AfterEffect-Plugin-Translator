#pragma once
#include <windows.h>
#include <string>
#include <map>
#include <stdexcept>
#include <charconv>
#include <cmath>
namespace lbjson {
struct Value {char type='0';std::string text;double number=0;bool boolean=false;std::map<std::string,Value> object;const Value&at(const std::string&key)const{auto it=object.find(key);if(it==object.end())throw std::runtime_error("Missing field "+key);return it->second;}};
inline std::wstring wide(const std::string&s){if(s.empty())return {};int n=MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,s.data(),(int)s.size(),nullptr,0);if(!n)throw std::runtime_error("Invalid UTF8");std::wstring out(n,0);MultiByteToWideChar(CP_UTF8,MB_ERR_INVALID_CHARS,s.data(),(int)s.size(),out.data(),n);return out;}
inline std::string utf8(const std::wstring&s){if(s.empty())return {};int n=WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),nullptr,0,nullptr,nullptr);if(!n)throw std::runtime_error("Invalid UTF16");std::string out(n,0);WideCharToMultiByte(CP_UTF8,WC_ERR_INVALID_CHARS,s.data(),(int)s.size(),out.data(),n,nullptr,nullptr);return out;}
class Parser {
 const std::string&s;size_t i=0;
 void fail(){throw std::runtime_error("Invalid JSON at "+std::to_string(i));}
 char take(){if(i>=s.size())fail();return s[i++];}
 void ws(){while(i<s.size()&&(s[i]==' '||s[i]=='\t'||s[i]=='\r'||s[i]=='\n'))i++;}
 unsigned hex(){unsigned value=0;for(int n=0;n<4;n++){char c=take();value<<=4;if(c>='0'&&c<='9')value+=c-'0';else if(c>='a'&&c<='f')value+=c-'a'+10;else if(c>='A'&&c<='F')value+=c-'A'+10;else fail();}return value;}
 std::string str(){if(take()!='"')fail();std::string out;while(i<s.size()){char c=take();if(c=='"'){wide(out);return out;}if((unsigned char)c<32)fail();if(c!='\\'){out+=c;continue;}char e=take();if(e=='"'||e=='\\'||e=='/')out+=e;else if(e=='b')out+='\b';else if(e=='f')out+='\f';else if(e=='n')out+='\n';else if(e=='r')out+='\r';else if(e=='t')out+='\t';else if(e=='u'){unsigned first=hex();std::wstring u(1,(wchar_t)first);if(first>=0xd800&&first<=0xdbff){if(take()!='\\'||take()!='u')fail();unsigned second=hex();if(second<0xdc00||second>0xdfff)fail();u+=(wchar_t)second;}out+=utf8(u);}else fail();}fail();return {};}
 Value value(unsigned depth){if(depth>32)fail();ws();Value v;char c=i<s.size()?s[i]:0;if(c=='{'){i++;v.type='o';ws();if(i<s.size()&&s[i]=='}'){i++;return v;}while(true){ws();auto key=str();ws();if(take()!=':')fail();v.object[key]=value(depth+1);ws();c=take();if(c=='}')return v;if(c!=',')fail();}}
 if(c=='"'){v.type='s';v.text=str();return v;}if(s.compare(i,4,"true")==0){i+=4;v.type='b';v.boolean=true;return v;}if(s.compare(i,5,"false")==0){i+=5;v.type='b';return v;}if(s.compare(i,4,"null")==0){i+=4;return v;}
 v.type='n';size_t begin=i;if(i<s.size()&&s[i]=='-')i++;if(i>=s.size())fail();if(s[i]=='0')i++;else{if(s[i]<'1'||s[i]>'9')fail();while(i<s.size()&&s[i]>='0'&&s[i]<='9')i++;}if(i<s.size()&&s[i]=='.'){i++;size_t digits=i;while(i<s.size()&&s[i]>='0'&&s[i]<='9')i++;if(i==digits)fail();}if(i<s.size()&&(s[i]=='e'||s[i]=='E')){i++;if(i<s.size()&&(s[i]=='+'||s[i]=='-'))i++;size_t digits=i;while(i<s.size()&&s[i]>='0'&&s[i]<='9')i++;if(i==digits)fail();}auto result=std::from_chars(s.data()+begin,s.data()+i,v.number);if(result.ec!=std::errc()||result.ptr!=s.data()+i||!std::isfinite(v.number))fail();return v;}
public:explicit Parser(const std::string&text):s(text){if(s.size()>=3&&s.compare(0,3,"\xef\xbb\xbf")==0)i=3;}Value parse(){auto out=value(0);ws();if(i!=s.size())fail();return out;}
};
inline Value parse(const std::string&s){return Parser(s).parse();}
inline std::string quote(const std::string&s){std::string out="\"";const char*hex="0123456789abcdef";for(unsigned char c:s){if(c=='"'||c=='\\'){out+='\\';out+=(char)c;}else if(c<32){out+="\\u00";out+=hex[c>>4];out+=hex[c&15];}else out+=(char)c;}return out+'"';}
}
