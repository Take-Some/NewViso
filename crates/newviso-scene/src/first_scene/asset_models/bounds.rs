//! Model and local bounds transformation.
use super::*;

pub(in crate::first_scene) fn transformed_local_bounds(
    local_bounds: SceneBounds,
    transform: SceneTransform,
) -> Result<SceneBounds, String> {
    if [
        local_bounds.min.x,
        local_bounds.min.y,
        local_bounds.min.z,
        local_bounds.max.x,
        local_bounds.max.y,
        local_bounds.max.z,
    ]
    .iter()
    .any(|value| !value.is_finite())
    {
        return Err("asset model has non-finite local bounds".to_owned());
    }

    let min = local_bounds.min;
    let max = local_bounds.max;
    let mut world_min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut world_max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

    for x in [min.x, max.x] {
        for y in [min.y, max.y] {
            for z in [min.z, max.z] {
                let p = transform_point(
                    Vec3::new(x, y, z),
                    transform.scale,
                    transform.rotation_degrees,
                    transform.position,
                );
                world_min.x = world_min.x.min(p.x);
                world_min.y = world_min.y.min(p.y);
                world_min.z = world_min.z.min(p.z);
                world_max.x = world_max.x.max(p.x);
                world_max.y = world_max.y.max(p.y);
                world_max.z = world_max.z.max(p.z);
            }
        }
    }

    Ok(SceneBounds {
        min: world_min,
        max: world_max,
    })
}

pub(in crate::first_scene) fn transformed_model_bounds(
    model: &ModelResource,
    transform: SceneTransform,
) -> Result<SceneBounds, String> {
    if !model.bounds.is_finite() {
        return Err(format!("model '{}' has non-finite bounds", model.name));
    }
    transformed_local_bounds(
        SceneBounds {
            min: Vec3::new(
                model.bounds.min[0],
                model.bounds.min[1],
                model.bounds.min[2],
            ),
            max: Vec3::new(
                model.bounds.max[0],
                model.bounds.max[1],
                model.bounds.max[2],
            ),
        },
        transform,
    )
}
