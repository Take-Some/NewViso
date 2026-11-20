//! Provider-neutral playback of source REDengine Ink interpolator timelines.
//! The asset and node bindings are project-owned data. The runtime evaluates
//! original source interpolation values, never invents a substitute animation.
//! Unbound tracks (EULA, patch notifications, video) stay available for their
//! relevant authored surfaces; they are not incorrectly projected onto buttons.
use super::*;

include!("application_ink_source_events.rs");

#[derive(Clone, Debug)]
pub(super) struct InkMenuPlayback {
    asset: String,
    bindings: Value,
    intro_clip: String,
    repeat_clip: String,
    hover_enter_clip: String,
    hover_leave_clip: String,
    library: Option<Value>,
    failed: bool,
    elapsed: f64,
    hover_elapsed: f64,
    hovering: bool,
    hover_clip: Option<String>,
}

impl InkMenuPlayback {
    pub(super) fn from_template(template: &Value) -> Option<Self> {
        let setup = template.pointer("/metrics/ink_animation")?;
        let asset = setup.get("asset")?.as_str()?.trim();
        if asset.is_empty() { return None; }
        Some(Self {
            asset: asset.to_owned(),
            bindings: setup.get("bindings").cloned().unwrap_or(Value::Null),
            intro_clip: setup.pointer("/clips/intro").and_then(Value::as_str).unwrap_or("intro").to_owned(),
            repeat_clip: setup.pointer("/clips/loop").and_then(Value::as_str).unwrap_or("loop").to_owned(),
            hover_enter_clip: setup.pointer("/clips/hover_enter").and_then(Value::as_str).unwrap_or("pause_button_hover_over_anim").to_owned(),
            hover_leave_clip: setup.pointer("/clips/hover_leave").and_then(Value::as_str).unwrap_or("pause_button_hover_out_anim").to_owned(),
            library: None,
            failed: false,
            elapsed: 0.0,
            hover_elapsed: 0.0,
            hovering: false,
            hover_clip: None,
        })
    }

    pub(super) fn ensure_loaded(&mut self) {
        if self.library.is_some() || self.failed { return; }
        let result = AssetClient::new().json(&self.asset);
        match result {
            Ok(source) if source["schema"] == "newviso.ink.animation_source.v1"
                && source["clips"].as_array().is_some() =>
            {
                let clip_count = source["clips"].as_array().map_or(0, Vec::len);
                let tracks = source["clips"].as_array().unwrap().iter()
                    .map(|clip| clip["tracks"].as_array().map_or(0, Vec::len))
                    .sum::<usize>();
                host::info("newviso.ink", format!(
                    "mounted original Ink animation timeline asset='{}' clips={} tracks={}",
                    self.asset, clip_count, tracks
                ));
                self.library = Some(source);
            }
            Ok(_) => {
                host::warn("newviso.ink", format!(
                    "invalid or unsupported Ink animation library asset='{}'", self.asset
                ));
                self.failed = true;
            }
            Err(error) => {
                host::warn("newviso.ink", format!(
                    "Ink animation asset '{}' unavailable: {}", self.asset, error
                ));
                self.failed = true;
            }
        }
    }

    pub(super) fn advance(&mut self, dt: f32, hovered_action: Option<&str>) {
        self.elapsed = (self.elapsed + f64::from(dt.clamp(0.0, 0.1))).min(1.0e10);
        // Only real action hit-tests trigger the original hover animation.
        // An external background image may itself be the hovered node.
        let now_hovering = hovered_action.is_some_and(|action| {
            action.starts_with("newviso.menu.")
        });
        if now_hovering != self.hovering {
            self.hovering = now_hovering;
            self.hover_clip = Some(if now_hovering {
                self.hover_enter_clip.clone()
            } else {
                self.hover_leave_clip.clone()
            });
            self.hover_elapsed = 0.0;
        } else {
            self.hover_elapsed = (self.hover_elapsed + f64::from(dt.clamp(0.0, 0.1))).min(1000.0);
        }
    }

