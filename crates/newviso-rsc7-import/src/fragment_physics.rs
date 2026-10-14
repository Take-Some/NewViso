use newviso_model::{ModelFragmentPartPhysics, ModelFragmentPhysicsChild};
use serde_json::Value;

fn number(v: &Value, key: &str) -> Result<f32, String> {
    v.get(key)
        .and_then(Value::as_f64)
        .map(|n| n as f32)
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("fragment physics '{key}' must be finite numeric"))
}
fn vector<const N: usize>(v: &Value, key: &str) -> Result<[f32; N], String> {
    let a = v
        .get(key)
        .and_then(Value::as_array)
        .filter(|a| a.len() == N)
        .ok_or_else(|| format!("fragment physics '{key}' must contain {N} numbers"))?;
    let mut out = [0.0; N];
    for (i, n) in a.iter().enumerate() {
        out[i] = n
            .as_f64()
            .map(|n| n as f32)
            .filter(|n| n.is_finite())
            .ok_or_else(|| format!("fragment physics '{key}[{i}]' must be finite"))?;
    }
    Ok(out)
}
pub(super) fn decode(v: &Value) -> Result<Option<ModelFragmentPartPhysics>, String> {
    let Some(v) = v.get("physics").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let group = v
        .get("group")
        .and_then(Value::as_u64)
        .and_then(|n| u16::try_from(n).ok())
        .ok_or("fragment physics group must be u16")?;
    let parent_group = v
        .get("parent_group")
        .filter(|v| !v.is_null())
        .map(|p| {
            p.as_u64()
                .and_then(|n| u16::try_from(n).ok())
                .ok_or("fragment physics parent_group must be u16")
        })
        .transpose()?;
    if parent_group == Some(group) {
        return Err("fragment physics group cannot parent itself".into());
    }
    let entries = v
        .get("children")
        .and_then(Value::as_array)
        .ok_or("fragment physics children must be an array")?;
    let mut seen = std::collections::BTreeSet::new();
    let mut children = Vec::new();
    for child in entries {
        let component = child
            .get("component")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("fragment physics component must be u32")?;
        if !seen.insert(component) {
            return Err("duplicate fragment physics component".into());
        }
        let mass = number(child, "mass")?;
        let damaged_mass = number(child, "damaged_mass")?;
        let inertia_diagonal: [f32; 3] = vector(child, "inertia_diagonal")?;
        if mass < 0.0 || damaged_mass < 0.0 || inertia_diagonal.iter().any(|n| *n < 0.0) {
            return Err("fragment physics mass/inertia cannot be negative".into());
        }
        let mut convex_hulls = Vec::new();
        for hull in child
            .get("convex_hulls")
            .and_then(Value::as_array)
            .ok_or("fragment convex_hulls must be an array")?
        {
            let points = hull
                .as_array()
                .filter(|a| (4..=4096).contains(&a.len()))
                .ok_or("fragment hull must contain 4..4096 points")?;
            let mut decoded = Vec::new();
            for p in points {
                decoded.push(vector(&serde_json::json!({"p":p}), "p")?);
            }
            convex_hulls.push(decoded);
        }
        children.push(ModelFragmentPhysicsChild {
            component,
            mass,
            damaged_mass,
            center_of_mass: vector(child, "center_of_mass")?,
            inertia_diagonal,
            convex_hulls,
        });
    }
    let p = ModelFragmentPartPhysics {
        group,
        parent_group,
        strength: number(v, "strength")?,
        latch_strength: number(v, "latch_strength")?,
        transmission_up: number(v, "transmission_up")?,
        transmission_down: number(v, "transmission_down")?,
        joint_stiffness: number(v, "joint_stiffness")?,
        soft_angle_limits: vector(v, "soft_angle_limits")?,
        flags: v
            .get("flags")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("fragment flags must be u32")?,
        min_damage_force: number(v, "min_damage_force")?,
        damage_health: number(v, "damage_health")?,
        weapon_health: number(v, "weapon_health")?,
        weapon_scale: number(v, "weapon_scale")?,
        vehicle_scale: number(v, "vehicle_scale")?,
        explosion_scale: number(v, "explosion_scale")?,
        object_scale: number(v, "object_scale")?,
        children,
    };
    if [
        p.transmission_up,
        p.transmission_down,
        p.min_damage_force,
        p.damage_health,
        p.weapon_health,
        p.weapon_scale,
        p.vehicle_scale,
        p.explosion_scale,
        p.object_scale,
    ]
    .iter()
    .any(|n| *n < 0.0)
    {
        return Err("fragment transmission, health and damage scales cannot be negative".into());
    }
    Ok(Some(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn authored() -> Value {
        serde_json::json!({"physics":{
        "group":4,"parent_group":0,"strength":-1,"latch_strength":120,
        "transmission_up":0.25,"transmission_down":0.5,"joint_stiffness":2,
        "soft_angle_limits":[-1,1,1,1],"flags":0,"min_damage_force":100,
        "damage_health":1000,"weapon_health":0,"weapon_scale":1,"vehicle_scale":1,
        "explosion_scale":1,"object_scale":1,"children":[{"component":8,
        "mass":25.5,"damaged_mass":23,"center_of_mass":[1,2,3],
        "inertia_diagonal":[2,3,4],"convex_hulls":[[[0,0,0],[1,0,0],[0,1,0],[0,0,1]]]}]}})
    }
    #[test]
    fn preserves_physical_child_and_unbreakable_group() {
        let p = decode(&authored()).unwrap().unwrap();
        assert_eq!(p.strength, -1.0);
        assert_eq!(p.children[0].mass, 25.5);
        assert_eq!(p.children[0].component, 8);
        assert_eq!(p.parent_group, Some(0));
        assert_eq!(p.children[0].convex_hulls.len(), 1);
        assert!(decode(&serde_json::json!({})).unwrap().is_none());
    }
    #[test]
    fn rejects_invalid_or_duplicate_physical_data() {
        let mut v = authored();
        v["physics"]["children"][0]["mass"] = serde_json::json!(-1);
        assert!(decode(&v).is_err());
        let mut v = authored();
        let c = v["physics"]["children"][0].clone();
        v["physics"]["children"].as_array_mut().unwrap().push(c);
        assert!(decode(&v).is_err());
    }
}
