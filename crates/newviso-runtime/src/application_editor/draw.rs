//! Renderer-neutral solid line mesh, ordered below the editor's panels.
use newviso_editor_viewport::ScreenLine;
use newviso_ui_draw::*;
use serde_json::Value;
pub(super) fn prepend_lines(
    value: &mut Value,
    lines: &[ScreenLine],
    size: [u32; 2],
    dpi: f32,
) -> Result<(), String> {
    if lines.is_empty() {
        return Ok(());
    }
    let mut list: UiDrawList = serde_json::from_value(value.take())
        .map_err(|e| format!("Editor overlay UI decode: {e}"))?;
    let scale = dpi.max(0.0001);
    let mut mesh = UiMesh::new();
    for line in lines {
        let dx = line.b[0] - line.a[0];
        let dy = line.b[1] - line.a[1];
        let length = dx.hypot(dy);
        if length < 0.01 {
            continue;
        }
        let n = [
            -dy / length * line.width * 0.5,
            dx / length * line.width * 0.5,
        ];
        let base = mesh.vertices.len() as u32;
        for p in [
            [line.a[0] + n[0], line.a[1] + n[1]],
            [line.a[0] - n[0], line.a[1] - n[1]],
            [line.b[0] - n[0], line.b[1] - n[1]],
            [line.b[0] + n[0], line.b[1] + n[1]],
        ] {
            mesh.vertices.push(UiVertex {
                pos: [p[0] as f32 / scale, p[1] as f32 / scale],
                uv: [0.5, 0.5],
                color: u32::from_le_bytes(line.color),
            });
        }
        mesh.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let index_offset = mesh.indices.len() as u32;
    let vertex_offset = mesh.vertices.len() as u32;
    mesh.cmds.push(UiDrawCmd {
        texture: reserved::SOLID_WHITE,
        clip_rect: UiRect {
            min_x: 0.0,
            min_y: 0.0,
            max_x: size[0] as f32,
            max_y: size[1] as f32,
        },
        index_range: 0..index_offset,
    });
    mesh.vertices.append(&mut list.mesh.vertices);
    mesh.indices
        .extend(list.mesh.indices.iter().map(|v| v + vertex_offset));
    for mut cmd in list.mesh.cmds {
        cmd.index_range = cmd.index_range.start + index_offset..cmd.index_range.end + index_offset;
        mesh.cmds.push(cmd);
    }
    list.mesh = mesh;
    *value = serde_json::to_value(list).map_err(|e| e.to_string())?;
    Ok(())
}
/// Action-only legacy providers expose no hit identities. Their opaque panel
/// backgrounds still cover triangles in the neutral draw list, including live
/// resized docks. Testing those pixels avoids guesses about panel widths.
pub(super) fn ui_hit(value: &Value, p: [f64; 2], dpi: f32) -> bool {
    let Some(vertices) = value.pointer("/mesh/vertices").and_then(Value::as_array) else {
        return false;
    };
    let Some(indices) = value.pointer("/mesh/indices").and_then(Value::as_array) else {
        return false;
    };
    let Some(cmds) = value.pointer("/mesh/cmds").and_then(Value::as_array) else {
        return false;
    };
    let scale = f64::from(dpi.max(0.0001));
    let p = [p[0] / scale, p[1] / scale];
    for cmd in cmds {
        let clip = &cmd["clip_rect"];
        if p[0] * scale < clip["min_x"].as_f64().unwrap_or(0.0)
            || p[1] * scale < clip["min_y"].as_f64().unwrap_or(0.0)
            || p[0] * scale > clip["max_x"].as_f64().unwrap_or(f64::INFINITY)
            || p[1] * scale > clip["max_y"].as_f64().unwrap_or(f64::INFINITY)
        {
            continue;
        }
        let start = cmd["index_range"]["start"].as_u64().unwrap_or(0) as usize;
        let end = (cmd["index_range"]["end"].as_u64().unwrap_or(0) as usize).min(indices.len());
        if start > end {
            continue;
        }
        for triangle in indices[start..end].chunks_exact(3) {
            let mut v = [[0.0; 2]; 3];
            let mut opaque = true;
            for i in 0..3 {
                let Some(vertex) = triangle[i]
                    .as_u64()
                    .and_then(|id| vertices.get(id as usize))
                else {
                    opaque = false;
                    break;
                };
                if vertex["color"].as_u64().unwrap_or(0) >> 24 < 16 {
                    opaque = false;
                    break;
                }
                v[i] = [
                    vertex["pos"][0].as_f64().unwrap_or(0.0),
                    vertex["pos"][1].as_f64().unwrap_or(0.0),
                ];
            }
            if !opaque {
                continue;
            }
            let cross = |a: [f64; 2], b: [f64; 2], q: [f64; 2]| {
                (b[0] - a[0]) * (q[1] - a[1]) - (b[1] - a[1]) * (q[0] - a[0])
            };
            let area = cross(v[0], v[1], v[2]);
            if area.abs() < 0.01 {
                continue;
            }
            let signs = [
                cross(v[0], v[1], p),
                cross(v[1], v[2], p),
                cross(v[2], v[0], p),
            ];
            if signs.iter().all(|v| *v >= 0.0) || signs.iter().all(|v| *v <= 0.0) {
                return true;
            }
        }
    }
    false
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlay_preserves_texture_delta_and_rebases_provider_commands() {
        let mut list = UiDrawList::new();
        list.mesh.vertices = vec![
            UiVertex {
                pos: [2.0, 3.0],
                uv: [0.0; 2],
                color: 0xffffffff
            };
            3
        ];
        list.mesh.indices = vec![0, 1, 2];
        list.mesh.cmds.push(UiDrawCmd {
            texture: UiTexId(1),
            clip_rect: UiRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 100.0,
                max_y: 100.0,
            },
            index_range: 0..3,
        });
        let mut value = serde_json::to_value(list).unwrap();
        prepend_lines(
            &mut value,
            &[ScreenLine {
                a: [10.0, 10.0],
                b: [20.0, 10.0],
                color: [255; 4],
                width: 2.0,
            }],
            [100, 100],
            1.0,
        )
        .unwrap();
        let list: UiDrawList = serde_json::from_value(value).unwrap();
        assert_eq!(list.mesh.indices, vec![0, 1, 2, 0, 2, 3, 4, 5, 6]);
        assert_eq!(list.mesh.cmds[1].index_range, 6..9);
        assert_eq!(list.mesh.cmds[0].texture, reserved::SOLID_WHITE);
    }
    #[test]
    fn ui_occlusion_uses_actual_geometry_and_clip() {
        let mut list = UiDrawList::new();
        list.mesh.vertices = [[0.0, 0.0], [100.0, 0.0], [0.0, 100.0]]
            .map(|pos| UiVertex {
                pos,
                uv: [0.0; 2],
                color: 0xff222222,
            })
            .to_vec();
        list.mesh.indices = vec![0, 1, 2];
        list.mesh.cmds.push(UiDrawCmd {
            texture: UiTexId(1),
            clip_rect: UiRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 100.0,
                max_y: 100.0,
            },
            index_range: 0..3,
        });
        let value = serde_json::to_value(list).unwrap();
        assert!(ui_hit(&value, [20.0, 20.0], 1.0));
        assert!(!ui_hit(&value, [90.0, 90.0], 1.0));
        assert!(!ui_hit(&value, [120.0, 20.0], 1.0));
    }
}

