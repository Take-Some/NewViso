//! Self-contained Cyrillic/Latin UI glyphs for the gameplay HUD.
//! The texture id is deliberately separate from Egui/Aurelia font atlases.
use super::*;
use newviso_ui_draw::{UiDrawCmd, UiDrawList, UiRect, UiTexId, UiTexture, UiVertex};
pub(super) const HUD_FONT_TEXTURE: UiTexId = UiTexId(0x2000_0010);

#[derive(Debug, Clone)]
pub(super) struct HudText {
    pub text: String,
    pub origin: [f32; 2],
    pub size_px: f32,
    pub tracking: f32,
    pub max_width: f32,
    pub color: [f32; 4],
}

#[derive(Deserialize)]
struct HudFontInfo {
    schema: String,
    texture_size: [u32;2],
    cell_px: u32,
    columns: u32,
    glyph_raster_px: f32,
    advances: Vec<f32>,
}

pub(super) struct HudFontAtlas {
    info: HudFontInfo,
    pixels: Vec<u8>,
}

impl HudFontAtlas {
    pub(super) fn load(root: &Path) -> Result<Option<Self>,String> {
        let prefix = "assets/textures/ui/hud/bahnschrift_hud_atlas";
        let meta = root.join(format!("{prefix}.json"));
        if !meta.exists() { return Ok(None); }
        let info: HudFontInfo = serde_json::from_slice(&fs::read(&meta)
            .map_err(|e| format!("HUD font manifest: {e}"))?)
            .map_err(|e| format!("HUD font manifest JSON: {e}"))?;
        if info.schema!="newviso.hud.font-atlas.v1"
            || info.texture_size!=[1024,512]
            || info.cell_px!=32 || info.columns!=32
            || info.glyph_raster_px<=0.0 || !info.glyph_raster_px.is_finite()
            || info.advances.len()!=512
            || info.advances.iter().any(|v|!v.is_finite()||*v<2.0||*v>60.0)
        { return Err("HUD atlas metadata is invalid".into()); }
        let pixels=fs::read(root.join(format!("{prefix}.rgba")))
            .map_err(|e|format!("HUD atlas pixels: {e}"))?;
        if pixels.len()!=info.texture_size[0] as usize*info.texture_size[1] as usize*4 {
            return Err("HUD atlas RGBA size mismatch".into());
        }
        host::info("newviso.hud","independent Cyrillic/Latin HUD atlas loaded");
        Ok(Some(Self { info,pixels }))
    }

    pub(super) fn texture(&self)->UiTexture {
        UiTexture {size:self.info.texture_size,rgba8:self.pixels.clone()}
    }

    fn glyph_uv(&self,ch:char)->([f32;4],f32) {
        let cp=ch as u32;
        let index=if cp<128 {cp as usize}
          else if (0x400..=0x52f).contains(&cp) {128+(cp-0x400) as usize}
          else {b'?' as usize};
        let col=index as u32%self.info.columns;
        let row=index as u32/self.info.columns;
        let cell=self.info.cell_px as f32;
        let w=self.info.texture_size[0] as f32;
        let h=self.info.texture_size[1] as f32;
        ([col as f32*cell/w,row as f32*cell/h,
          (col+1) as f32*cell/w,(row+1) as f32*cell/h],self.info.advances[index])
    }

    pub(super) fn append_glyphs(
        &self,draw:&mut UiDrawList,texts:&[HudText],
    )->Result<(),String>{
        let w=draw.screen_size_px[0] as f32;
        let h=draw.screen_size_px[1] as f32;
        let start=draw.mesh.indices.len() as u32;
        for text in texts {
            if text.origin.iter().chain(text.color.iter()).any(|x|!x.is_finite()) {
                return Err("HUD text has nonfinite position/color".into());
            }
            let mut x=text.origin[0];
            let y=text.origin[1];
            let scale=text.size_px.clamp(10.0,40.0)/self.info.glyph_raster_px;
            let cell=self.info.cell_px as f32*scale;
            let limit=(x+text.max_width.max(0.0)).min(w+100.0);
            let c=&text.color;
            let ch=|v:f32|->u32 { (v.clamp(0.0,1.0)*255.0).round() as u32 };
            let rgba=ch(c[0])|(ch(c[1])<<8)|(ch(c[2])<<16)|(ch(c[3])<<24);
            for symbol in text.text.chars().take(160) {
                let (uv,advance)=self.glyph_uv(symbol);
                if x+advance*scale>limit {break;}
                if !symbol.is_whitespace() {
                    let i=u32::try_from(draw.mesh.vertices.len())
                        .map_err(|_|"HUD vertex overflow".to_owned())?;
                    for (pos,tex) in [
                        ([x,y],[uv[0],uv[1]]),
                        ([x+cell,y],[uv[2],uv[1]]),
                        ([x+cell,y+cell],[uv[2],uv[3]]),
                        ([x,y+cell],[uv[0],uv[3]]),
                    ] {
                        draw.mesh.vertices.push(UiVertex{pos,uv:tex,color:rgba});
                    }
                    draw.mesh.indices.extend_from_slice(&[i,i+1,i+2,i,i+2,i+3]);
                }
                x+=advance*scale+text.tracking.clamp(-1.0,3.0);
            }
        }
        let end=draw.mesh.indices.len() as u32;
        if end>start {
            draw.mesh.cmds.push(UiDrawCmd{
                texture:HUD_FONT_TEXTURE,
                clip_rect:UiRect{min_x:0.0,min_y:0.0,max_x:w,max_y:h},
                index_range:start..end,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
 use super::*;
 #[test]
 fn glyph_mapping_supports_cyrillic_and_reserved_ids_do_not_collide() {
    assert_ne!(HUD_FONT_TEXTURE,newviso_ui_draw::reserved::FONT_ATLAS);
    assert_ne!(HUD_FONT_TEXTURE,newviso_ui_draw::reserved::SOLID_WHITE);
    let atlas=HudFontAtlas {
        info:HudFontInfo {schema:"newviso.hud.font-atlas.v1".into(),
          texture_size:[1024,512],cell_px:32,columns:32,
          glyph_raster_px:26.0,advances:vec![20.0;512]},
        pixels:vec![],
    };
    assert_ne!(atlas.glyph_uv('Ж').0,atlas.glyph_uv('?').0);
    assert_ne!(atlas.glyph_uv('Я').0,atlas.glyph_uv('A').0);
    let mut draw=UiDrawList::new();
    draw.screen_size_px=[1920,1080];
    atlas.append_glyphs(&mut draw,&[HudText {
        text:"НАЙТ-СИТИ 2077".into(),origin:[25.0,20.0],
        size_px:24.0,tracking:0.25,max_width:460.0,
        color:[1.0,0.8,0.2,1.0],
    }]).unwrap();
    assert!(draw.mesh.vertices.len()>20);
    assert_eq!(draw.mesh.cmds[0].texture,HUD_FONT_TEXTURE);
    assert_eq!(draw.mesh.cmds[0].index_range.end as usize,draw.mesh.indices.len());
 }
}