    pub(super) fn apply(&self, surface: &mut Value, reduce_motion: bool) {
        let Some(source) = self.library.as_ref() else { return; };
        let intro_duration = clip_length(source, &self.intro_clip).max(0.001);
        let t = if reduce_motion { intro_duration } else { self.elapsed };
        if t <= intro_duration {
            apply_clip(surface, source, &self.bindings, &self.intro_clip, t);
        } else {
            // Maintain the final authored intro state while the repeat clip runs.
            // Without this the HUD snaps back to its initial template after intro.
            apply_clip(surface, source, &self.bindings, &self.intro_clip, intro_duration);
            // Source loop's Video interpolator sets a 5-second period.
            let period = clip_length(source, &self.repeat_clip).max(0.001);
            let loop_time = if reduce_motion { 0.0 } else {
                (t - intro_duration) % period
            };
            apply_clip(surface, source, &self.bindings, &self.repeat_clip, loop_time);
        }
        // Independently play original pause-menu hover curves for the user
        // input surface. Their node-path routing is authored in the project.
        if !reduce_motion {
            if let Some(clip) = self.hover_clip.as_deref() {
                apply_clip(surface, source, &self.bindings, clip, self.hover_elapsed);
            }
        }
    }
}

fn clip<'a>(source: &'a Value, name: &str) -> Option<&'a Value> {
    source.get("clips")?.as_array()?.iter()
        .find(|value| value.get("name").and_then(Value::as_str) == Some(name))
}

fn clip_length(source: &Value, name: &str) -> f64 {
    clip(source, name).and_then(|clip| clip["tracks"].as_array()).map(|tracks| {
        tracks.iter()
            .filter_map(|track| track["interpolators"].as_array())
            .flat_map(|interpolators| interpolators.iter())
            .map(|interpolator| {
                let start = interpolator.get("startDelay").and_then(Value::as_f64).unwrap_or(0.0);
                let len = interpolator.get("duration").and_then(Value::as_f64).unwrap_or(0.0);
                (start + len).max(0.0)
            })
            .fold(0.0, f64::max)
    }).unwrap_or(0.0)
}

fn sample_segment(interpolator: &Value, t: f64) -> Option<Value> {
    let start = interpolator.get("startValue")?;
    let end = interpolator.get("endValue")?;
    let from_time = interpolator.get("startDelay").and_then(Value::as_f64).unwrap_or(0.0);
    let duration = interpolator.get("duration").and_then(Value::as_f64)
        .unwrap_or(0.0).max(0.0);
    let fraction = if duration <= 1.0e-9 {
        if t >= from_time { 1.0 } else { 0.0 }
    } else {
        ((t - from_time) / duration).clamp(0.0, 1.0)
    };
    let exponent = match interpolator.get("interpolationType").and_then(Value::as_str) {
        Some("Quadratic") => 2,
        Some("Cubic") => 3,
        Some("Quartic") => 4,
        Some("Quintic") => 5,
        _ => 1,
    };
    let easing_mode = interpolator.get("interpolationMode").and_then(Value::as_str)
        .unwrap_or("EasyIn");
    let curve = match easing_mode {
        "EasyOut" => 1.0 - (1.0-fraction).powi(exponent),
        "EasyInOut" if fraction < 0.5 =>
            0.5*(fraction*2.0).powi(exponent),
        "EasyInOut" =>
            1.0-0.5*((1.0-fraction)*2.0).powi(exponent),
        _ => fraction.powi(exponent),
    };
    interpolate_value(start, end, curve)
}

fn interpolate_value(from: &Value, to: &Value, t: f64) -> Option<Value> {
    if let (Some(a), Some(b)) = (from.as_f64(), to.as_f64()) {
        return serde_json::Number::from_f64(a + (b-a)*t)
            .map(Value::Number);
    }
    let (Some(a),Some(b)) = (from.as_object(), to.as_object()) else {
        return Some(if t < 1.0 { from.clone() } else { to.clone() });
    };
    let mut output = serde_json::Map::new();
    for (key, initial) in a {
        let final_value = b.get(key).unwrap_or(initial);
        output.insert(key.clone(), if initial.is_number() && final_value.is_number() {
            interpolate_value(initial, final_value, t)?
        } else { initial.clone() });
    }
    Some(Value::Object(output))
}

