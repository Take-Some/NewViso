use super::*;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TraversalGate {
    pub id: String,
    pub center: Vec3,
    pub normal: Vec3,
    pub half_width: f32,
    pub half_height: f32,
    pub locked: bool,
    pub free_egress_side: Option<i8>,
    pub blocked: bool,
    pub action_cost: f32,
}
impl TraversalGate {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty()
            || self.id.len() > 128
            || self
                .center
                .iter()
                .chain(&self.normal)
                .any(|v| !v.is_finite())
            || self.normal[1].abs() > 0.001
            || (self.normal[0].hypot(self.normal[2]) - 1.0).abs() > 0.001
            || ![self.half_width, self.half_height]
                .iter()
                .all(|v| v.is_finite() && *v > 0.0)
            || !self.action_cost.is_finite()
            || self.action_cost < 0.0
            || self.free_egress_side.is_some_and(|s| s != -1 && s != 1)
        {
            return Err("invalid navigation traversal gate".into());
        }
        Ok(())
    }
    pub fn crossing(&self, a: Vec3, b: Vec3, radius: f32) -> Option<f32> {
        let dot = |p: Vec3| {
            (p[0] - self.center[0]) * self.normal[0] + (p[2] - self.center[2]) * self.normal[2]
        };
        let da = dot(a);
        let db = dot(b);
        let delta = da - db;
        if da * db > 0.0 || delta.abs() < 1e-6 {
            return None;
        }
        let t = da / delta;
        if !(0.0..=1.0).contains(&t) {
            return None;
        }
        let p = std::array::from_fn::<_, 3, _>(|i| a[i] + t * (b[i] - a[i]));
        let lateral =
            (p[0] - self.center[0]) * self.normal[2] - (p[2] - self.center[2]) * self.normal[0];
        if lateral.abs() > self.half_width + radius
            || (p[1] - self.center[1]).abs() > self.half_height + radius
        {
            return None;
        }
        Some(if da.abs() > 1e-6 {
            da.signum()
        } else {
            -db.signum()
        })
    }
    pub fn accessible(
        &self,
        side: f32,
        access: &BTreeSet<String>,
        radius: f32,
        height: f32,
    ) -> bool {
        !self.blocked
            && self.half_width >= radius
            && 2.0 * self.half_height >= height
            && (!self.locked
                || access.contains(&self.id)
                || self.free_egress_side.is_some_and(|s| side * s as f32 > 0.0))
    }
}
impl NavigationRuntime {
    pub fn set_traversal_gates(&mut self, gates: Vec<TraversalGate>) -> Result<bool, String> {
        let mut next = BTreeMap::new();
        for gate in gates {
            gate.validate()?;
            if next.insert(gate.id.clone(), gate).is_some() {
                return Err("duplicate traversal gate".into());
            }
        }
        if self.gates == next {
            return Ok(false);
        }
        self.gates = next;
        self.revision = self.revision.wrapping_add(1).max(1);
        Ok(true)
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) fn gate_cost(&self, a: Vec3, b: Vec3, request: &PathRequest) -> Option<f32> {
        let mut cost = 0.0;
        for gate in self.gates.values() {
            if let Some(side) = gate.crossing(a, b, request.agent_radius) {
                if !gate.accessible(
                    side,
                    &request.gate_access,
                    request.agent_radius,
                    request.agent_height,
                ) {
                    return None;
                }
                cost += gate.action_cost;
            }
        }
        Some(cost)
    }
}

#[cfg(test)]
mod tests;
