//! Stable, unlit screen-space HUD composited after the full scene.
use super::*;
use newviso_ui_draw::{reserved, UiDrawCmd, UiDrawList, UiRect, UiVertex};
const HUD_LIMIT: usize = 16384;
fn color_channel(v: f32) -> u32 { (v.clamp(0.0, 1.0) * 255.0).round() as u32 }
fn pack_rgba(v: [f32; 4]) -> u32 {
    color_channel(v[0]) | (color_channel(v[1]) << 8)
    | (color_channel(v[2]) << 16) | (color_channel(v[3]) << 24)
}
/// Append a single authored solid primitive in UI pixels, merging only
/// adjacent solid geometry. Depth test/lighting never apply to this mesh.
fn append_hud_rect(
    draw: &mut UiDrawList, quad: &SceneOverlayQuad,
) -> Result<(), String> {
    if quad.rect.iter().chain(quad.color.iter()).any(|v| !v.is_finite()) {
        return Err("HUD quad contains non-finite coordinates/color".to_owned());
    }
    let w=draw.screen_size_px[0].max(1) as f32;
    let h=draw.screen_size_px[1].max(1) as f32;
    let [a,b,c,d]=quad.rect;
    let [x0,y0,x1,y1]=[(a+1.0)*w*0.5,(b+1.0)*h*0.5,
                       (c+1.0)*w*0.5,(d+1.0)*h*0.5];
    if x1<=x0 || y1<=y0 {return Ok(());}
    let base=u32::try_from(draw.mesh.vertices.len())
        .map_err(|_|"HUD vertex index overflow".to_owned())?;
    let start=u32::try_from(draw.mesh.indices.len())
        .map_err(|_|"HUD index count overflow".to_owned())?;
    let rgba=pack_rgba(quad.color);
    for pos in [[x0,y0],[x1,y0],[x1,y1],[x0,y1]] {
        draw.mesh.vertices.push(UiVertex {pos,uv:[0.5,0.5],color:rgba});
    }
    draw.mesh.indices.extend_from_slice(&[
        base,base+1,base+2,base,base+2,base+3
    ]);
    if let Some(last)=draw.mesh.cmds.last_mut() {
        if last.texture==reserved::SOLID_WHITE
            && last.index_range.end==start
        {
            last.index_range.end=start+6;
            return Ok(());
        }
    }
    draw.mesh.cmds.push(UiDrawCmd {
        texture:reserved::SOLID_WHITE,
        clip_rect:UiRect {min_x:0.0,min_y:0.0,max_x:w,max_y:h},
        index_range:start..start+6,
    });
    Ok(())
}

#[cfg(test)]
pub(super) fn make_hud_draw_list(
    quads: &[SceneOverlayQuad], width: u32, height: u32,
) -> Result<Option<Value>, String> {
    if quads.is_empty() { return Ok(None); }
    if quads.len() > HUD_LIMIT { return Err("HUD exceeds 16384 rectangles".into()); }
    let mut draw=UiDrawList::new();
    draw.screen_size_px=[width.max(1),height.max(1)];
    draw.pixels_per_point=1.0;
    draw.mesh.vertices.reserve(quads.len()*4);
    draw.mesh.indices.reserve(quads.len()*6);
    for quad in quads {
        append_hud_rect(&mut draw,quad)?;
    }
    if draw.mesh.indices.is_empty() {return Ok(None);}
    serde_json::to_value(draw).map(Some).map_err(|e|e.to_string())
}

/// Preserve painter order across both texture types. The vertex stream
/// is screen-space only and has no interaction with scene lighting/depth.
fn compose_ordered_hud(
    rects:&[SceneOverlayQuad],
    texts:&[application_hud_font::HudText],
    painter_order:&[(bool,usize)],
    font:Option<&application_hud_font::HudFontAtlas>,
    width:u32,
    height:u32,
) -> Result<UiDrawList,String> {
    let mut draw=UiDrawList::new();
    draw.screen_size_px=[width.max(1),height.max(1)];
    draw.pixels_per_point=1.0;
    for &(text,index) in painter_order {
        if text {
            let value=texts.get(index)
                .ok_or_else(||format!("HUD text order index {index} is invalid"))?;
            font.ok_or("HUD text atlas is missing")?
                .append_glyphs(&mut draw,std::slice::from_ref(value))?;
        } else {
            let value=rects.get(index)
                .ok_or_else(||format!("HUD rect order index {index} is invalid"))?;
            append_hud_rect(&mut draw,value)?;
        }
    }
    Ok(draw)
}

impl EngineApplication {
    pub(super) fn set_hud_quads(&mut self,items:&[Value]) -> Result<(),String> {
        if items.len()>HUD_LIMIT { return Err("HUD primitive limit exceeded".into()); }
        let mut quads=Vec::new();
        let mut texts=Vec::new();
        let mut order=Vec::with_capacity(items.len());
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
                order.push((true,texts.len()));
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
                order.push((false,quads.len()));
                quads.push(SceneOverlayQuad {rect,color});
            }
        }
        self.hud_quads=quads;
        self.hud_texts=texts;
        self.hud_item_order=order;
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
        let mut draw=compose_ordered_hud(
            &self.hud_quads,&self.hud_texts,&self.hud_item_order,
            self.hud_font.as_ref(),width,height,
        )?;
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
    fn mixed_shapes_text_shapes_preserve_original_painter_order() {
        let atlas=application_hud_font::HudFontAtlas::fixture_for_tests();
        let rects=[
            SceneOverlayQuad{rect:[-1.0,-1.0,1.0,1.0],
                color:[0.0,0.0,0.0,0.7]},
            SceneOverlayQuad{rect:[-0.2,0.3,0.2,0.35],
                color:[1.0,0.0,0.0,1.0]},
        ];
        let texts=[application_hud_font::HudText{
            text:"НАЙТ-СИТИ".to_owned(),
            origin:[80.0,40.0],size_px:22.0,tracking:0.3,
            max_width:500.0,color:[0.8,0.9,1.0,1.0],
        }];
        let draw=compose_ordered_hud(
            &rects,&texts,
            &[(false,0),(true,0),(false,1)],
            Some(&atlas),1920,1080,
        ).unwrap();
        assert_eq!(draw.mesh.cmds.len(),3);
        assert_eq!(draw.mesh.cmds[0].texture,reserved::SOLID_WHITE);
        assert_eq!(draw.mesh.cmds[1].texture,application_hud_font::HUD_FONT_TEXTURE);
        assert_eq!(draw.mesh.cmds[2].texture,reserved::SOLID_WHITE);
        assert!(draw.mesh.cmds[0].index_range.end
            <=draw.mesh.cmds[1].index_range.start);
        assert!(draw.mesh.cmds[1].index_range.end
            <=draw.mesh.cmds[2].index_range.start);
        assert_eq!(draw.screen_size_px,[1920,1080]);
    }

    #[test]
    fn invalid_painter_index_is_reported_not_silently_ignored() {
        let result=compose_ordered_hud(&[],&[],&[(false,1)],None,1920,1080);
        assert!(result.is_err());
    }

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
