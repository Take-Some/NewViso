//! Stable, unlit screen-space HUD composited after the full scene.
use super::*;
use newviso_ui_draw::{reserved, UiDrawCmd, UiDrawList, UiRect, UiVertex};
const HUD_LIMIT: usize = 16384;
fn color_channel(v: f32) -> u32 { (v.clamp(0.0, 1.0) * 255.0).round() as u32 }
fn pack_rgba(v: [f32; 4]) -> u32 {
    color_channel(v[0]) | (color_channel(v[1]) << 8)
    | (color_channel(v[2]) << 16) | (color_channel(v[3]) << 24)
}
pub(super) fn make_hud_draw_list(
    quads: &[SceneOverlayQuad], width: u32, height: u32,
) -> Result<Option<Value>, String> {
    if quads.is_empty() { return Ok(None); }
    if quads.len() > HUD_LIMIT { return Err("HUD exceeds 16384 rectangles".into()); }
    let (w, h) = (width.max(1) as f32, height.max(1) as f32);
    let mut draw = UiDrawList::new();
    draw.screen_size_px = [width.max(1), height.max(1)];
    draw.pixels_per_point = 1.0;
    draw.mesh.vertices.reserve(quads.len() * 4);
    draw.mesh.indices.reserve(quads.len() * 6);
    for (i, quad) in quads.iter().enumerate() {
        if quad.rect.iter().chain(quad.color.iter()).any(|x| !x.is_finite()) {
            return Err(format!("HUD quad {i}: nonfinite coordinates/color"));
        }
        let [a,b,c,d] = quad.rect;
        let [x0,y0,x1,y1] = [(a+1.0)*w*0.5,(b+1.0)*h*0.5,
                              (c+1.0)*w*0.5,(d+1.0)*h*0.5];
        if x1<=x0 || y1<=y0 { continue; }
        let base = u32::try_from(draw.mesh.vertices.len())
            .map_err(|_| "HUD vertex count exceeded u32".to_owned())?;
        let rgba = pack_rgba(quad.color);
        for pos in [[x0,y0],[x1,y0],[x1,y1],[x0,y1]] {
            draw.mesh.vertices.push(UiVertex { pos, uv:[0.5,0.5], color:rgba });
        }
        draw.mesh.indices.extend_from_slice(
            &[base,base+1,base+2,base,base+2,base+3]);
    }
    if draw.mesh.indices.is_empty() { return Ok(None); }
    draw.mesh.cmds.push(UiDrawCmd {
        texture:reserved::SOLID_WHITE,
        clip_rect:UiRect {min_x:0.0,min_y:0.0,max_x:w,max_y:h},
        index_range:0..u32::try_from(draw.mesh.indices.len()).unwrap_or(u32::MAX),
    });
    serde_json::to_value(draw).map(Some).map_err(|e|e.to_string())
}

impl EngineApplication {
    pub(super) fn set_hud_quads(&mut self,items:&[Value]) -> Result<(),String> {
        if items.len()>HUD_LIMIT { return Err("HUD primitive limit exceeded".into()); }
        let mut quads=Vec::new();
        let mut texts=Vec::new();
        for (i,item) in items.iter().enumerate() {
            let color=command_vec4(item,"color",i)?;
            if let Some(content)=item.get("text").and_then(Value::as_str) {
                if content.len()>2048 {
                    return Err(format!("HUD text {i}: too long"));
                }
                let position=command_vec3(item,"position",i)?;
                let scalar=|key:&str,default:f32| -> Result<f32,String> {
                    let v=item.get(key).and_then(Value::as_f64).map_or(default,|v|v as f32);
                    if !v.is_finite() {return Err(format!("HUD text {i}: invalid {key}"));}
                    Ok(v)
                };
                texts.push(application_hud_font::HudText {
                    text:content.to_owned(),
                    origin:[position[0],position[1]],
                    size_px:scalar("font_px",17.0)?.clamp(10.0,40.0),
                    tracking:scalar("tracking",0.0)?,
                    max_width:scalar("max_width",600.0)?.clamp(0.0,8192.0),
                    color,
                });
            } else {
                let rect=command_vec4(item,"rect",i)?;
                quads.push(SceneOverlayQuad {rect,color});
            }
        }
        self.hud_quads=quads;
        self.hud_texts=texts;
        self.hud_overlay_cache=None;
        Ok(())
    }

