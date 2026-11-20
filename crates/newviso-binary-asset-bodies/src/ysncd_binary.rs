//! Strict binary decoder for NEF8 `.ysncd` sound-cue dictionaries.
//!
//! `engine.assets` owns the NEF8 envelope. This module owns only the inflated
//! YSNCD domain body and verifies each embedded encoded-audio payload hash.

use serde::{Deserialize, Serialize};

pub const YSNCD_BINARY_MAGIC: [u8; 4] = *b"YSNC";
pub const YSNCD_BINARY_SCHEMA_VERSION: u16 = 1;
const HEADER_LEN: usize = 64;
const CUE_RECORD_LEN: usize = 40;
const CLIP_RECORD_LEN: usize = 88;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YsncdAttenuation {
    pub min_distance: f32,
    pub max_distance: f32,
    pub curve: String,
    pub rolloff: f32,
    pub curve_points: Vec<[f32; 2]>,
}

impl Default for YsncdAttenuation {
    fn default() -> Self {
        Self {
            min_distance: 1.0,
            max_distance: 100.0,
            curve: "inverse".to_owned(),
            rolloff: 1.0,
            curve_points: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YsncdLayerDescriptor {
    pub name: String,
    pub role: String,
    pub clip_names: Vec<String>,
    pub gain: f32,
    pub pitch: f32,
    pub attenuation: Option<YsncdAttenuation>,
}

impl Default for YsncdLayerDescriptor {
    fn default() -> Self {
        Self {
            name: "body".to_owned(),
            role: "body".to_owned(),
            clip_names: Vec::new(),
            gain: 1.0,
            pitch: 1.0,
            attenuation: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YsncdCueDescriptor {
    /// Opaque logical mix route. Legacy `bus` is accepted only at the asset-loader boundary.
    #[serde(alias = "bus")]
    pub route: String,
    pub looping: bool,
    pub concurrency_group: String,
    pub concurrency_limit: usize,
    pub concurrency_scope: String,
    pub steal_rule: String,
    pub voice_budget: String,
    pub priority: i32,
    pub repeat_avoidance: usize,
    pub spatial_policy: String,
    pub gain_range: [f32; 2],
    pub pitch_range: [f32; 2],
    pub attenuation: Option<YsncdAttenuation>,
    pub layers: Vec<YsncdLayerDescriptor>,
    /// Optional typed trigger graph. Mutually exclusive with legacy `layers`.
    pub sound_graph: Option<crate::ysncd_sound_graph::YsncdSoundGraph>,
}

impl Default for YsncdCueDescriptor {
    fn default() -> Self {
        Self {
            route: String::new(),
            looping: false,
            concurrency_group: String::new(),
            concurrency_limit: 1,
            concurrency_scope: "global".to_owned(),
            steal_rule: "lower_priority_then_oldest".to_owned(),
            voice_budget: String::new(),
            priority: 0,
            repeat_avoidance: 0,
            spatial_policy: "inherit".to_owned(),
            gain_range: [1.0, 1.0],
            pitch_range: [1.0, 1.0],
            attenuation: None,
            layers: Vec::new(),
            sound_graph: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct YsncdClip {
    pub name: String,
    pub source: String,
    pub codec: String,
    pub weight: f32,
    pub gain: f32,
    pub pitch: f32,
    pub bytes: Vec<u8>,
    pub payload_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq)]
pub struct YsncdCue {
    pub name: String,
    pub stable_hash: u64,
    pub descriptor: YsncdCueDescriptor,
    pub clips: Vec<YsncdClip>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct YsncdDictionary {
    pub cues: Vec<YsncdCue>,
}

impl YsncdDictionary {
    #[inline]
    pub fn cue(&self, name: &str) -> Option<&YsncdCue> {
        self.cues
            .iter()
            .find(|cue| cue.name.eq_ignore_ascii_case(name))
    }
}

/// Encode the canonical inflated YSNCD-v1 domain body.
///
/// The NEF8 envelope is intentionally not owned here: callers may choose build/profile
/// metadata and compression policy through `newengine-assets-api::encode_list_file`.
/// Clip payload hashes are always recomputed from the embedded bytes so authored stale
/// hashes can never leak into a production dictionary.
fn validate_cue_sound_graph(cue: &YsncdCue) -> Result<(), String> {
    if cue.descriptor.sound_graph.is_some() && !cue.descriptor.layers.is_empty() {
        return Err(format!(
            "YSNCD cue '{}' cannot author both legacy layers and sound_graph",
            cue.name
        ));
    }
    if cue.descriptor.sound_graph.is_some() && cue.descriptor.repeat_avoidance != 0 {
        return Err(format!(
            "YSNCD cue '{}' cannot combine legacy repeat_avoidance with sound_graph; author Random/Sequence explicitly",
            cue.name
        ));
    }
    if let Some(graph) = cue.descriptor.sound_graph.as_ref() {
        graph.validate(cue.clips.iter().map(|clip| clip.name.as_str()))?;
    }
    Ok(())
}

pub fn encode_ysncd_binary_body(dictionary: &YsncdDictionary) -> Result<Vec<u8>, String> {
    use std::collections::BTreeSet;

    let cue_count = u32::try_from(dictionary.cues.len())
        .map_err(|_| "YSNCD cue count exceeds u32".to_owned())?;
    let clip_total = dictionary
        .cues
        .iter()
        .try_fold(0usize, |total, cue| total.checked_add(cue.clips.len()))
        .ok_or("YSNCD clip count overflow")?;
    let clip_count =
        u32::try_from(clip_total).map_err(|_| "YSNCD clip count exceeds u32".to_owned())?;

    #[derive(Clone)]
    struct CueRow {
        stable_hash: u64,
        name_offset: u32,
        descriptor_offset: u32,
        descriptor_len: u32,
        first_clip: u32,
        clip_count: u32,
    }
    #[derive(Clone)]
    struct ClipRow<'a> {
        stable_hash: u64,
        name_offset: u32,
        source_offset: u32,
        codec_offset: u32,
        weight: f32,
        gain: f32,
        pitch: f32,
        clip: &'a YsncdClip,
    }

    let mut strings = Vec::<u8>::new();
    let mut cue_rows = Vec::<CueRow>::with_capacity(dictionary.cues.len());
    let mut clip_rows = Vec::<ClipRow<'_>>::with_capacity(clip_total);
    let mut cue_names = BTreeSet::<String>::new();
    let mut first_clip = 0u32;

    for cue in &dictionary.cues {
        let cue_name = cue.name.trim();
        if cue_name.is_empty() {
            return Err("YSNCD cue name must not be empty".to_owned());
        }
        if !cue_names.insert(cue_name.to_ascii_lowercase()) {
            return Err(format!("duplicate YSNCD cue name '{cue_name}'"));
        }
        validate_cue_sound_graph(cue)?;
        let descriptor = serde_json::to_string(&cue.descriptor)
            .map_err(|error| format!("YSNCD cue '{cue_name}' descriptor encode failed: {error}"))?;
        let descriptor_len = u32::try_from(descriptor.len())
            .map_err(|_| format!("YSNCD cue '{cue_name}' descriptor exceeds u32"))?;
        let name_offset = push_cstr(&mut strings, cue_name)?;
        let descriptor_offset = push_cstr(&mut strings, &descriptor)?;
        let this_clip_count = u32::try_from(cue.clips.len())
            .map_err(|_| format!("YSNCD cue '{cue_name}' clip count exceeds u32"))?;
        if this_clip_count == 0 {
            return Err(format!("YSNCD cue '{cue_name}' has no clips"));
        }
        let stable_hash = if cue.stable_hash == 0 {
            crate::stable_hash_from_text(cue_name)
        } else {
            cue.stable_hash
        };
        cue_rows.push(CueRow {
            stable_hash,
            name_offset,
            descriptor_offset,
            descriptor_len,
            first_clip,
            clip_count: this_clip_count,
        });

        let mut clip_names = BTreeSet::<String>::new();
        for clip in &cue.clips {
            let clip_name = clip.name.trim();
            if clip_name.is_empty() {
                return Err(format!(
                    "YSNCD cue '{cue_name}' contains an empty clip name"
                ));
            }
            if !clip_names.insert(clip_name.to_ascii_lowercase()) {
                return Err(format!(
                    "YSNCD cue '{cue_name}' has duplicate clip '{clip_name}'"
                ));
            }
            if clip.bytes.is_empty() {
                return Err(format!(
                    "YSNCD cue '{cue_name}' clip '{clip_name}' has empty payload"
                ));
            }
            if !clip.weight.is_finite() || clip.weight <= 0.0 {
                return Err(format!(
                    "YSNCD cue '{cue_name}' clip '{clip_name}' has invalid weight"
                ));
            }
            if !clip.gain.is_finite() || !clip.pitch.is_finite() {
                return Err(format!(
                    "YSNCD cue '{cue_name}' clip '{clip_name}' has non-finite gain/pitch"
                ));
            }
            let source = clip.source.trim();
            let codec = clip.codec.trim();
            if source.is_empty() || codec.is_empty() {
                return Err(format!(
                    "YSNCD cue '{cue_name}' clip '{clip_name}' requires source and codec"
                ));
            }
            clip_rows.push(ClipRow {
                stable_hash: crate::stable_hash_from_text(clip_name),
                name_offset: push_cstr(&mut strings, clip_name)?,
                source_offset: push_cstr(&mut strings, source)?,
                codec_offset: push_cstr(&mut strings, codec)?,
                weight: clip.weight,
                gain: clip.gain,
                pitch: clip.pitch,
                clip,
            });
        }
        first_clip = first_clip
            .checked_add(this_clip_count)
            .ok_or("YSNCD first clip index overflow")?;
    }

    let cue_table_offset = HEADER_LEN;
    let clip_table_offset = cue_table_offset
        .checked_add(
            cue_rows
                .len()
                .checked_mul(CUE_RECORD_LEN)
                .ok_or("YSNCD cue table overflow")?,
        )
        .ok_or("YSNCD clip table offset overflow")?;
    let string_table_offset = clip_table_offset
        .checked_add(
            clip_rows
                .len()
                .checked_mul(CLIP_RECORD_LEN)
                .ok_or("YSNCD clip table overflow")?,
        )
        .ok_or("YSNCD string table offset overflow")?;
    let payload_offset = string_table_offset
        .checked_add(strings.len())
        .ok_or("YSNCD payload offset overflow")?;
    let payload_len = clip_rows.iter().try_fold(0usize, |total, row| {
        total
            .checked_add(row.clip.bytes.len())
            .ok_or("YSNCD payload length overflow")
    })?;
    let total_len = payload_offset
        .checked_add(payload_len)
        .ok_or("YSNCD body length overflow")?;
    let mut out = vec![0u8; total_len];

    out[0..4].copy_from_slice(&YSNCD_BINARY_MAGIC);
    write_u16(&mut out, 4, YSNCD_BINARY_SCHEMA_VERSION)?;
    write_u32(&mut out, 8, cue_count)?;
    write_u32(&mut out, 12, clip_count)?;
    write_u64(&mut out, 16, cue_table_offset as u64)?;
    write_u64(&mut out, 24, clip_table_offset as u64)?;
    write_u64(&mut out, 32, string_table_offset as u64)?;
    write_u64(&mut out, 40, strings.len() as u64)?;
    write_u64(&mut out, 48, payload_offset as u64)?;
    write_u64(&mut out, 56, payload_len as u64)?;

    for (index, row) in cue_rows.iter().enumerate() {
        let at = cue_table_offset + index * CUE_RECORD_LEN;
        write_u64(&mut out, at, row.stable_hash)?;
        write_u32(&mut out, at + 8, row.name_offset)?;
        write_u32(&mut out, at + 12, row.descriptor_offset)?;
        write_u32(&mut out, at + 16, row.descriptor_len)?;
        write_u32(&mut out, at + 20, row.first_clip)?;
        write_u32(&mut out, at + 24, row.clip_count)?;
    }
    out[string_table_offset..payload_offset].copy_from_slice(&strings);

    let mut payload_cursor = payload_offset;
    for (index, row) in clip_rows.iter().enumerate() {
        let at = clip_table_offset + index * CLIP_RECORD_LEN;
        let payload = &row.clip.bytes;
        let payload_hash = *blake3::hash(payload).as_bytes();
        write_u64(&mut out, at, row.stable_hash)?;
        write_u32(&mut out, at + 8, row.name_offset)?;
        write_u32(&mut out, at + 12, row.source_offset)?;
        write_u32(&mut out, at + 16, row.codec_offset)?;
        write_f32(&mut out, at + 24, row.weight)?;
        write_f32(&mut out, at + 28, row.gain)?;
        write_f32(&mut out, at + 32, row.pitch)?;
        write_u64(&mut out, at + 40, payload_cursor as u64)?;
        write_u64(&mut out, at + 48, payload.len() as u64)?;
        out[at + 56..at + 88].copy_from_slice(&payload_hash);
        let end = payload_cursor
            .checked_add(payload.len())
            .ok_or("YSNCD payload cursor overflow")?;
        out[payload_cursor..end].copy_from_slice(payload);
        payload_cursor = end;
    }
    debug_assert_eq!(payload_cursor, out.len());
    Ok(out)
}

fn push_cstr(strings: &mut Vec<u8>, value: &str) -> Result<u32, String> {
    if value.as_bytes().contains(&0) {
        return Err("YSNCD strings may not contain NUL".to_owned());
    }
    let offset =
        u32::try_from(strings.len()).map_err(|_| "YSNCD string table exceeds u32".to_owned())?;
    strings.extend_from_slice(value.as_bytes());
    strings.push(0);
    Ok(offset)
}

pub fn decode_ysncd_binary_body(body: &[u8]) -> Result<YsncdDictionary, String> {
    if body.len() < HEADER_LEN || body.get(0..4) != Some(&YSNCD_BINARY_MAGIC) {
        return Err("YSNCD body magic mismatch or truncated header".to_owned());
    }
    let schema = read_u16(body, 4)?;
    if schema != YSNCD_BINARY_SCHEMA_VERSION {
        return Err(format!("unsupported YSNCD body schema {schema}"));
    }
    let cue_count = read_u32(body, 8)? as usize;
    let clip_count = read_u32(body, 12)? as usize;
    let cue_table_offset = to_usize(read_u64(body, 16)?, "cue table offset")?;
    let clip_table_offset = to_usize(read_u64(body, 24)?, "clip table offset")?;
    let string_table_offset = to_usize(read_u64(body, 32)?, "string table offset")?;
    let string_table_len = to_usize(read_u64(body, 40)?, "string table len")?;
    let payload_offset = to_usize(read_u64(body, 48)?, "payload offset")?;
    let payload_len = to_usize(read_u64(body, 56)?, "payload len")?;

    checked_slice(
        body,
        cue_table_offset,
        cue_count
            .checked_mul(CUE_RECORD_LEN)
            .ok_or("cue table overflow")?,
        "cue table",
    )?;
    checked_slice(
        body,
        clip_table_offset,
        clip_count
            .checked_mul(CLIP_RECORD_LEN)
            .ok_or("clip table overflow")?,
        "clip table",
    )?;
    let strings = checked_slice(body, string_table_offset, string_table_len, "string table")?;
    checked_slice(body, payload_offset, payload_len, "payload region")?;

    #[derive(Clone)]
    struct ClipRow {
        name: String,
        source: String,
        codec: String,
        weight: f32,
        gain: f32,
        pitch: f32,
        bytes: Vec<u8>,
        hash: [u8; 32],
    }

    let mut clip_rows = Vec::with_capacity(clip_count);
    for index in 0..clip_count {
        let at = clip_table_offset + index * CLIP_RECORD_LEN;
        let name = read_cstr(strings, read_u32(body, at + 8)? as usize)?;
        let source = read_cstr(strings, read_u32(body, at + 12)? as usize)?;
        let codec = read_cstr(strings, read_u32(body, at + 16)? as usize)?;
        let payload_at = to_usize(read_u64(body, at + 40)?, "clip payload offset")?;
        let payload_len = to_usize(read_u64(body, at + 48)?, "clip payload len")?;
        let bytes = checked_slice(body, payload_at, payload_len, "clip payload")?.to_vec();
        let mut expected_hash = [0u8; 32];
        expected_hash.copy_from_slice(checked_slice(body, at + 56, 32, "clip hash")?);
        let actual_hash = *blake3::hash(&bytes).as_bytes();
        if actual_hash != expected_hash {
            return Err(format!("YSNCD clip '{name}' BLAKE3 mismatch"));
        }
        clip_rows.push(ClipRow {
            name,
            source,
            codec,
            weight: read_f32(body, at + 24)?,
            gain: read_f32(body, at + 28)?,
            pitch: read_f32(body, at + 32)?,
            bytes,
            hash: expected_hash,
        });
    }

    let mut cues = Vec::with_capacity(cue_count);
    for index in 0..cue_count {
        let at = cue_table_offset + index * CUE_RECORD_LEN;
        let stable_hash = read_u64(body, at)?;
        let name = read_cstr(strings, read_u32(body, at + 8)? as usize)?;
        let descriptor = read_cstr(strings, read_u32(body, at + 12)? as usize)?;
        let descriptor_len = read_u32(body, at + 16)? as usize;
        if descriptor.len() != descriptor_len {
            return Err(format!("cue '{name}' descriptor length mismatch"));
        }
        let descriptor: YsncdCueDescriptor = serde_json::from_str(&descriptor)
            .map_err(|error| format!("cue '{name}' descriptor JSON invalid: {error}"))?;
        let first = read_u32(body, at + 20)? as usize;
        let count = read_u32(body, at + 24)? as usize;
        let end = first.checked_add(count).ok_or("cue clip range overflow")?;
        if end > clip_rows.len() {
            return Err(format!("cue '{name}' clip range out of bounds"));
        }
        let clips = clip_rows[first..end]
            .iter()
            .map(|row| YsncdClip {
                name: row.name.clone(),
                source: row.source.clone(),
                codec: row.codec.clone(),
                weight: row.weight,
                gain: row.gain,
                pitch: row.pitch,
                bytes: row.bytes.clone(),
                payload_hash: row.hash,
            })
            .collect();
        let cue = YsncdCue {
            name,
            stable_hash,
            descriptor,
            clips,
        };
        validate_cue_sound_graph(&cue)?;
        cues.push(cue);
    }
    Ok(YsncdDictionary { cues })
}

fn write_u16(bytes: &mut [u8], at: usize, value: u16) -> Result<(), String> {
    checked_slice_mut(bytes, at, 2, "u16")?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u32(bytes: &mut [u8], at: usize, value: u32) -> Result<(), String> {
    checked_slice_mut(bytes, at, 4, "u32")?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u64(bytes: &mut [u8], at: usize, value: u64) -> Result<(), String> {
    checked_slice_mut(bytes, at, 8, "u64")?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_f32(bytes: &mut [u8], at: usize, value: f32) -> Result<(), String> {
    checked_slice_mut(bytes, at, 4, "f32")?.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn checked_slice_mut<'a>(
    bytes: &'a mut [u8],
    offset: usize,
    len: usize,
    label: &str,
) -> Result<&'a mut [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("{label} range overflow"))?;
    let bytes_len = bytes.len();
    bytes
        .get_mut(offset..end)
        .ok_or_else(|| format!("{label} truncated offset={offset} len={len} bytes={bytes_len}"))
}

fn checked_slice<'a>(
    bytes: &'a [u8],
    offset: usize,
    len: usize,
    label: &str,
) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("{label} range overflow"))?;
    bytes.get(offset..end).ok_or_else(|| {
        format!(
            "{label} truncated offset={offset} len={len} bytes={}",
            bytes.len()
        )
    })
}

fn read_cstr(strings: &[u8], offset: usize) -> Result<String, String> {
    let tail = strings
        .get(offset..)
        .ok_or("YSNCD string offset out of bounds")?;
    let len = tail
        .iter()
        .position(|byte| *byte == 0)
        .ok_or("YSNCD string is not NUL terminated")?;
    String::from_utf8(tail[..len].to_vec())
        .map_err(|error| format!("YSNCD string is not UTF-8: {error}"))
}

fn to_usize(value: u64, label: &str) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| format!("{label} does not fit usize: {value}"))
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    Ok(u16::from_le_bytes(
        checked_slice(bytes, at, 2, "u16")?
            .try_into()
            .map_err(|_| "invalid u16 slice")?,
    ))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        checked_slice(bytes, at, 4, "u32")?
            .try_into()
            .map_err(|_| "invalid u32 slice")?,
    ))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        checked_slice(bytes, at, 8, "u64")?
            .try_into()
            .map_err(|_| "invalid u64 slice")?,
    ))
}

fn read_f32(bytes: &[u8], at: usize) -> Result<f32, String> {
    Ok(f32::from_le_bytes(
        checked_slice(bytes, at, 4, "f32")?
            .try_into()
            .map_err(|_| "invalid f32 slice")?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_descriptor_without_sound_graph_defaults_to_none() {
        let descriptor: YsncdCueDescriptor =
            serde_json::from_str(r#"{"bus":"sfx"}"#).expect("legacy descriptor");
        assert!(descriptor.sound_graph.is_none());
    }

    #[test]
    fn rejects_non_ysncd_body() {
        assert!(decode_ysncd_binary_body(b"NOPE").is_err());
    }

    #[test]
    fn ysncd_binary_roundtrips_embedded_clip() {
        let dictionary = YsncdDictionary {
            cues: vec![YsncdCue {
                name: "dirt_run".to_owned(),
                stable_hash: crate::stable_hash_from_text("dirt_run"),
                descriptor: YsncdCueDescriptor {
                    route: "project.world.foley".to_owned(),
                    concurrency_group: "project.footsteps".to_owned(),
                    concurrency_limit: 4,
                    concurrency_scope: "object".to_owned(),
                    steal_rule: "quietest".to_owned(),
                    voice_budget: "project.foley".to_owned(),
                    priority: 23,
                    spatial_policy: "spatial".to_owned(),
                    gain_range: [0.95, 1.05],
                    pitch_range: [0.97, 1.03],
                    sound_graph: Some(crate::ysncd_sound_graph::YsncdSoundGraph {
                        root: "root".to_owned(),
                        nodes: vec![crate::ysncd_sound_graph::YsncdSoundGraphNode::Clip {
                            id: "root".to_owned(),
                            clip: "dirt_run_01".to_owned(),
                            gain: 0.8,
                            pitch: 1.1,
                        }],
                    }),
                    ..Default::default()
                },
                clips: vec![YsncdClip {
                    name: "dirt_run_01".to_owned(),
                    source: "dirt/run_01.wav".to_owned(),
                    codec: "wav".to_owned(),
                    weight: 1.0,
                    gain: 1.0,
                    pitch: 1.0,
                    bytes: b"RIFF-test-payload".to_vec(),
                    payload_hash: [0; 32],
                }],
            }],
        };
        let encoded = encode_ysncd_binary_body(&dictionary).expect("encode YSNCD");
        let decoded = decode_ysncd_binary_body(&encoded).expect("decode encoded YSNCD");
        let cue = decoded.cue("dirt_run").expect("cue");
        assert_eq!(cue.descriptor.route, "project.world.foley");
        assert_eq!(cue.descriptor.concurrency_group, "project.footsteps");
        assert_eq!(cue.descriptor.concurrency_limit, 4);
        assert_eq!(cue.descriptor.concurrency_scope, "object");
        assert_eq!(cue.descriptor.steal_rule, "quietest");
        assert_eq!(cue.descriptor.voice_budget, "project.foley");
        assert_eq!(cue.descriptor.priority, 23);
        let graph = cue
            .descriptor
            .sound_graph
            .as_ref()
            .expect("SoundGraph roundtrip");
        assert_eq!(graph.root, "root");
        assert_eq!(graph.nodes.len(), 1);
        assert_eq!(cue.descriptor.spatial_policy, "spatial");
        assert_eq!(cue.clips.len(), 1);
        assert_eq!(cue.clips[0].source, "dirt/run_01.wav");
        assert_eq!(cue.clips[0].bytes, b"RIFF-test-payload");
        assert_eq!(
            cue.clips[0].payload_hash,
            *blake3::hash(b"RIFF-test-payload").as_bytes()
        );
    }
}