/// Infer dock edges from opaque provider panel geometry. The deployed provider
/// exposes no layout rectangles; this follows actual resized panels without
/// baking their widths or the bottom panel's legacy size clamp into navigation.
pub(super) fn viewport_rect(value: &Value, size: [u32; 2], dpi: f32) -> [f64; 4] {
    let w = size[0] as f64;
    let h = size[1] as f64;
    let mut rect = [0.0, 0.0, w, h];
    let scale = f64::from(dpi.max(0.0001));
    let edge = 4.0 * scale;
    let Some(vertices) = value.pointer("/mesh/vertices").and_then(Value::as_array) else {
        return rect;
    };
    let Some(indices) = value.pointer("/mesh/indices").and_then(Value::as_array) else {
        return rect;
    };
    let Some(cmds) = value.pointer("/mesh/cmds").and_then(Value::as_array) else {
        return rect;
    };
    for cmd in cmds {
        let start = cmd["index_range"]["start"].as_u64().unwrap_or(0) as usize;
        let end = (cmd["index_range"]["end"].as_u64().unwrap_or(0) as usize).min(indices.len());
        if start > end {
            continue;
        }
        for ids in indices[start..end].chunks_exact(3) {
            let mut min = [f64::INFINITY; 2];
            let mut max = [f64::NEG_INFINITY; 2];
            let mut color = None;
            let mut solid = true;
            for id in ids {
                let Some(v) = id.as_u64().and_then(|id| vertices.get(id as usize)) else {
                    solid = false;
                    break;
                };
                let c = v["color"].as_u64().unwrap_or(0);
                if c >> 24 < 240 || color.is_some_and(|previous| previous != c) {
                    solid = false;
                    break;
                }
                color = Some(c);
                for i in 0..2 {
                    let n = v["pos"][i].as_f64().unwrap_or(0.0) * scale;
                    min[i] = min[i].min(n);
                    max[i] = max[i].max(n);
                }
            }
            if !solid {
                continue;
            }
            let clip = &cmd["clip_rect"];
            min[0] = min[0].max(clip["min_x"].as_f64().unwrap_or(0.0));
            min[1] = min[1].max(clip["min_y"].as_f64().unwrap_or(0.0));
            max[0] = max[0].min(clip["max_x"].as_f64().unwrap_or(w));
            max[1] = max[1].min(clip["max_y"].as_f64().unwrap_or(h));
            let width = max[0] - min[0];
            let height = max[1] - min[1];
            if width > w * 0.5 && height > 12.0 * scale && height < h * 0.5 {
                if min[1] <= edge {
                    rect[1] = rect[1].max(max[1]);
                }
                if max[1] >= h - edge {
                    rect[3] = rect[3].min(min[1]);
                }
            }
            if height > h * 0.3 && width > 80.0 * scale && width < w * 0.65 {
                if min[0] <= edge {
                    rect[0] = rect[0].max(max[0]);
                }
                if max[0] >= w - edge {
                    rect[2] = rect[2].min(min[0]);
                }
            }
        }
    }
    if rect[2] - rect[0] < 64.0 || rect[3] - rect[1] < 64.0 {
        [0.0, 0.0, w, h]
    } else {
        rect
    }
}

