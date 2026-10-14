//! Full resolution DDS material set for procedural terrain, BC1/BC3 GPU mip chains.
use super::*;

#[derive(Debug)]
pub(super) struct TerrainGpuMaterial {
    pub bind_group: u32,
    pub textures: Vec<u32>,
    pub uniform_buffer: Option<u32>,
}

// The heightfield's five samplers are color layers, not the generic PBR channels.
// Select that interpretation explicitly, never from an imported vertex's alpha.
fn terrain_material_uniform() -> [f32; 12] {
    [1.0, 0.0, 32.0, 0.0, 0.0, 0.5, (64 | 256) as f32, 1.0, 0.0, 0.0, 0.0, 0.0]
}

pub(super) fn load_dds_mips(data: &[u8], path: &str)
    -> Result<(u32,u32,&'static str,Vec<TextureMipUpload>),String>
{
    if data.len()<128 || &data[0..4]!=b"DDS " {
        return Err(format!("invalid terrain DDS: {path}"));
    }
    let read = |o:usize| u32::from_le_bytes(data[o..o+4].try_into().unwrap());
    let height=read(12); let width=read(16);
    let mip_count=read(28).max(1).min(16);
    if width==0 || height==0 || width>8192 || height>8192 {
        return Err(format!("invalid terrain DDS dimensions: {path}"));
    }
    let (format,bytes_per_block)=match &data[84..88] {
        b"DXT1"=>("Bc1RgbaSrgb",8_usize),
        b"DXT5"=>("Bc3RgbaSrgb",16_usize),
        other=>return Err(format!("unsupported DDS FourCC {other:?} for {path}")),
    };
    let mut offset=0_usize;
    let mut mips=Vec::with_capacity(mip_count as usize);
    for level in 0..mip_count {
        let w=(width>>level).max(1);
        let h=(height>>level).max(1);
        let bytes=(w.div_ceil(4) as usize)*(h.div_ceil(4) as usize)*bytes_per_block;
        if 128+offset+bytes>data.len() {
            return Err(format!("terrain DDS '{path}' mip {level} truncated"));
        }
        mips.push(TextureMipUpload {
            level,width:w,height:h,offset:offset as u64,byte_len:bytes as u64
        });
        offset+=bytes;
    }
    Ok((width,height,format,mips))
}
impl Scene3dRuntime {
    pub(super) fn initialize_terrain_gpu_material(
        &mut self, render:&RenderClient
    ) -> Result<(),String> {
        let Some(paths)=self.terrain_texture_paths.clone() else {return Ok(())};
        let gpu=self.gpu.ok_or("terrain renderer requires initialized scene GPU")?;
        let mut handles=Vec::new();
        for path in paths {
            let dds=host_runtime::call_service(
                ASSET_SERVICE,"asset.raw_bytes_v1",path.as_bytes()
            ).map_err(|err|format!("terrain DDS '{path}' load failed: {err}"))?;
            let (w,h,format,mips)=load_dds_mips(&dds,&path)?;
            let handle=render.create_texture(
                &format!("newviso.terrain.dds.{path}"),
                w,h,format,&mips,&dds[128..]
            )?;
            handles.push(handle);
            host_runtime::info("newviso.terrain",
              format!("real DDS '{}' {}x{} mips={}",path,w,h,mips.len()));
        }
        let t:[u32;5]=handles.clone().try_into()
            .map_err(|_|"terrain expected five DDS textures")?;
        let uniform_buffer = render.create_buffer(
            "newviso.terrain.material.parameters", 48, "Uniform", "CpuToGpu",
        )?;
        render.write_buffer_f32(uniform_buffer, 0, &terrain_material_uniform())?;
        let bind_group=render.create_bind_group6(
            "newviso.terrain.full_resolution",gpu.material_bind_group_layout,
            [Some(t[0]),Some(t[1]),Some(t[2]),Some(t[3]),Some(t[4]),None],
            Some(gpu.material_sampler),
            Some((uniform_buffer,0,48))
        )?;
        self.terrain_gpu_material=Some(TerrainGpuMaterial {
            bind_group,textures:handles,uniform_buffer:Some(uniform_buffer)
        });
        Ok(())
    }
    pub(super) fn initialize_water_gpu_material(
        &mut self,render:&RenderClient
    ) -> Result<(),String> {
        let Some(paths)=self.water_texture_paths.clone() else {return Ok(());};
        let gpu=self.gpu.ok_or("water requires initialized GPU")?;
        let mut handles=Vec::new();
        for path in paths {
            let dds=host_runtime::call_service(
                ASSET_SERVICE,"asset.raw_bytes_v1",path.as_bytes()
            ).map_err(|e|format!("water DDS '{path}' load failed: {e}"))?;
            let (w,h,format,mips)=load_dds_mips(&dds,&path)?;
            // Normal vectors are linear data, NEVER decoded through sRGB.
            let linear_format=match format {
                "Bc1RgbaSrgb"=>"Bc1RgbaUnorm",
                "Bc3RgbaSrgb"=>"Bc3RgbaUnorm",
                _=>format,
            };
            let handle=render.create_texture(
                &format!("newviso.water.dds.{path}"),
                w,h,linear_format,&mips,&dds[128..]
            )?;
            handles.push(handle);
            host_runtime::info("newviso.water",format!(
                "real ripple DDS '{}' {}x{} mip_levels={}",path,w,h,mips.len()
            ));
        }
        let bind_group=render.create_bind_group6(
            "newviso.water.normal_material",gpu.material_bind_group_layout,
            // Source static bumps, foam, water-colour lookup and forcing noise.
            // Small dynamic ripples are supplied by the dedicated optics group.
            [Some(handles[0]),Some(handles[1]),Some(handles[2]),Some(handles[3]),Some(handles[4]),None],
            Some(gpu.material_sampler),
            Some((gpu.default_material_uniform,0,48)),
        )?;
        self.water_gpu_material=Some(TerrainGpuMaterial{
            bind_group,textures:handles,uniform_buffer:None,
        });
        Ok(())
    }
    pub(super) fn shutdown_water_gpu_material(&mut self, render:&RenderClient) {
        if let Some(t)=self.water_gpu_material.take() {
            render.destroy_bind_group(t.bind_group);
            for tex in t.textures {render.destroy_texture(tex);}
        }
    }
    pub(super) fn shutdown_terrain_gpu_material(&mut self, render:&RenderClient) {
        if let Some(t)=self.terrain_gpu_material.take() {
            render.destroy_bind_group(t.bind_group);
            if let Some(buffer) = t.uniform_buffer { render.destroy_buffer(buffer); }
            for tex in t.textures {render.destroy_texture(tex);}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heightfield_material_requires_an_explicit_domain_flag() {
        let flags = terrain_material_uniform()[6] as u32;
        assert_eq!(flags, 64 | 256);
        assert_eq!(flags & (1 | 2 | 4 | 8 | 16 | 32 | 128), 0);
    }

    #[test]
    fn dds_mips_have_byte_offsets_relative_to_payload() {
        let mut data=vec![0_u8;168];
        data[0..4].copy_from_slice(b"DDS ");
        data[12..16].copy_from_slice(&8u32.to_le_bytes());
        data[16..20].copy_from_slice(&8u32.to_le_bytes());
        data[28..32].copy_from_slice(&2u32.to_le_bytes());
        data[84..88].copy_from_slice(b"DXT1");
        let (_,_,f,m)=load_dds_mips(&data,"test").unwrap();
        assert_eq!(f,"Bc1RgbaSrgb");
        assert_eq!((m[0].offset,m[1].offset),(0,32));
    }
}
