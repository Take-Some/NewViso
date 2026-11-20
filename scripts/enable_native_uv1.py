"""Enable real UV0/UV1 in NewViso native multilayer (not two copies of UV0).
Idempotent with source backups. Vulkan graphics receives TEXCOORD_1 as a
vertex attribute; GPU/CPU skinning preserves the secondary stream.
"""
from pathlib import Path
import shutil,re
ROOT=Path(__file__).resolve().parents[1]
BACK=ROOT/'.newviso_uv1_backup_20261109'
def edit(name,updates):
 p=ROOT/name
 t=p.read_text('utf8')
 original=t
 for old,new in updates:
  if old not in t:
   if new in t:continue
   raise ValueError(f'Missing patch anchor in {name}: {old[:90]!r}')
  if t.count(old)!=1:raise ValueError(f'Ambiguous anchor in {name}: {old[:90]!r} occurs {t.count(old)} times')
  t=t.replace(old,new)
 if t!=original:
  dest=BACK/name;dest.parent.mkdir(parents=True,exist_ok=True)
  if not dest.exists():shutil.copy2(p,dest)
  p.write_text(t,'utf8')
 print('UV1_PATCH',name,'changed',t!=original,flush=True)

edit('crates/newviso-scene/src/first_scene.rs',[
 ('const FLOATS_PER_VERTEX: usize = 17;','const FLOATS_PER_VERTEX: usize = 19;')
])
edit('crates/newviso-scene/src/geometry.rs',[
 ("""    out.extend_from_slice(&[1.0, 0.0, 0.0, 1.0]);
}

pub(super) fn append_local_asset_vertex(""",
 """    out.extend_from_slice(&[1.0, 0.0, 0.0, 1.0]);
    out.extend_from_slice(&[0.0, 0.0]); // TEXCOORD_1
}

pub(super) fn append_local_asset_vertex("""),
 ("""    color: [f32; 4],
    uv: [f32; 2],
) {
    append_world_vertex_uv_tangent(out, position, normal, color, uv, tangent);
}""",
 """    color: [f32; 4],
    uv: [f32; 2],
    uv1: [f32; 2],
) {
    append_world_vertex_uv_tangent(out, position, normal, color, uv, tangent);
    let end = out.len();
    out[end - 2..end].copy_from_slice(&uv1);
}"""),
 ("""    out.extend_from_slice(&tangent);
}

fn sphere_normal""",
 """    out.extend_from_slice(&tangent);
    out.extend_from_slice(&uv); // generic surfaces use UV0 as fallback UV1
}

fn sphere_normal""")
])
edit('crates/newviso-scene/src/first_scene/asset_models.rs',[
 ('    pub(super) uv: [f32; 2],\n    pub(super) joints:', '    pub(super) uv: [f32; 2],\n    pub(super) uv1: [f32; 2],\n    pub(super) joints:')
])
edit('crates/newviso-scene/src/first_scene/asset_models/triangles.rs',[
 ("""            vertex.color,
            vertex.uv,
        );""",
 """            vertex.color,
            vertex.uv,
            vertex.uv1,
        );"""),
 ("""        let joint_indices = mesh
""",
 """        // Original REDengine MLMask uses UV0; the material detail and
        // microblend source textures use TEXCOORD_1. Generic meshes
        // keep UV0 when the second channel is absent.
        let texcoord1 = mesh.vertex_streams.iter()
            .find(|stream| stream.semantic == VertexSemantic::TexCoord(1));
        let joint_indices = mesh
"""),
 ("""                    uv: [0.0, 0.0],
                    joints:""",
 """                    uv: [0.0, 0.0],
                    uv1: [0.0, 0.0],
                    joints:"""),
 ("""                    if let (Some(indices_stream), Some(weights_stream)) =
""",
 """                    triangle[corner].uv1 = if let Some(stream) = texcoord1 {
                        read_vec2(stream, vertex_index)?
                    } else {
                        triangle[corner].uv
                    };
                    if let (Some(indices_stream), Some(weights_stream)) =
""")
])
edit('crates/newviso-scene/src/first_scene/animation_skinning/sampling.rs',[
 ("""            skinned.color,
            skinned.uv,
        );""",
 """            skinned.color,
            skinned.uv,
            skinned.uv1,
        );""")
])
edit('crates/newviso-scene/src/first_scene/renderer_skinning.rs',[
 ('HashMap::<[u32; 34], usize>','HashMap::<[u32; 36], usize>'),
 ('Vec::<[f32; 34]>::new()','Vec::<[f32; 36]>::new()'),
 ('Vec::with_capacity(34)','Vec::with_capacity(36)'),
 ("""            vertex.color,
            vertex.uv,
        );""",
 """            vertex.color,
            vertex.uv,
            vertex.uv1,
        );"""),
 ('let row: [f32; 34]','let row: [f32; 36]'),
 ('data[begin..begin + 34].copy_from_slice(&row)','data[begin..begin + 36].copy_from_slice(&row)'),
 ('assert_eq!(packed.data[34].to_bits(), 72);','assert_eq!(packed.data[36].to_bits(), 76);'),
 ('assert_eq!(packed.data[35].to_bits(), 2);','assert_eq!(packed.data[37].to_bits(), 2);'),
 ('assert_eq!(packed.data[70].to_bits(), 74);','assert_eq!(packed.data[74].to_bits(), 78);'),
 ('assert_eq!(packed.data[71].to_bits(), 1);','assert_eq!(packed.data[75].to_bits(), 1);'),
 ('packed.data[72..]','packed.data[76..]'),
 ('assert_eq!(&data[17..25], &[0.; 8]);','assert_eq!(&data[19..27], &[0.; 8]);'),
 ('assert_eq!(data[25], 1.);','assert_eq!(data[27], 1.);'),
 ('assert_eq!(data[33], 1.);','assert_eq!(data[35], 1.);')
])
# All AssetTriangleVertex test literals with a manually supplied uv field.
for filename in [
 'crates/newviso-scene/src/first_scene/renderer_skinning.rs',
 'crates/newviso-scene/src/first_scene/animation_skinning/tests.rs',
 'crates/newviso-scene/src/first_scene/asset_models/tests.rs',
]:
 p=ROOT/filename
 old=p.read_text('utf8');t=old
 t=re.sub(r'(?m)^(\s*)uv: ([^\n]+),\n(\s*)joints:',r'\1uv: \2,\n\1uv1: [0.0; 2],\n\3joints:',t)
 if t!=old:
  dest=BACK/filename;dest.parent.mkdir(parents=True,exist_ok=True)
  if not dest.exists():shutil.copy2(p,dest)
  p.write_text(t,'utf8')
 print('TEST_UV1',filename,'added',t.count('uv1:')-old.count('uv1:'))