    /// Cache tessellation until script payload or physical screen size changes.
    /// The GPU still composites the retained HUD on every acquired frame.
    pub(super) fn screen_hud_draw_list(
        &mut self,width:u32,height:u32,
    ) -> Result<Option<Value>,String> {
        let size=[width.max(1),height.max(1)];
        if self.hud_last_framebuffer_size!=Some(size) {
            host::info("newviso.hud", format!(
                "framebuffer resized old={:?} new={:?}; invalidate font GPU atlas",
                self.hud_last_framebuffer_size, size,
            ));
            self.hud_last_framebuffer_size=Some(size);
            self.hud_font_uploaded=false;
            self.hud_overlay_cache=None;
        }
        if self.hud_quads.is_empty() && self.hud_texts.is_empty() {
            self.hud_overlay_cache=None;
            return Ok(None);
        }
        if let Some((size,cached))=self.hud_overlay_cache.as_ref() {
            if *size==[width,height] {
                return Ok(Some(cached.clone()));
            }
            // Fullscreen/resize must not lose our GPU-owned font resource.
            self.hud_font_uploaded=false;
        }
        let mut draw:UiDrawList=match make_hud_draw_list(&self.hud_quads,width,height)? {
            Some(value)=>serde_json::from_value(value)
                .map_err(|e|format!("HUD rect mesh decode: {e}"))?,
            None=>{
                let mut empty=UiDrawList::new();
                empty.screen_size_px=[width.max(1),height.max(1)];
                empty
            }
        };
        if !self.hud_texts.is_empty() {
            let font=self.hud_font.as_ref().ok_or(
                "HUD font is not available; generate the project-owned HUD atlas"
            )?;
            font.append_glyphs(&mut draw,&self.hud_texts)?;
        }
        let normal=serde_json::to_value(&draw)
            .map_err(|e|format!("HUD draw serialization: {e}"))?;
        self.hud_overlay_cache=Some(([width,height],normal.clone()));
        if !self.hud_font_uploaded && !self.hud_texts.is_empty() {
            let atlas=self.hud_font.as_ref().ok_or("HUD atlas missing")?;
            draw.texture_delta.set.insert(
                application_hud_font::HUD_FONT_TEXTURE,
                atlas.texture(),
            );
            host::info("newviso.hud", format!(
                "font atlas upload: rects={} texts={} surface={}x{}",
                self.hud_quads.len(),self.hud_texts.len(),width,height
            ));
            self.hud_font_uploaded=true;
            return serde_json::to_value(draw).map(Some)
                .map_err(|e|format!("HUD atlas upload serialization: {e}"));
        }
        Ok(Some(normal))
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hud_uses_screen_space_and_unlit_solid_white() {
        let quad=SceneOverlayQuad {
            rect:[-1.0,-1.0,0.0,0.0],color:[1.0,0.0,0.0,0.5],
        };
        let v=make_hud_draw_list(&[quad],1920,1080).unwrap().unwrap();
        assert_eq!(v["screen_size_px"],json!([1920,1080]));
        assert_eq!(v["mesh"]["vertices"][0]["pos"],json!([0.0,0.0]));
        assert_eq!(v["mesh"]["vertices"][2]["pos"],json!([960.0,540.0]));
        assert_eq!(v["mesh"]["vertices"][0]["color"],json!(0x800000ffu32));
        assert_eq!(v["mesh"]["cmds"][0]["texture"],json!(15));
    }
    #[test]
    fn hud_keeps_layer_order_and_resizes() {
        let a=SceneOverlayQuad {rect:[-1.0,-1.0,1.0,1.0],color:[0.0,0.0,0.0,0.8]};
        let b=SceneOverlayQuad {rect:[0.0,0.0,1.0,1.0],color:[0.0,1.0,1.0,1.0]};
        let v=make_hud_draw_list(&[a,b],2560,1440).unwrap().unwrap();
        assert_eq!(v["mesh"]["indices"],json!([0,1,2,0,2,3,4,5,6,4,6,7]));
        assert_eq!(v["mesh"]["vertices"][4]["pos"],json!([1280.0,720.0]));
    }
}
