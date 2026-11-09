//! Endorphin: provider-independent articulated bodies and physical recovery.
//! Coordinates are metres, seconds, kilograms, Y-up; quaternions are xyzw.
//! The host supplies resident poses/contacts. This crate never queries the world,
//! owns gameplay health, loads assets, or calls a renderer/physics provider.
mod balance;
pub use balance::*;
mod anatomy;
pub use anatomy::*;
mod clock;
pub mod math;
mod model;
pub use clock::ObservationClock;
mod reactions;
mod recovery;
mod recovery_match;
mod recovery_placement;
mod recovery_profile;
mod recovery_support;
pub use recovery_match::{
    match_recovery_pose, match_recovery_samples, match_weighted_recovery_samples,
    recovery_sample_times, RecoveryMarker, RecoveryPoseMatch, WeightedRecoveryMarker,
};
pub use recovery_placement::{
    find_recovery_placement, RecoveryPlacement, RecoveryPlacementInput, RecoveryPlacementReport,
};
pub use recovery_profile::{RecoveryMatchClip, RecoveryMatchJoint, RecoveryMatchProfile};
pub use recovery_support::{RecoverySupport, RecoverySupportSample};
mod rig;
pub use model::*;
pub use reactions::*;
pub use recovery::*;
pub use rig::*;

#[cfg(test)]
mod tests;

mod support;
pub use support::{AngularDriveBody, SupportContact, SupportPlan, WorldAngularDrive};

mod plant;
pub use plant::{FootPlant, FootPlantState};

mod stepping;
pub use stepping::{
    solve_two_link, StepGround, StepLeg, SteppingModel, SteppingState, SupportAnchor,
};

mod authority;
pub use authority::{AuthorityObservation, BodyOwner, ControlAuthority};

mod binding;
pub use binding::{ArmBinding, GripPose, JointRotation, WeaponRig};

mod protection;
pub use protection::*;
