from pathlib import Path
import shutil
root=Path(__file__).resolve().parent.parent
files=['crates/newviso-platform/src/lib.rs','crates/newviso-runtime/src/bootstrap.rs']
backup=root/'backups/2026-11-12-hud-fullscreen-config'
for rel in files:
 p=root/rel
 target=backup/rel
 target.parent.mkdir(parents=True,exist_ok=True)
 if not target.exists():shutil.copy2(p,target)
p=root/files[0];s=p.read_text(encoding='utf-8')
replacements=[
('NativeWindowBackendV1, PlatformAppConfigV1, PlatformCursorGrabModeV1,',
 'NativeWindowBackendV1, PlatformAppConfigV1, PlatformWindowModeV1, PlatformCursorGrabModeV1,'),
('    pub frame_limit: Option<u64>,\n}\n\nimpl Default for PlatformRunConfig {',
 '''    pub frame_limit: Option<u64>,
    pub window_mode: PlatformWindowModeV1,
}

pub fn parse_window_mode(value: Option<&str>) -> Result<PlatformWindowModeV1,String> {
    match value.unwrap_or("windowed").trim().to_ascii_lowercase().as_str() {
        "windowed" | "window" => Ok(PlatformWindowModeV1::Windowed),
        "borderless" | "borderless_fullscreen" => Ok(PlatformWindowModeV1::Borderless),
        "exclusive" | "exclusive_fullscreen" => Ok(PlatformWindowModeV1::ExclusiveFullscreen),
        other => Err(format!("unknown window mode '{other}'")),
    }
}

impl Default for PlatformRunConfig {'''),
('            frame_limit: None,\n        }\n    }\n}',
 '            frame_limit: None,\n            window_mode: PlatformWindowModeV1::Windowed,\n        }\n    }\n}'),
('    let app_config = PlatformAppConfigV1::new(config.title, config.width, config.height);\n\n    let result = unsafe { run(plugin_host, api, app_config) };',
 '''    let mut app_config = PlatformAppConfigV1::new(config.title, config.width, config.height);
    app_config.display.window_mode = config.window_mode;
    host::info("newviso.platform", format!("requested window mode={:?}",config.window_mode));
    let result = unsafe { run(plugin_host, api, app_config) };''')
]
for a,b in replacements:
 assert s.count(a)==1,('platform',a[:75],s.count(a))
 s=s.replace(a,b)
s+='''

#[cfg(test)]
mod presentation_mode_tests {
    use super::*;
    #[test]
    fn window_mode_parses_supported_runtime_strings() {
        assert_eq!(parse_window_mode(None).unwrap(),PlatformWindowModeV1::Windowed);
        assert_eq!(parse_window_mode(Some("borderless")).unwrap(),PlatformWindowModeV1::Borderless);
        assert_eq!(parse_window_mode(Some("exclusive_fullscreen")).unwrap(),PlatformWindowModeV1::ExclusiveFullscreen);
        assert!(parse_window_mode(Some("unsupported")).is_err());
    }
}
'''
p.write_text(s,encoding='utf-8')
p=root/files[1];s=p.read_text(encoding='utf-8')
a='                    frame_limit: bootstrap.max_frames,\n                },'
b='''                    frame_limit: bootstrap.max_frames,
                    window_mode: newviso_platform::parse_window_mode(
                        runtime_settings.variables.get("platform_display")
                            .and_then(|display|display.get("window_mode"))
                            .and_then(serde_json::Value::as_str),
                    )?,
                },'''
assert s.count(a)==1
p.write_text(s.replace(a,b),encoding='utf-8')
print('FULLSCREEN_LAUNCH_CONFIG_PATCHED')
