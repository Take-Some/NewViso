//! Engine-owned character contact audio. Foot contacts follow animation phase
//! when the clip is resident; distance-based cadence is the streaming fallback.
//! One tracker per character avoids cross-actor footstep/landing interference.

use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct CharacterMotionSample {
    pub actor_id: String,
    pub scene_id: String,
    pub position: [f32; 3],
    pub grounded: bool,
    pub jumped: bool,
    pub landed: bool,
    pub speed: f32,
    pub gait: Gait,
    pub surface: String,
    pub dictionary: String,
    pub delta_seconds: f32,
    pub impact_speed: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Gait { Idle, Walk, Run, Sprint }

impl Gait {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "idle" => Some(Self::Idle),
            "walk" => Some(Self::Walk),
            "run" => Some(Self::Run),
            "sprint" => Some(Self::Sprint),
            _ => None,
        }
    }
    pub fn action(self) -> &'static str {
        match self {
            Self::Idle => "walk",
            Self::Walk => "walk",
            Self::Run => "run",
            Self::Sprint => "sprint",
        }
    }
    /// Physical advance per footfall, rather than a time-based footstep timer.
    /// Both the locomotion clip cadence and its sound share this stride.
    pub fn stride(self) -> f32 {
        match self {
            Self::Idle => f32::INFINITY,
            Self::Walk => 0.70,
            Self::Run => 0.85,
            Self::Sprint => 1.0,
        }
    }

    /// Animation playback is adjusted only within a natural-looking range:
    /// never use the previous 0.2..4.0 rate that overcranked Abby's movement.
    pub fn playback_rate(self, speed: f32, duration: f32) -> Option<f32> {
        if self == Self::Idle || !speed.is_finite() || speed < 0.12
            || !duration.is_finite() || duration <= 0.0 {
            return None;
        }
        // Imported .ycd locomotion clips may contain MANY alternating
        // footsteps per loop. Never assume exactly two steps per clip.
        // Keep the authored reference animation at rate=1 and scale only
        // with physical speed within a safe natural playback range.
        let reference_speed = match self {
            Self::Idle => return None,
            Self::Walk => 1.5,
            Self::Run => 3.0,
            Self::Sprint => 4.6,
        };
        Some((speed / reference_speed).clamp(0.70, 1.30))
    }
}

