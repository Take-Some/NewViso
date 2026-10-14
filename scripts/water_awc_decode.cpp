#include <cstdint>
#include <vector>
#include <fstream>
#include <iostream>
#include <iterator>
#include <cstring>
void decrypt(uint32_t*v,uint32_t n,const uint32_t*k) {
 if(n<2)return;
 uint32_t sum=(6+52/n)*0x9e3779b9u,y=v[0],z,p,e;
 do {e=(sum>>2)&3;for(p=n-1;p>0;--p){
  z=v[p-1];uint32_t mx=(((z>>5)^(y<<2))+((y>>3)^(z<<4)))^((sum^y)+(k[(p&3)^e]^z));
  y=v[p]-=mx;
 }z=v[n-1];uint32_t mx=(((z>>5)^(y<<2))+((y>>3)^(z<<4)))^((sum^y)+(k[(p&3)^e]^z));
 y=v[0]-=mx;sum-=0x9e3779b9u;}while(sum);
}
int main(int argc,char**argv) {
 if(argc!=3)return 2;
 uint32_t key[4];for(auto &k:key)std::cin>>std::hex>>k;if(!std::cin)return 3;
 std::ifstream in(argv[1],std::ios::binary);std::vector<char>b((std::istreambuf_iterator<char>(in)),{});
 if(b.size()<16)return 4;
 if(std::memcmp(b.data(),"ADAT",4))decrypt(reinterpret_cast<uint32_t*>(b.data()),b.size()/4,key);
 if(std::memcmp(b.data(),"ADAT",4))return 5;
 uint32_t version,count;std::memcpy(&version,b.data()+4,4);std::memcpy(&count,b.data()+8,4);
 if(count>100000)return 6;
 if(version&0x80000) {
  if(version&0x40000)return 7;
  size_t ids=16+((version&0x10000)?count*2:0),pos=ids+count*4;
  if(pos>b.size())return 8;
  for(uint32_t i=0;i<count;i++) {
   uint32_t info;std::memcpy(&info,b.data()+ids+i*4,4);
   for(uint32_t j=0;j<(info>>29);j++,pos+=8) {
    if(pos+8>b.size())return 9;
    uint64_t chunk;std::memcpy(&chunk,b.data()+pos,8);
    size_t off=chunk&0xfffffff,size=(chunk>>28)&0xfffffff;
    if(off+size>b.size())return 10;
    if((chunk>>56)==0x55)decrypt(reinterpret_cast<uint32_t*>(b.data()+off),size/4,key);
   }
  }
  version&=~0x80000;std::memcpy(b.data()+4,&version,4);
 }
 std::ofstream out(argv[2],std::ios::binary);out.write(b.data(),b.size());
}