fn sample_property(interpolators: &[Value], kind: &str, t: f64) -> Option<Value> {
    // Sorting matters: source tool emits unsorted overlapping sequences.
    let mut filtered: Vec<_> = interpolators.iter()
        .filter(|value| value.get("$type").and_then(Value::as_str) == Some(kind))
        .collect();
    filtered.sort_by(|a,b| {
        let t = |v: &Value| v.get("startDelay").and_then(Value::as_f64).unwrap_or(0.0);
        t(a).total_cmp(&t(b))
    });
    let first = filtered.first()?;
    let current = filtered.iter().rev()
        .find(|v| v.get("startDelay").and_then(Value::as_f64).unwrap_or(0.0) <= t)
        .copied().unwrap_or(first);
    sample_segment(current, t)
}

fn target_key(track: &Value) -> Option<String> {
    let path = track.get("target_path")?.as_array()?;
    Some(path.iter().map(|n| n.as_u64().map(|v| v.to_string()))
        .collect::<Option<Vec<_>>>()?.join("/"))
}

fn apply_clip(surface: &mut Value, source: &Value, bindings: &Value,
              clip_name: &str, t: f64) {
    let Some(tracks) = clip(source, clip_name)
        .and_then(|value| value["tracks"].as_array()) else { return; };
    let Some(routes) = bindings.get(clip_name).and_then(Value::as_object) else { return; };
    for track in tracks {
        let Some(path) = target_key(track) else { continue; };
        let Some(destination) = routes.get(&path) else { continue; };
        let ids: Vec<&str> = match destination {
            Value::String(id) => vec![id.as_str()],
            Value::Array(values) => values.iter().filter_map(Value::as_str).collect(),
            _ => continue,
        };
        let interpolators = track.get("interpolators").and_then(Value::as_array);
        for id in ids {
            let Some(components) = surface.get_mut("components").and_then(Value::as_array_mut) else {
                continue;
            };
            if let Some(node) = find_node_mut(components, id) {
                for (kind, property) in [
                    ("inkanimTransparencyInterpolator", "opacity"),
                    ("inkanimTranslationInterpolator", "translation"),
                    ("inkanimScaleInterpolator", "scale"),
                    ("inkanimSizeInterpolator", "size"),
                    ("inkanimRotationInterpolator", "rotation"),
                    ("inkanimMarginInterpolator", "margin"),
                    ("inkanimEffectInterpolator", "effect"),
                    ("inkanimTextReplaceInterpolator", "text_replace"),
                    ("inkanimVideoInterpolator", "video_position"),
                    ("inkanimColorInterpolator", "color"),
                    ("inkanimPivotInterpolator", "pivot"),
                    ("inkanimAnchorInterpolator", "anchor"),
                    ("inkanimShearInterpolator", "shear"),
                    ("inkanimTextOffsetInterpolator", "text_offset"),
                    ("inkanimTextValueProgressInterpolator", "text_value_progress"),
                ] {
                    if let Some(value) = interpolators.and_then(|items| sample_property(items, kind, t)) {
                        apply_property(node, property, &value);
                    }
                }
                if let Some(source_events) = track.get("events").and_then(Value::as_array) {
                    let mut ordered: Vec<_> = source_events.iter().take(2048).collect();
                    ordered.sort_by(|a,b| ink_event_time(a).total_cmp(&ink_event_time(b)));
                    for event in ordered { apply_ink_visual_event(node,event,t); }
                }
            }
        }
    }
}

fn find_node_mut<'a>(nodes: &'a mut [Value], id: &str) -> Option<&'a mut Value> {
    for node in nodes {
        if node.get("id").and_then(Value::as_str) == Some(id) {
            return Some(node);
        }
        if let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) {
            if let Some(found) = find_node_mut(children, id) { return Some(found); }
        }
    }
    None
}

