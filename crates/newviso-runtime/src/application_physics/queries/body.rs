use super::super::*;
/// Sweep in the body's oriented box frame and return a world-space normal.
pub(crate) fn sweep_body_box(
    body: &PhysicsBodySnapshot,
    origin: [f32; 3],
    delta: [f32; 3],
    radius: f32,
) -> Option<newviso_character::SweepHit> {
    let local_origin = inverse_rotate_camera_vector(
        std::array::from_fn(|i| origin[i] - body.position[i]),
        body.rotation,
    );
    let local_delta = inverse_rotate_camera_vector(delta, body.rotation);
    let (min, max) = shape_bounds(body.shape, [0.0; 3]);
    let hit =
        newviso_collision::sweep_sphere_aabb_hit(local_origin, local_delta, radius, min, max)?;
    Some(newviso_character::SweepHit {
        fraction: hit.fraction,
        normal: rotate_camera_vector(hit.normal, body.rotation),
        entity: Some(body.entity),
    })
}