#[cfg(test)]
mod dock_tests {
    use super::*;
    fn draw(dpi: f32) -> Value {
        let mut d = UiDrawList::new();
        d.screen_size_px = [(1280.0 * dpi) as u32, (800.0 * dpi) as u32];
        d.pixels_per_point = dpi;
        for [x, y, w, h] in [
            [0.0, 0.0, 1280.0, 38.0],
            [0.0, 38.0, 310.0, 582.0],
            [810.0, 38.0, 470.0, 582.0],
            [0.0, 620.0, 1280.0, 180.0],
        ] {
            let base = d.mesh.vertices.len() as u32;
            for pos in [[x, y], [x + w, y], [x + w, y + h], [x, y + h]] {
                d.mesh.vertices.push(UiVertex {
                    pos,
                    uv: [0.0; 2],
                    color: 0xffeeeeee,
                });
            }
            d.mesh
                .indices
                .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        d.mesh.cmds.push(UiDrawCmd {
            texture: UiTexId(1),
            clip_rect: UiRect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: 1280.0 * dpi,
                max_y: 800.0 * dpi,
            },
            index_range: 0..d.mesh.indices.len() as u32,
        });
        serde_json::to_value(d).unwrap()
    }
    #[test]
    fn focus_rect_follows_provider_panels_in_physical_pixels() {
        assert_eq!(
            viewport_rect(&draw(1.0), [1280, 800], 1.0),
            [310.0, 38.0, 810.0, 620.0]
        );
        assert_eq!(
            viewport_rect(&draw(2.0), [2560, 1600], 2.0),
            [620.0, 76.0, 1620.0, 1240.0]
        );
    }
}
