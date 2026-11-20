"""One-time migration: preserve authored rectangle / Unicode text painter order.

Prior implementation partitioned quads and text separately, making all text
draw over all rectangles and potentially hiding later panel/border primitives.
"""
from pathlib import Path
import shutil
root=Path(__file__).resolve().parent.parent
lib=root/'crates/newviso-runtime/src/lib.rs'
hud=root/'crates/newviso-runtime/src/application_hud_overlay.rs'
font=root/'crates/newviso-runtime/src/application_hud_font.rs'
backup=root/'backups/2026-11-12-hud-painter-order'
for source in (lib,hud,font):
 dst=backup/source.relative_to(root)
 dst.parent.mkdir(parents=True,exist_ok=True)
 if not dst.exists():shutil.copy2(source,dst)

def replace(s,old,new):
 assert s.count(old)==1,(old[:110],s.count(old))
 return s.replace(old,new)

s=lib.read_text(encoding='utf-8')
s=replace(s,'    hud_texts: Vec<application_hud_font::HudText>,',
    '''    hud_texts: Vec<application_hud_font::HudText>,
    /// Authored painter order: false = quad, true = Unicode text.
    hud_item_order: Vec<(bool, usize)>,''')
s=replace(s,'            hud_texts: Vec::new(),',
    '''            hud_texts: Vec::new(),
            hud_item_order: Vec::new(),''')
lib.write_text(s,encoding='utf-8')

s=hud.read_text(encoding='utf-8')
needle='pub(super) fn make_hud_draw_list('
append=r'''/// Append a single authored solid primitive in UI pixels, merging only
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

'''
assert needle in s
s=s.replace(needle,append+needle,1)
a=s.index('    let (w, h) = (width.max(1) as f32, height.max(1) as f32);',s.index(needle))
b=s.index('    serde_json::to_value(draw).map(Some)',a)
new=r'''    let mut draw=UiDrawList::new();
    draw.screen_size_px=[width.max(1),height.max(1)];
    draw.pixels_per_point=1.0;
    draw.mesh.vertices.reserve(quads.len()*4);
    draw.mesh.indices.reserve(quads.len()*6);
    for quad in quads {
        append_hud_rect(&mut draw,quad)?;
    }
    if draw.mesh.indices.is_empty() {return Ok(None);}
'''
s=s[:a]+new+s[b:]
s=replace(s,'''        let mut quads=Vec::new();
        let mut texts=Vec::new();''',
'''        let mut quads=Vec::new();
        let mut texts=Vec::new();
        let mut order=Vec::with_capacity(items.len());''')
s=replace(s,'''                texts.push(application_hud_font::HudText {''',
'''                order.push((true,texts.len()));
                texts.push(application_hud_font::HudText {''')
s=replace(s,'''                quads.push(SceneOverlayQuad {rect,color});''',
'''                order.push((false,quads.len()));
                quads.push(SceneOverlayQuad {rect,color});''')
s=replace(s,'''        self.hud_quads=quads;
        self.hud_texts=texts;''',
'''        self.hud_quads=quads;
        self.hud_texts=texts;
        self.hud_item_order=order;''')
a=s.index('        let mut draw:UiDrawList=match make_hud_draw_list(')
b=s.index('        let normal=serde_json::to_value(&draw)',a)
new=r'''        let mut draw=UiDrawList::new();
        draw.screen_size_px=[width.max(1),height.max(1)];
        draw.pixels_per_point=1.0;
        let font=self.hud_font.as_ref();
        for (text,index) in self.hud_item_order.iter().copied() {
            if text {
                let value=self.hud_texts.get(index)
                    .ok_or_else(||format!("HUD text order index {index} is invalid"))?;
                font.ok_or("HUD text atlas is missing")?
                    .append_glyphs(&mut draw,std::slice::from_ref(value))?;
            } else {
                let value=self.hud_quads.get(index)
                    .ok_or_else(||format!("HUD rect order index {index} is invalid"))?;
                append_hud_rect(&mut draw,value)?;
            }
        }
'''
s=s[:a]+new+s[b:]
hud.write_text(s,encoding='utf-8')
s=font.read_text(encoding='utf-8')
s=replace(s,'''        if end>start {
            draw.mesh.cmds.push(UiDrawCmd{
                texture:HUD_FONT_TEXTURE,
                clip_rect:UiRect{min_x:0.0,min_y:0.0,max_x:w,max_y:h},
                index_range:start..end,
            });
        }''',
'''        if end>start {
            if let Some(last)=draw.mesh.cmds.last_mut() {
                if last.texture==HUD_FONT_TEXTURE && last.index_range.end==start {
                    last.index_range.end=end;
                    return Ok(());
                }
            }
            draw.mesh.cmds.push(UiDrawCmd{
                texture:HUD_FONT_TEXTURE,
                clip_rect:UiRect{min_x:0.0,min_y:0.0,max_x:w,max_y:h},
                index_range:start..end,
            });
        }''')
font.write_text(s,encoding='utf-8')
print('HUD_PAINTER_ORDER_FIXED_AND_BACKED_UP',backup)
