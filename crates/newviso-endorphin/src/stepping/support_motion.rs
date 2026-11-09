//! Transport support intents in the frame of the resident supporting body.
use super::*;
impl SteppingState {
    /// Contact multiplicity is geometry, not measured load. Give each observed
    /// supporting foot one vote; pending/airborne targets supply no floor motion.
    pub(super) fn support_velocity(
        &self,
        contacts: &[SupportContact],
        exclude: Option<usize>,
    ) -> [f32; 3] {
        let mut velocity = [0.0; 3];
        let mut count = 0;
        for (foot, ground) in &self.planted {
            if Some(*foot) != exclude && contacts.iter().any(|c| c.part == *foot) {
                velocity = math::add(velocity, ground.velocity);
                count += 1;
            }
        }
        if count == 0 {
            velocity
        } else {
            velocity.map(|v| v / count as f32)
        }
    }

    pub(super) fn advance_supports(
        &mut self,
        dt: f32,
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) {
        self.planted
            .retain(|_, target| target.refresh(dt, &mut pose));
        self.sole_frames
            .retain(|foot, _| self.planted.contains_key(foot));
        if let Some((entity, local)) = self.support_goal_frame.take() {
            self.support_goal = pose(entity)
                .filter(|p| ground::valid_frame(*p))
                .map(|p| math::add(p.position, math::rotate(math::normalize(p.rotation), local)));
        } else if let Some(goal) = &mut self.support_goal {
            if !self.planted.is_empty() && self.planted.values().all(|g| g.support.is_none()) {
                for i in 0..3 {
                    goal[i] += self.planted.values().map(|g| g.velocity[i]).sum::<f32>() * dt
                        / self.planted.len() as f32;
                }
            }
        }
        if self
            .swing
            .as_mut()
            .is_some_and(|s| !s.landing.refresh(dt, &mut pose))
        {
            self.swing = None;
            self.support_goal = None;
            self.rejected += 1;
        }
    }
    pub(super) fn follow_sole_frame(
        &mut self,
        foot: usize,
        mut pose: impl FnMut(u64) -> Option<BodyPose>,
    ) {
        if let Some(anchor) = self.planted.get(&foot).and_then(|g| g.support.as_ref()) {
            if let Some(frame) = pose(anchor.entity).filter(|p| ground::valid_frame(*p)) {
                let rotation = math::normalize(frame.rotation);
                let previous = self.sole_references[&foot];
                let entry = self.sole_frames.entry(foot).or_insert_with(|| {
                    (
                        anchor.entity,
                        math::multiply(math::inverse(rotation), previous),
                    )
                });
                if entry.0 != anchor.entity {
                    *entry = (
                        anchor.entity,
                        math::multiply(math::inverse(rotation), previous),
                    );
                }
                self.sole_references
                    .insert(foot, math::multiply(rotation, entry.1));
            }
        } else {
            self.sole_frames.remove(&foot);
        }
    }
    pub(super) fn bind_support_goal(&mut self, mut pose: impl FnMut(u64) -> Option<BodyPose>) {
        // Transport the balance intent with a common supporting body, including
        // rotation. Different supports keep separate foot anchors.
        let supports: Vec<_> = self
            .planted
            .iter()
            .filter(|(foot, _)| {
                !self
                    .swing
                    .as_ref()
                    .is_some_and(|s| s.released && s.foot == **foot)
            })
            .map(|(_, g)| g.support.as_ref().map(|a| a.entity))
            .collect();
        self.support_goal_frame = supports
            .first()
            .copied()
            .flatten()
            .filter(|entity| supports.iter().all(|s| *s == Some(*entity)))
            .zip(self.support_goal)
            .and_then(|(entity, goal)| {
                pose(entity).filter(|p| ground::valid_frame(*p)).map(|p| {
                    (
                        entity,
                        math::rotate(
                            math::inverse(math::normalize(p.rotation)),
                            math::sub(goal, p.position),
                        ),
                    )
                })
            });
    }
}
