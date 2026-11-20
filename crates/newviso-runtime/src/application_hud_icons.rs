//! Independent screen-space iconography from project-owned Cyberpunk INK textures.
use super::*;
use newviso_ui_draw::{UiDrawCmd, UiDrawList, UiRect, UiTexId, UiTexture, UiVertex};

pub(super) const HUD_ICON_TEXTURE: UiTexId=UiTexId(0x2000_0011);

#[derive(Clone,Debug)]
pub(super) struct HudIcon {
    pub name:String,
    pub position:[f32;2],
    pub size:[f32;2],
    pub color:[f32;4],
}
#[derive(Deserialize)]
struct IconEntry {name:String,index:u32}
#[derive(Deserialize)]
struct IconInfo {
    schema:String,
    texture_size:[u32;2],
    cell_px:u32,
    icons:Vec<IconEntry>,
}
pub(super) struct HudIconAtlas {
    info:IconInfo,
    pixels:Vec<u8>,
}
impl HudIconAtlas {
    pub(super) fn load(root:&Path)->Result<Option<Self>,String>{
        let prefix="assets/textures/ui/hud/cyberpunk_hud_icons";
        let meta=root.join(format!("{prefix}.json"));
        if !meta.exists(){return Ok(None);}
        let info:IconInfo=serde_json::from_slice(&fs::read(&meta)
            .map_err(|e|format!("HUD icon manifest: {e}"))?)
            .map_err(|e|format!("HUD icon manifest JSON: {e}"))?;
        if info.schema!="newviso.hud.icons.v1"||info.texture_size!=[1024,64]
            ||info.cell_px!=64||info.icons.is_empty()||info.icons.len()>16
            ||info.icons.iter().any(|e|e.name.is_empty()||e.index>=16){
            return Err("HUD icon manifest has invalid schema, size or slot".into());
        }
        let pixels=fs::read(root.join(format!("{prefix}.rgba")))
            .map_err(|e|format!("HUD icon atlas: {e}"))?;
        if pixels.len()!=1024*64*4 {
            return Err("HUD icon atlas RGBA size mismatch".into());
        }
        host::info("newviso.hud",format!(
            "original INK HUD icon atlas loaded, icons={}",info.icons.len()));
        Ok(Some(Self{info,pixels}))
    }
    pub(super) fn texture(&self)->UiTexture {
        UiTexture{size:self.info.texture_size,rgba8:self.pixels.clone()}
    }
    pub(super) fn append_icon(
        &self,draw:&mut UiDrawList,item:&HudIcon,
    )->Result<(),String>{
        if item.position.iter().chain(item.size.iter())
            .chain(item.color.iter()).any(|x|!x.is_finite()){
            return Err("HUD icon geometry or tint is not finite".into());
        }
        if item.size.iter().any(|x|*x<0.0||*x>512.0){
            return Err("HUD icon dimensions outside supported range".into());
        }
        let slot=self.info.icons.iter().find(|e|e.name==item.name)
            .ok_or_else(||format!("HUD icon '{}' not in project atlas",item.name))?
            .index;
        let [x,y]=item.position;
        let [sx,sy]=item.size;
        if sx<=0.0||sy<=0.0{return Ok(());}
        let w=draw.screen_size_px[0].max(1) as f32;
        let h=draw.screen_size_px[1].max(1) as f32;
        let u0=slot as f32*self.info.cell_px as f32/self.info.texture_size[0] as f32;
        let u1=(slot+1) as f32*self.info.cell_px as f32/self.info.texture_size[0] as f32;
        let start=u32::try_from(draw.mesh.indices.len())
            .map_err(|_|"HUD icon index overflow".to_owned())?;
        let v=u32::try_from(draw.mesh.vertices.len())
            .map_err(|_|"HUD icon vertex overflow".to_owned())?;
        let ch=|x:f32|->u32{(x.clamp(0.0,1.0)*255.0).round() as u32};
        let c=&item.color;
        let tint=ch(c[0])|(ch(c[1])<<8)|(ch(c[2])<<16)|(ch(c[3])<<24);
        for (pos,uv) in [
            ([x,y],[u0,0.0]),([x+sx,y],[u1,0.0]),
            ([x+sx,y+sy],[u1,1.0]),([x,y+sy],[u0,1.0])
        ]{
            draw.mesh.vertices.push(UiVertex{pos,uv,color:tint});
        }
        draw.mesh.indices.extend_from_slice(&[v,v+1,v+2,v,v+2,v+3]);
        if let Some(last)=draw.mesh.cmds.last_mut(){
            if last.texture==HUD_ICON_TEXTURE&&last.index_range.end==start{
                last.index_range.end=start+6;
                return Ok(());
            }
        }
        draw.mesh.cmds.push(UiDrawCmd{
            texture:HUD_ICON_TEXTURE,
            clip_rect:UiRect{min_x:0.0,min_y:0.0,max_x:w,max_y:h},
            index_range:start..start+6,
        });
        Ok(())
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    fn mock()->HudIconAtlas{
        HudIconAtlas{
            info:IconInfo{schema:"newviso.hud.icons.v1".into(),
                texture_size:[1024,64],cell_px:64,
                icons:vec![IconEntry{name:"phone".into(),index:1}]},
            pixels:vec![],
        }
    }
    #[test]
    fn icon_asset_uses_separate_texture_and_screen_uvs(){
        let mut draw=UiDrawList::new();
        draw.screen_size_px=[1920,1080];
        let item=HudIcon{name:"phone".into(),position:[40.0,60.0],
            size:[28.0,28.0],color:[1.0;4]};
        mock().append_icon(&mut draw,&item).unwrap();
        assert_ne!(HUD_ICON_TEXTURE,crate::application_hud_font::HUD_FONT_TEXTURE);
        assert_eq!(draw.mesh.cmds[0].texture,HUD_ICON_TEXTURE);
        assert_eq!(draw.mesh.vertices[0].pos,[40.0,60.0]);
        assert_eq!(draw.mesh.vertices[2].pos,[68.0,88.0]);
        assert!(mock().append_icon(&mut draw,&HudIcon{
            name:"missing".into(),..item}).is_err());
    }
}