#[derive(Clone, Debug)]
pub(super) struct ClipPhase {
    pub name: String,
    pub time: f32,
    pub duration: f32,
    /// Render-frame world-space positions from the actual animated foot bones.
    pub feet: Option<[[f32; 3]; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SoundKind { Footstep, Lift, Landing }

#[derive(Clone, Debug)]
pub(super) struct ContactSound {
    pub kind: SoundKind,
    pub cue: String,
    pub position: [f32; 3],
    pub seed: u64,
    pub gain: f32,
    pub source: &'static str,
}

#[derive(Default)]
struct ActorContacts {
    position: Option<[f32; 3]>,
    clip: Option<String>,
    clip_time: f32,
    distance_remainder: f32,
    step_index: u64,
    airborne: bool,
    grounded_seen: bool,
    /// Prevent spawn/streaming contact from being heard as a landing.
    landing_armed: bool,
    airborne_seconds: f32,
    foot_raised: [bool; 2],
    foot_samples_active: bool,
    clock_seconds: f32,
    last_contact_seconds: [Option<f32>; 2],

}

#[derive(Default)]
pub(super) struct CharacterAudioRuntime {
    actors: BTreeMap<String, ActorContacts>,
}

fn cue(dictionary: &str, surface: &str, action: &str) -> String {
    format!("{dictionary}@{surface}_{action}")
}

fn sound(
    sample: &CharacterMotionSample,
    kind: SoundKind,
    action: &str,
    seed: u64,
    gain: f32,
) -> ContactSound {
    ContactSound {
        kind,
        cue: cue(&sample.dictionary, &sample.surface, action),
        position: sample.position,
        seed,
        gain,
        source: "physics",
    }
}

pub(super) fn clip_has_gait(clip: &str, gait: Gait) -> bool {
    match gait {
        Gait::Idle => false,
        Gait::Walk => clip.contains("walk"),
        Gait::Run => clip.contains("run") && !clip.contains("sprint"),
        Gait::Sprint => clip.contains("sprint") || clip.contains("run"),
    }
}

/// Emit only on a *raised -> planted* transition, for both feet independently.
/// Comparing the two ankle heights cancels root/pelvis bob and floor elevation.
fn planted_feet(raised: &mut [bool; 2], feet: &[[f32; 3]; 2]) -> Vec<usize> {
    const LIFT_HEIGHT: f32 = 0.045;
    const PLANT_HEIGHT: f32 = 0.018;
    let bottom = feet[0][1].min(feet[1][1]);
    let relative = [feet[0][1] - bottom, feet[1][1] - bottom];
    let mut planted = Vec::with_capacity(1);
    for i in 0..2 {
        if relative[i] >= LIFT_HEIGHT {
            raised[i] = true;
        } else if relative[i] <= PLANT_HEIGHT && raised[i] {
            raised[i] = false;
            planted.push(i);
        }
    }
    planted
}

impl CharacterAudioRuntime {
    pub(super) fn remove(&mut self, actor_id: &str) { self.actors.remove(actor_id); }

    pub(super) fn sample(
        &mut self,
        sample: &CharacterMotionSample,
        playback: Option<&ClipPhase>,
    ) -> Vec<ContactSound> {
        let actor = self.actors.entry(sample.actor_id.clone()).or_default();
        let mut result = Vec::with_capacity(3);
        let valid_motion = sample.delta_seconds.is_finite()
            && (0.0..=0.25).contains(&sample.delta_seconds)
            && sample.speed.is_finite() && sample.speed >= 0.0;
        let previous_position = actor.position.replace(sample.position);
        let displacement = previous_position.map(|previous| {
            (sample.position[0] - previous[0])
                .hypot(sample.position[2] - previous[2])
        });
        let real_travel = displacement.filter(|distance| distance.is_finite() && *distance <= 2.25)
            .unwrap_or(0.0);
        let stationary = sample.gait == Gait::Idle || sample.speed < 0.12;

        // A falling/unresolved physics body during scene startup is not a
        // character action. Only a real jump, or departure from an established
        // ground contact, can arm landing audio.
        if sample.jumped {
            if actor.grounded_seen {
                let lift_action = match sample.surface.as_str() {
                    "dirt" | "metal" | "stone" | "water" | "wood" => "lift",
                    _ => "walk",
                };
                result.push(sound(sample, SoundKind::Lift, lift_action, actor.step_index, 1.0));
                actor.landing_armed = true;
            }
            actor.airborne = true;
            actor.airborne_seconds = 0.0;
            actor.distance_remainder = 0.0;
            actor.clip = None;
            actor.foot_raised = [false; 2];
            actor.foot_samples_active = false;
        }

        if !sample.grounded {
            // A single lost contact during a physics substep is not a jump or
            // a fall. Require sustained airborne movement before arming land.
            if actor.grounded_seen && !sample.jumped && valid_motion {
                actor.airborne_seconds += sample.delta_seconds;
                if actor.airborne_seconds >= 0.08 {
                    actor.landing_armed = true;
                }
            }
            actor.airborne = true;
            actor.clip = None;
            actor.distance_remainder = 0.0;
            actor.foot_raised = [false; 2];
            actor.foot_samples_active = false;
            return result;
        }

        if sample.landed && actor.airborne && actor.landing_armed {
            result.push(sound(sample, SoundKind::Landing, "land", actor.step_index,
                (0.72 + sample.impact_speed.abs() * 0.08).clamp(0.65, 1.35)));
        }
        if sample.landed {
            actor.airborne = false;
            actor.airborne_seconds = 0.0;
            actor.landing_armed = false;
            actor.distance_remainder = 0.0;
            actor.clip = None;
            actor.foot_raised = [false; 2];
            actor.foot_samples_active = false;
        }

        actor.grounded_seen = true;
        actor.airborne_seconds = 0.0;
        if stationary || !valid_motion || sample.jumped || sample.landed {
            if stationary || !valid_motion {
                actor.clip = None;
                actor.distance_remainder = 0.0;
                actor.foot_raised = [false; 2];
                actor.foot_samples_active = false;
            }
            return result;
        }

        actor.clock_seconds += sample.delta_seconds;
        // Track actual metres, not normalized clip phase: long imported clips
        // often include many footfalls before they loop.
        actor.distance_remainder += real_travel;
        let stride = sample.gait.stride();
        let min_contact_interval = match sample.gait {
            Gait::Idle => f32::INFINITY,
            Gait::Walk => 0.20,
            Gait::Run => 0.12,
            Gait::Sprint => 0.10,
        };

        let compatible = playback.filter(|p| p.duration.is_finite()
            && p.duration > 0.0 && p.time.is_finite()
            && clip_has_gait(&p.name.to_ascii_lowercase(), sample.gait));

        if let Some(clip) = compatible {
            if let Some(feet) = &clip.feet {
                let changed_clip = actor.clip.as_deref() != Some(clip.name.as_str());
                let planted = if changed_clip || !actor.foot_samples_active {
                    actor.foot_raised = [false; 2];
                    actor.foot_samples_active = true;
                    let _ = planted_feet(&mut actor.foot_raised, feet);
                    Vec::new() // Never play an initial frame's grounded foot.
                } else {
                    planted_feet(&mut actor.foot_raised, feet)
                };
                for foot in planted {
                    if actor.last_contact_seconds[foot].is_some_and(|last|
                        actor.clock_seconds - last < min_contact_interval) {
                        continue;
                    }
                    let mut hit = sound(sample, SoundKind::Footstep,
                        sample.gait.action(), actor.step_index, 1.0);
                    hit.position = feet[foot];
                    hit.source = "foot_joint";
                    result.push(hit);
                    actor.step_index = actor.step_index.wrapping_add(1);
                    actor.last_contact_seconds[foot] = Some(actor.clock_seconds);
                    // Actual skeletal plant always wins over distance fallback.
                    actor.distance_remainder = 0.0;
                }
                actor.clip = Some(clip.name.clone());
                actor.clip_time = (clip.time / clip.duration).rem_euclid(1.0);
                // The displayed rig owns every audible contact. A
                // distance-based recovery here made sounds land BETWEEN
                // visible foot plants, even when frequency was correct.
                actor.distance_remainder = 0.0;
                return result;
            } else {
                actor.foot_samples_active = false;
                actor.clip = None;
            }
        } else {
            actor.foot_samples_active = false;
            actor.clip = None;
        }

        // This fallback applies only to models without presentable foot
        // bones (or before first skinning result), never while an animated
        // skeleton is providing visible contacts.
        let fallback_threshold = stride;
        let mut emitted = 0;
        while actor.distance_remainder >= fallback_threshold && emitted < 4 {
            actor.distance_remainder -= fallback_threshold;
            let foot = (actor.step_index % 2) as usize;
            if actor.last_contact_seconds[foot].is_some_and(|last|
                actor.clock_seconds - last < min_contact_interval) {
                continue;
            }
            let mut hit = sound(sample, SoundKind::Footstep,
                sample.gait.action(), actor.step_index, 1.0);
            hit.source = "travel_distance";
            result.push(hit);
            actor.step_index = actor.step_index.wrapping_add(1);
            actor.last_contact_seconds[foot] = Some(actor.clock_seconds);
            emitted += 1;
        }
        if actor.distance_remainder >= fallback_threshold {
            actor.distance_remainder %= fallback_threshold;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(speed: f32, grounded: bool, gait: Gait, x: f32) -> CharacterMotionSample {
        CharacterMotionSample {
            actor_id: "player".into(), scene_id: "player.character".into(),
            position: [x, 0.0, 0.0], grounded,
            jumped: false, landed: false, speed, gait,
            surface: "stone".into(), dictionary: "audio/footsteps/footsteps.yscd".into(),
            delta_seconds: 1.0 / 60.0, impact_speed: 0.0,
        }
    }

    #[test]
    fn physical_walk_and_run_cadence_is_distance_based() {
        let count = |gait, speed| {
            let mut runtime = CharacterAudioRuntime::default();
            (0..600).map(|frame| {
                runtime.sample(&sample(speed, true, gait, frame as f32 * speed / 60.0), None).len()
            }).sum::<usize>()
        };
        let walk = count(Gait::Walk, 1.5);
        let run = count(Gait::Run, 3.0);
        assert!((20..=23).contains(&walk), "walk={walk}");
        assert!((34..=37).contains(&run), "run={run}");
        assert!(run > walk);
    }

    #[test]
    fn long_clips_are_never_assumed_to_contain_only_two_footfalls() {
        let mut runtime = CharacterAudioRuntime::default();
        let mut input = sample(2.8, true, Gait::Run, 0.0);
        let clip = |time, l, r| ClipPhase {
            name: "abby-mm-explore-run-loop-fw".into(),
            time, duration: 2.4,
            feet: Some([[0.0, l, 0.0], [0.25, r, 0.0]]),
        };
        // Eight independent alternating bone plants inside ONE clip cycle.
        let mut contacts = 0;
        for n in 0..17 {
            let i = n / 2;
            let raised_left = n % 4 < 2;
            let (left,right) = if raised_left {(0.10,0.0)} else {(0.0,0.10)};
            input.position[0] = n as f32 * 0.17;
            input.delta_seconds = 0.16;
            let sounds = runtime.sample(&input, Some(&clip(n as f32 * 0.12,left,right)));
            contacts += sounds.len();
            assert!(sounds.len() <= 1, "duplicate foot contact");
            let _=i;
        }
        assert!(contacts >= 6, "expected multiple strides within one long clip, got {contacts}");
    }

    #[test]
    fn gait_pacing_does_not_depend_on_clip_length() {
        assert_eq!(Gait::Walk.playback_rate(1.5,2.4), Some(1.0));
        assert_eq!(Gait::Run.playback_rate(3.0,2.4), Some(1.0));
        assert_eq!(Gait::Sprint.playback_rate(4.6,2.4), Some(1.0));
        assert!(Gait::Walk.playback_rate(1.2,2.4).unwrap()
            < Gait::Walk.playback_rate(1.8,2.4).unwrap());
        assert!(Gait::Run.playback_rate(2.5,2.4).unwrap()
            < Gait::Run.playback_rate(3.5,2.4).unwrap());
        for gait in [Gait::Walk,Gait::Run,Gait::Sprint] {
            assert!((0.70..=1.30).contains(&gait.playback_rate(15.0,100.0).unwrap()));
        }
    }

    #[test]
    fn presented_feet_never_emit_offbeat_distance_recovery() {
        let mut runtime = CharacterAudioRuntime::default();
        let mut input = sample(2.8, true, Gait::Run, 0.0);
        let clip = |time| ClipPhase {
            name: "abby-run-loop".into(), time, duration: 2.4,
            feet: Some([[0.0, 0.005, 0.0], [0.2, 0.0, 0.0]]),
        };
        // Presentable but planted, no new foot lift. The old fallback would
        // fire after every 1.3 strides at arbitrary mesh positions.
        for frame in 0..160 {
            input.position[0] = frame as f32 * 2.8 / 60.0;
            assert!(runtime.sample(&input, Some(&clip(frame as f32 / 60.0))).is_empty());
        }
    }

    #[test]
    fn first_foot_plant_after_lift_emits_once() {
        let mut runtime = CharacterAudioRuntime::default();
        let input = sample(1.5,true,Gait::Walk,0.0);
        let pose = |time,left,right| ClipPhase{
            name: "abby-walk-loop".into(), time, duration:2.5,
            feet:Some([[0.0,left,0.0],[0.2,right,0.0]])};
        assert!(runtime.sample(&input,Some(&pose(0.0,0.1,0.0))).is_empty());
        assert_eq!(runtime.sample(&input,Some(&pose(0.1,0.0,0.1))).len(),1);
        assert!(runtime.sample(&input,Some(&pose(0.12,0.0,0.1))).is_empty());
    }

    #[test]
    fn no_startup_landing_or_idle_footsteps() {
        let mut runtime=CharacterAudioRuntime::default();
        for _ in 0..3 {
            assert!(runtime.sample(&sample(0.0,false,Gait::Idle,0.0),None).is_empty());
        }
        let mut landed=sample(0.0,true,Gait::Idle,0.0);
        landed.landed=true;
        assert!(runtime.sample(&landed,None).is_empty());
        assert!(runtime.sample(&landed,None).is_empty());
    }

    #[test]
    fn jump_and_real_landing_only_once() {
        let mut runtime=CharacterAudioRuntime::default();
        runtime.sample(&sample(0.0,true,Gait::Idle,0.0),None);
        let mut air=sample(3.0,false,Gait::Run,0.0);
        air.jumped=true;
        assert_eq!(runtime.sample(&air,None).len(),1);
        air.jumped=false;
        assert!(runtime.sample(&air,None).is_empty());
        let mut landed=sample(0.0,true,Gait::Idle,0.0);
        landed.landed=true;
        assert_eq!(runtime.sample(&landed,None).len(),1);
        assert!(runtime.sample(&landed,None).is_empty());
    }

    #[test]
    fn teleport_does_not_emit_catchup_sounds() {
        let mut runtime=CharacterAudioRuntime::default();
        runtime.sample(&sample(1.5,true,Gait::Walk,0.0),None);
        assert!(runtime.sample(&sample(1.5,true,Gait::Walk,1000.0),None).is_empty());
    }

    #[test]
    fn a_lost_single_physics_contact_is_not_landing() {
        let mut runtime=CharacterAudioRuntime::default();
        runtime.sample(&sample(0.0,true,Gait::Idle,0.0),None);
        assert!(runtime.sample(&sample(0.0,false,Gait::Idle,0.0),None).is_empty());
        let mut landed=sample(0.0,true,Gait::Idle,0.0);
        landed.landed=true;
        assert!(runtime.sample(&landed,None).is_empty());
    }
}