fn apply_property(node: &mut Value, property: &str, value: &Value) {
    let channel = match node.get("component_id").and_then(Value::as_str) {
        Some("external_texture") => "tint_rgba",
        Some("text") => "text_rgba",
        _ => "fill_rgba",
    };
    let Some(props) = node.get_mut("props") else { return; };
    let gain = props.get("ink_alpha_gain").and_then(Value::as_f64).unwrap_or(1.0)
        .clamp(0.0, 1.0);
    match property {
        "opacity" => {
            let opacity = value.as_f64().unwrap_or(1.0).clamp(0.0, 1.0)*gain;
            let rgba = props.get(channel).and_then(Value::as_array).cloned()
                .unwrap_or_else(|| vec![json!(255),json!(255),json!(255),json!(255)]);
            if rgba.len() == 4 {
                let mut rgba = rgba;
                rgba[3] = json!((255.0*opacity).round() as u8);
                props[channel] = Value::Array(rgba);
            }
        }
        "translation" => {
            let scale = props.get("ink_source_unit_px").and_then(Value::as_f64)
                .unwrap_or(1600.0/3840.0);
            if let (Some(x),Some(y)) = (value.get("X").and_then(Value::as_f64),
                                         value.get("Y").and_then(Value::as_f64)) {
                let bx = props.get("x_px").and_then(Value::as_f64).unwrap_or(0.0);
                let by = props.get("y_px").and_then(Value::as_f64).unwrap_or(0.0);
                props["x_px"] = json!(bx+x*scale);
                props["y_px"] = json!(by+y*scale);
            }
        }
        "scale" => {
            if let (Some(x),Some(y)) = (value.get("X").and_then(Value::as_f64),
                                         value.get("Y").and_then(Value::as_f64)) {
                let w = props.get("w_px").and_then(Value::as_f64).unwrap_or(1.0);
                let h = props.get("h_px").and_then(Value::as_f64).unwrap_or(1.0);
                props["w_px"] = json!((w*x.abs()).max(0.01));
                props["h_px"] = json!((h*y.abs()).max(0.01));
            }
        }
        "size" => {
            let factor = props.get("ink_source_unit_px").and_then(Value::as_f64)
                .unwrap_or(1600.0/3840.0);
            if let (Some(x),Some(y)) = (value.get("X").and_then(Value::as_f64),
                                         value.get("Y").and_then(Value::as_f64)) {
                props["w_px"] = json!((x*factor).max(0.01));
                props["h_px"] = json!((y*factor).max(0.01));
            }
        }
        // The following typed values are evaluated and preserved as semantic
        // properties, but backend-specific effects/video rasterization remains
        // separate from the timeline interpreter.
        "rotation" => { props["ink_rotation_deg"] = value.clone(); }
        "margin" => { props["ink_margin"] = value.clone(); }
        "effect" => { props["ink_effect_intensity"] = value.clone(); }
        "text_replace" => { props["ink_text_state"] = value.clone(); }
        "video_position" => { props["ink_video_position"] = value.clone(); }
        "pivot" => { props["ink_render_pivot"] = value.clone(); }
        "anchor" => { props["ink_render_anchor"] = value.clone(); }
        "shear" => { props["ink_render_shear"] = value.clone(); }
        "text_offset" => { props["ink_text_offset"] = value.clone(); }
        "text_value_progress" => { props["ink_text_value_progress"] = value.clone(); }
        "color" => {
            let c = |name| value.get(name).and_then(Value::as_f64);
            if let (Some(red),Some(green),Some(blue),Some(alpha)) =
                (c("Red"),c("Green"),c("Blue"),c("Alpha")) {
                let base_alpha=props.get(channel).and_then(Value::as_array)
                    .and_then(|rgba|rgba.get(3)).and_then(Value::as_f64)
                    .unwrap_or(255.0);
                let byte=|v:f64| (v.clamp(0.0,1.0)*255.0).round() as u8;
                props[channel]=json!([
                    byte(red),byte(green),byte(blue),
                    (base_alpha*alpha.clamp(0.0,1.0)).round() as u8
                ]);
            } else {
                props["ink_color_state"]=value.clone();
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_preview_animates_actual_rendered_ink_nodes() {
        let Ok(path) = std::env::var("NEWVISO_INK_SOURCE_SURFACE_TEST_PATH") else {
            return;
        };
        let dir = std::path::Path::new(&path).parent().expect("source preview parent");
        let initial: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let source: Value = serde_json::from_slice(
            &std::fs::read(dir.join("singleplayer_menu.animations.json")).unwrap()
        ).unwrap();
        let bindings = initial.pointer("/metrics/ink_animation/bindings")
            .expect("original preview must include source animation bindings");
        let n = initial.pointer("/metrics/ink_animation_binding_stats/bound_tracks")
            .and_then(Value::as_u64).unwrap_or(0);
        assert!(n >= 15, "original clip route count regressed: {n}");
        assert_eq!(source["clips"].as_array().unwrap().len(),11);
        let mut at_start = initial.clone();
        let mut at_mid = initial.clone();
        let mut at_end = initial.clone();
        apply_clip(&mut at_start,&source,bindings,"intro",0.0);
        apply_clip(&mut at_mid,&source,bindings,"intro",0.3);
        apply_clip(&mut at_end,&source,bindings,"intro",0.8);
        assert!(at_start["components"]!=at_mid["components"]
            || at_mid["components"]!=at_end["components"],
            "source Ink animation has no visible target effect");
    }

    #[test]
    fn actual_source_evaluates_all_11_clips_and_199_interpolators_when_supplied() {
        let Ok(path) = std::env::var("NEWVISO_INK_SOURCE_TEST_PATH") else {
            return; // The Cyberpunk source installation is local to the project.
        };
        let bytes = std::fs::read(path).expect("local original Ink clip export");
        let source: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(source["schema"], "newviso.ink.animation_source.v1");
        let clips = source["clips"].as_array().unwrap();
        assert_eq!(clips.len(),11);
        let mut kinds = std::collections::BTreeSet::new();
        let mut count = 0;
        for clip in clips {
            let name = clip["name"].as_str().unwrap();
            assert!(clip_length(&source,name)>0.0);
            for track in clip["tracks"].as_array().unwrap() {
                let _ = target_key(track);
                for interpolator in track["interpolators"].as_array().unwrap() {
                    count += 1;
                    kinds.insert(interpolator["$type"].as_str().unwrap());
                    let start = interpolator["startDelay"].as_f64().unwrap();
                    let duration = interpolator["duration"].as_f64().unwrap();
                    assert!(start.is_finite() && duration.is_finite() && duration>=0.0);
                    let value = sample_segment(interpolator,start+duration*0.5).unwrap();
                    assert!(!value.is_null());
                }
            }
        }
        assert_eq!(count,199);
        assert_eq!(kinds.len(),9);
    }

    #[test]
    fn completed_intro_keeps_final_hud_frame_when_repeat_is_missing() {
        let data=json!({"clips":[{"name":"intro","tracks":[{
            "target_path":[0],"interpolators":[{
                "$type":"inkanimTransparencyInterpolator","startDelay":0.0,
                "duration":1.0,"startValue":0.0,"endValue":1.0
            }]
        }]}]});
        let setup=json!({"metrics":{"ink_animation":{
            "asset":"source","bindings":{"intro":{"0":"hud"}},
            "clips":{"intro":"intro"}
        }}});
        let mut p=InkMenuPlayback::from_template(&setup).unwrap();
        p.library=Some(data);
        p.elapsed=2.0;
        let mut target=json!({"components":[{"id":"hud",
            "component_id":"panel","props":{}}]});
        p.apply(&mut target,false);
        assert_eq!(target["components"][0]["props"]["fill_rgba"][3],255);
    }
    #[test]
    fn hover_is_triggered_only_by_a_real_menu_action() {
        let source = json!({"metrics":{"ink_animation":{
            "asset":"ui/source.ink.json",
            "clips":{"intro":"open","loop":"idle",
                     "hover_enter":"hover_on","hover_leave":"hover_off"},
            "bindings":{}
        }}});
        let mut playback=InkMenuPlayback::from_template(&source).unwrap();
        playback.advance(0.016,Some("menu.background.ytd"));
        assert!(!playback.hovering);
        assert!(playback.hover_clip.is_none());
        playback.advance(0.016,Some("newviso.menu.settings"));
        assert!(playback.hovering);
        assert_eq!(playback.hover_clip.as_deref(),Some("hover_on"));
        playback.advance(0.016,None);
        assert_eq!(playback.hover_clip.as_deref(),Some("hover_off"));
        assert_eq!(playback.intro_clip,"open");
        assert_eq!(playback.repeat_clip,"idle");
    }
    #[test]
    fn original_quadratic_ease_and_delayed_start() {
        let interpolator = json!({
            "startValue":0.0,"endValue":1.0,"startDelay":0.2,
            "duration":0.4,"interpolationType":"Quadratic",
            "interpolationMode":"EasyInOut"
        });
        assert_eq!(sample_segment(&interpolator,0.0),Some(json!(0.0)));
        let mid = sample_segment(&interpolator,0.4).unwrap().as_f64().unwrap();
        assert!((mid-0.5).abs()<1.0e-6);
        assert_eq!(sample_segment(&interpolator,0.9),Some(json!(1.0)));
    }
    #[test]
    fn source_order_does_not_override_temporal_interpolator_order() {
        let v = vec![
            json!({"$type":"inkanimTransparencyInterpolator","startDelay":1.0,
                   "duration":0.1,"startValue":0.2,"endValue":1.0}),
            json!({"$type":"inkanimTransparencyInterpolator","startDelay":0.0,
                   "duration":0.5,"startValue":0.0,"endValue":0.2})
        ];
        let n = sample_property(&v,"inkanimTransparencyInterpolator",0.25).unwrap();
        assert!((n.as_f64().unwrap()-0.1).abs()<1.0e-6);
    }
    #[test]
    fn original_intro_logo_track_targets_authored_logo_position_and_opacity() {
        let source = json!({"clips":[{"name":"intro","tracks":[{"target_path":[7,0],
          "interpolators":[
            {"$type":"inkanimTransparencyInterpolator","startDelay":0,
             "duration":0.128,"startValue":0,"endValue":1},
            {"$type":"inkanimTranslationInterpolator","startDelay":0.05,
             "duration":0.25,"startValue":{"X":-50,"Y":0},"endValue":{"X":0,"Y":0}}
          ]}]}]});
        let bindings = json!({"intro":{"7/0":"menu.logo"}});
        let mut surface = json!({"components":[{"id":"menu.logo","component_id":"external_texture",
            "props":{"x_px":100,"y_px":100,"w_px":300,"h_px":200}}]});
        apply_clip(&mut surface,&source,&bindings,"intro",0.0);
        assert_eq!(surface["components"][0]["props"]["tint_rgba"][3],0);
        assert!((surface["components"][0]["props"]["x_px"].as_f64().unwrap()-
            (100.0-50.0*1600.0/3840.0)).abs()<0.01);
        // Runtime re-materializes the immutable template every frame.
        let mut surface = json!({"components":[{"id":"menu.logo","component_id":"external_texture",
            "props":{"x_px":100,"y_px":100,"w_px":300,"h_px":200}}]});
        apply_clip(&mut surface,&source,&bindings,"intro",0.5);
        assert_eq!(surface["components"][0]["props"]["tint_rgba"][3],255);
        assert_eq!(surface["components"][0]["props"]["x_px"],100.0);
    }
    #[test]
    fn source_visual_events_are_timed_and_do_not_need_interpolators() {
        let source=json!({"clips":[{"name":"hud_open","tracks":[{
            "target_path":[1],"interpolators":[],"events":[
                {"Data":{"$type":"inkanimSetVisibilityEvent",
                         "startTime":0.4,"isVisible":0}},
                {"Data":{"$type":"inkanimChangeStateEvent",
                         "startTime":0.8,"state":{"$value":"Selected"}}},
                {"Data":{"$type":"inkanimChangeTexturePartEvent",
                         "startTime":1.2,"imageTexturePartName":{"$value":"health_low"}}}
            ]}]}]});
        let bindings=json!({"hud_open":{"1":"hud.health"}});
        let template=json!({"components":[{"id":"hud.health",
            "component_id":"external_texture",
            "props":{"visible":true,"ink_part":"health_full"}}]});
        let mut at_early=template.clone();
        apply_clip(&mut at_early,&source,&bindings,"hud_open",0.2);
        assert_eq!(at_early["components"][0]["props"]["visible"],true);
        let mut at_mid=template.clone();
        apply_clip(&mut at_mid,&source,&bindings,"hud_open",0.9);
        assert_eq!(at_mid["components"][0]["props"]["visible"],false);
        assert_eq!(at_mid["components"][0]["props"]["ink_state"],"Selected");
        assert_eq!(at_mid["components"][0]["props"]["ink_part"],"health_full");
        let mut at_late=template.clone();
        apply_clip(&mut at_late,&source,&bindings,"hud_open",1.4);
        assert_eq!(at_late["components"][0]["props"]["ink_part"],"health_low");
        assert_eq!(at_late["components"][0]["props"]
                   ["ink_part_requires_runtime_atlas_binding"],true);
    }

    #[test]
    fn source_hdr_color_blends_with_existing_opacity() {
        let mut node=json!({"component_id":"external_texture",
            "props":{"tint_rgba":[255,255,255,128]}});
        let color=json!({"$type":"HDRColor",
            "Red":1.0,"Green":0.25,"Blue":0.5,"Alpha":0.5});
        apply_property(&mut node,"color",&color);
        assert_eq!(node["props"]["tint_rgba"],json!([255,64,128,64]));
    }

    #[test]
    fn source_clip_supports_all_nine_interpolator_value_types() {
        let values = [
            json!(0.0),json!({"X":2.0,"Y":7.0}),json!({"left":12.0,"top":3.0}),
        ];
        assert_eq!(interpolate_value(&values[1],&json!({"X":4.0,"Y":9.0}),0.5),
            Some(json!({"X":3.0,"Y":8.0})));
        assert_eq!(interpolate_value(&values[2],&json!({"left":16.0,"top":5.0}),0.5),
            Some(json!({"left":14.0,"top":4.0})));
        let source = json!({"clips":[{"name":"t","tracks":[{"target_path":[0],
            "interpolators":[
                {"$type":"inkanimRotationInterpolator","startValue":0,"endValue":360,"duration":1},
                {"$type":"inkanimTextReplaceInterpolator","startValue":0,"endValue":1,"duration":1},
                {"$type":"inkanimVideoInterpolator","startValue":0,"endValue":0.223,"duration":1},
                {"$type":"inkanimEffectInterpolator","startValue":0.3,"endValue":0,"duration":1},
                {"$type":"inkanimMarginInterpolator","startValue":{"left":0,"top":0},"endValue":{"left":20,"top":8},"duration":1},
                {"$type":"inkanimSizeInterpolator","startValue":{"X":100,"Y":100},"endValue":{"X":200,"Y":200},"duration":1}
            ]}]}]});
        let mut surface=json!({"components":[{"id":"test","component_id":"panel",
            "props":{"w_px":90,"h_px":90}}]});
        apply_clip(&mut surface,&source,&json!({"t":{"0":"test"}}),"t",0.5);
        let props=&surface["components"][0]["props"];
        assert_eq!(props["ink_rotation_deg"],180.0);
        assert_eq!(props["ink_text_state"],0.5);
        assert_eq!(props["ink_effect_intensity"],0.15);
        assert!(props["ink_video_position"].as_f64().unwrap()>0.1);
        assert_eq!(props["ink_margin"]["left"],10.0);
        assert!((props["w_px"].as_f64().unwrap()-62.5).abs()<0.01);
    }
}
