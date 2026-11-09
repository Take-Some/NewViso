//! A planner tick reads each resident frame once, including missing frames.
use super::{BodyPose, ground};

pub(super) struct PoseObservation<F> {
    sample: F,
    frames: Vec<(u64, Option<BodyPose>)>,
}

impl<F: FnMut(u64) -> Option<BodyPose>> PoseObservation<F> {
    pub(super) fn new(sample: F, mut frames: Vec<(u64, Option<BodyPose>)>) -> Self {
        // Retain capacity across ticks; observations themselves never survive.
        frames.clear();
        Self { sample, frames }
    }

    pub(super) fn pose(&mut self, entity: u64) -> Option<BodyPose> {
        if let Some((_, pose)) = self.frames.iter().find(|(id, _)| *id == entity) {
            return *pose;
        }
        let pose = (self.sample)(entity).filter(|p| ground::valid_frame(*p));
        self.frames.push((entity, pose));
        pose
    }

    pub(super) fn into_frames(self) -> Vec<(u64, Option<BodyPose>)> {
        self.frames
    }
}