edit('crates/newviso-scene/src/first_scene/heightfield.rs',[
 ("""        if snow { mask[2] } else { 0.0 },
        1.0,
    ]
}""",
 """        if snow { mask[2] } else { 0.0 },
        1.0,
        if snow { mask[0] } else { 0.0 },
        if snow { mask[1] } else { 0.0 },
    ]
}""")
])
edit('crates/newviso-scene/src/first_scene/renderer_pipelines.rs',[
 ("""        VertexAttribute {
            location: 4,
            offset: 52,
            format: VertexFormat::Float32x4,
        },
    ];""",
 """        VertexAttribute {
            location: 4,
            offset: 52,
            format: VertexFormat::Float32x4,
        },
        VertexAttribute {
            location: 9,
            offset: 68,
            format: VertexFormat::Float32x2,
        },
    ];""")
])
for filename in ['scene.vert','scene_instanced.vert']:
 edit('crates/newviso-scene/src/assets/'+filename,[
 ("""layout(location = 4) in vec4 in_tangent;""",
 """layout(location = 4) in vec4 in_tangent;
layout(location = 9) in vec2 in_uv1;"""),
 ("""layout(location = 6) out vec4 v_tangent;""",
 """layout(location = 6) out vec4 v_tangent;
layout(location = 7) out vec2 v_uv1;"""),
 ("""    v_uv = in_uv;""",
 """    v_uv = in_uv;
    v_uv1 = in_uv1;""")
 ])
for filename in ['scene.frag','scene_gbuffer.frag']:
 edit('crates/newviso-scene/src/assets/'+filename,[
 ("""layout(location = 6) in vec4 v_tangent;""",
 """layout(location = 6) in vec4 v_tangent;
layout(location = 7) in vec2 v_uv1;"""),
 ("""        ? v_uv * material.shading2.x + material.shading0.zw : v_uv;""",
 """        ? v_uv1 * material.shading2.x + material.shading0.zw : v_uv;"""),
 ("""newviso_multilayer ? v_uv * material.shading2.x + material.shading0.zw : v_uv;""",
 """newviso_multilayer ? v_uv1 * material.shading2.x + material.shading0.zw : v_uv;""")
 ])
# Instanced gbuffer/forward share the correct secondary coordinates.
edit('crates/newviso-scene/src/assets/skinning.comp',[
 ('identical 17-float output','identical 19-float output'),
 ('uint src = vertex * 36u;','uint src = vertex * 38u;'),
 ('uint count = uint(bind_data[src+33]);','uint count = uint(bind_data[src+35]);'),
 ('uint joint=uint(bind_data[src+17+lane]);','uint joint=uint(bind_data[src+19+lane]);'),
 ('float weight=bind_data[src+25+lane];','float weight=bind_data[src+27+lane];'),
 ('floatBitsToUint(bind_data[src+34])','floatBitsToUint(bind_data[src+36])'),
 ('floatBitsToUint(bind_data[src+35])','floatBitsToUint(bind_data[src+37])'),
 ('uint dst=(frame.dimensions.y+expanded)*17u;','uint dst=(frame.dimensions.y+expanded)*19u;'),
 ('output_data[dst+16]=bind_data[src+16];','output_data[dst+16]=bind_data[src+16];\n        output_data[dst+17]=bind_data[src+17];\n        output_data[dst+18]=bind_data[src+18];')
])
print('UV1 ENGINE SOURCE PATCH COMPLETE')
