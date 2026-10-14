"""Decode authored Island DDS raster texels into a portable GLSL atlas.

The primitive scene renderer currently binds a shared default image. This tool
preserves original material bitmaps and packs 32x32 real texels per material
for that path. A future dedicated terrain PBR pass can directly bind the DDS.
"""
from pathlib import Path
import struct

ROOT = Path(__file__).resolve().parents[2]
DDS = ROOT / "Projects/Island/assets/textures"
TARGET = ROOT / "NewViso/crates/newviso-scene/src/assets/island_texture_atlas.glsl"
LAYERS = ("grass", "sand", "rock", "soil", "wood")


def dxt_color(value):
    return ((value >> 11 & 31)*255//31, (value >> 5 & 63)*255//63,
            (value & 31)*255//31)


def sample_dds(image, x, y, width, format_bytes):
    block_bytes = 8 if format_bytes == b"DXT1" else 16
    block = 128 + ((y//4)*((width+3)//4) + x//4)*block_bytes
    offset = block + (0 if block_bytes == 8 else 8)
    a, b, mask = struct.unpack_from("<HHI", image, offset)
    c0, c1 = dxt_color(a), dxt_color(b)
    if a > b or block_bytes == 16:
        colors = (c0,c1,tuple((2*u+v)//3 for u,v in zip(c0,c1)),
                 tuple((u+2*v)//3 for u,v in zip(c0,c1)))
    else:
        colors = (c0,c1,tuple((u+v)//2 for u,v in zip(c0,c1)),(0,0,0))
    return colors[(mask >> (2*((y%4)*4+x%4))) & 3]


def build():
    existing = TARGET.read_text(encoding="utf-8")
    routines = "vec3 nv_unpack" + existing.split("vec3 nv_unpack",1)[1]
    arrays = []
    for name in LAYERS:
        image = (DDS/(name+".dds")).read_bytes()
        if image[:4] != b"DDS ": raise ValueError(name+": invalid DDS")
        height,width = struct.unpack_from("<II",image,12)
        fmt=image[84:88]
        if fmt not in (b"DXT1",b"DXT5"):
            raise ValueError(f"{name}: unsupported {fmt!r}")
        pixels=[]
        for y in range(32):
            for x in range(32):
                r,g,b=sample_dds(image,(2*x+1)*width//64,
                     (2*y+1)*height//64,width,fmt)
                pixels.append(f"0x{r:02x}{g:02x}{b:02x}u")
        rows=[", ".join(pixels[i:i+8]) for i in range(0,1024,8)]
        arrays.append("const uint NV_TEX_"+name.upper()+
            "[1024] = uint[](\n  "+",\n  ".join(rows)+"\n);\n")
        print("sampled",name,width,height,fmt.decode())
    TARGET.write_text("// Baked decoded texels from actual DXT diffuse maps.\n"+
        "".join(arrays)+routines,encoding="utf-8")
    print("atlas", TARGET.stat().st_size)


if __name__ == "__main__":
    build()
