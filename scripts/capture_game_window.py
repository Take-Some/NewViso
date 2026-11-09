"""Capture only the verification process game window, never the desktop."""
import sys
import ctypes,struct,zlib,base64,json,pathlib
from ctypes import wintypes as w
c=pathlib.Path(sys.argv[1])
pid=int((c/'native.pid').read_text())
u=ctypes.WinDLL('user32');g=ctypes.WinDLL('gdi32');H=w.HANDLE
u.GetWindowThreadProcessId.argtypes=[w.HWND,ctypes.POINTER(w.DWORD)]
u.GetWindowTextW.argtypes=[w.HWND,w.LPWSTR,ctypes.c_int]
u.GetWindowRect.argtypes=[w.HWND,ctypes.POINTER(w.RECT)]
u.GetDC.argtypes=[w.HWND];u.GetDC.restype=H
u.ReleaseDC.argtypes=[w.HWND,H]
g.CreateCompatibleDC.argtypes=[H];g.CreateCompatibleDC.restype=H
g.CreateCompatibleBitmap.argtypes=[H,ctypes.c_int,ctypes.c_int];g.CreateCompatibleBitmap.restype=H
g.SelectObject.argtypes=[H,H];g.SelectObject.restype=H
g.StretchBlt.argtypes=[H,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,H,ctypes.c_int,ctypes.c_int,ctypes.c_int,ctypes.c_int,w.DWORD]
g.GetDIBits.argtypes=[H,H,w.UINT,w.UINT,ctypes.c_void_p,ctypes.c_void_p,w.UINT]
g.DeleteObject.argtypes=[H];g.DeleteDC.argtypes=[H]
windows=[]
@ctypes.WINFUNCTYPE(w.BOOL,w.HWND,w.LPARAM)
def collect(hwnd,_):
 p=w.DWORD();u.GetWindowThreadProcessId(hwnd,ctypes.byref(p));name=ctypes.create_unicode_buffer(512);u.GetWindowTextW(hwnd,name,512)
 if p.value==pid and (sys.argv[3] if len(sys.argv)>3 else 'NorthStar Endorphin verification') in name.value:
  rect=w.RECT();u.GetWindowRect(hwnd,ctypes.byref(rect));windows.append((hwnd,rect))
 return True
u.EnumWindows(collect,0)
if not windows:print(json.dumps({'error':'aim window unavailable','pid':pid}));raise SystemExit()
hwnd,rect=windows[0];width=rect.right-rect.left;height=rect.bottom-rect.top
dw,dh=width,height
dc=u.GetDC(hwnd);mem=g.CreateCompatibleDC(dc);bitmap=g.CreateCompatibleBitmap(dc,dw,dh);old=g.SelectObject(mem,bitmap)
try:
 u.PrintWindow.argtypes=[w.HWND,H,w.UINT];u.PrintWindow.restype=w.BOOL
 assert u.PrintWindow(hwnd,mem,2), 'Scoped game-window capture unavailable'
 g.SelectObject(mem,old)
 info=ctypes.create_string_buffer(struct.pack('<IiiHHIIiiII',40,dw,-dh,1,32,0,dw*dh*4,0,0,0,0)+b'\0'*1024)
 bits=ctypes.create_string_buffer(dw*dh*4);assert g.GetDIBits(mem,bitmap,0,dh,bits,info,0)==dh
 raw=bits.raw;rows=[]
 for y in range(dh):
  row=raw[y*dw*4:(y+1)*dw*4];rgb=bytearray(dw*3);rgb[0::3]=row[2::4];rgb[1::3]=row[1::4];rgb[2::3]=row[0::4];rows.append(b'\0'+rgb)
 def chunk(tag,data):return struct.pack('>I',len(data))+tag+data+struct.pack('>I',zlib.crc32(tag+data))
 png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',dw,dh,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(b''.join(rows),6))+chunk(b'IEND',b'')
 (c/sys.argv[2]).write_bytes(png)
 print(json.dumps({'bytes':len(png),'path':str(c/sys.argv[2])}))
finally:
 g.SelectObject(mem,old);g.DeleteObject(bitmap);g.DeleteDC(mem);u.ReleaseDC(hwnd,dc)
