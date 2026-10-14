//! Provider-neutral, opt-in transport for immutable runtime snapshots.
//! Full diagnostics remain unchanged. Modules without a decoder keep full JSON.
use serde_json::{json, Value};

#[derive(Default)]
pub(super) struct SnapshotDelta {
    previous: Option<Value>,
    revision: u64,
}

impl SnapshotDelta {
    pub(super) fn revision(&self) -> Option<u64> {
        self.previous.as_ref().map(|_| self.revision)
    }

    pub(super) fn next_revision(&self) -> u64 {
        self.revision.wrapping_add(1).max(1)
    }

    pub(super) fn full(current: &Value, revision: u64) -> Value {
        json!({"format":"newviso.runtime_delta.v1", "revision":revision,
            "base_revision":null, "value":current})
    }

    pub(super) fn encode(&self, current: &Value) -> Value {
        let Some(previous) = self.previous.as_ref() else {
            return Self::full(current, self.next_revision());
        };
        let mut operations = Vec::new();
        diff(previous, current, &mut Vec::new(), &mut operations);
        json!({"format":"newviso.runtime_delta.v1", "revision":self.next_revision(),
            "base_revision":self.revision, "operations":operations})
    }

    // Called only after all module responses have been accepted. A module already
    // advanced before another module failed receives a full base on the retry.
    pub(super) fn commit(&mut self, current: Value) {
        self.previous = Some(current);
        self.revision = self.next_revision();
    }
}

// Preserve signed zero and number representation as observed by script code.
fn exact_equal(previous: &Value, current: &Value) -> bool {
    match (previous, current) {
        (Value::Object(a), Value::Object(b)) => a.len() == b.len()
            && a.iter().zip(b).all(|((ka, va), (kb, vb))| ka == kb && exact_equal(va, vb)),
        (Value::Array(a), Value::Array(b)) => a.len() == b.len()
            && a.iter().zip(b).all(|(a, b)| exact_equal(a, b)),
        (Value::Number(a), Value::Number(b)) if a.is_f64() && b.is_f64() =>
            a.as_f64().unwrap().to_bits() == b.as_f64().unwrap().to_bits(),
        _ => previous == current,
    }
}

enum PathSegment<'a> {
    Key(&'a str),
    Index(usize),
}

fn path_value(path: &[PathSegment<'_>]) -> Value {
    Value::Array(path.iter().map(|segment| match segment {
        PathSegment::Key(key) => Value::String((*key).to_owned()),
        PathSegment::Index(index) => json!(index),
    }).collect())
}

// Compare transport sizes without allocating two temporary byte buffers.
struct EncodedSize(usize);
impl std::io::Write for EncodedSize {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}
fn encoded_size<T: serde::Serialize + ?Sized>(value: &T) -> Option<usize> {
    let mut size = EncodedSize(0);
    serde_json::to_writer(&mut size, value).ok()?;
    Some(size.0)
}

// Path segments borrow keys. Unchanged fields allocate no temporary strings.
fn diff<'a>(previous: &'a Value, current: &'a Value, path: &mut Vec<PathSegment<'a>>, out: &mut Vec<Value>) {
    // Equal descriptor subtrees need no path construction or keyed lookups.
    if exact_equal(previous, current) {
        return;
    }
    match (previous, current) {
        (Value::Object(old), Value::Object(new)) => {
            for key in old.keys().filter(|key| !new.contains_key(*key)) {
                path.push(PathSegment::Key(key));
                out.push(json!({"path":path_value(path),"remove":true}));
                path.pop();
            }
            for (key, value) in new {
                path.push(PathSegment::Key(key));
                if let Some(old) = old.get(key) {
                    diff(old, value, path, out);
                } else {
                    out.push(json!({"path":path_value(path),"value":value}));
                }
                path.pop();
            }
        }
        (Value::Array(old), Value::Array(new)) if old.len() == new.len() => {
            let start = out.len();
            let mut changed_elements = 0usize;
            for (index, (old, new)) in old.iter().zip(new).enumerate() {
                let element_start = out.len();
                path.push(PathSegment::Index(index));
                diff(old, new, path, out);
                path.pop();
                if out.len() != element_start { changed_elements += 1; }
            }
            // Dense vector/body updates otherwise produce thousands of paths
            // which each decoder must traverse. Replace the complete array only
            // when its actual JSON is no larger than the fine-grained changes.
            // Sparse descriptor edits keep their existing narrow paths.
            if out.len() - start >= 2 && changed_elements.saturating_mul(2) >= new.len() {
                let replacement = json!({"path":path_value(path),"value":current});
                if encoded_size(&out[start..]).zip(encoded_size(&replacement))
                    .is_some_and(|(fine, coarse)| coarse <= fine)
                {
                    out.truncate(start);
                    out.push(replacement);
                }
            }
        }
        _ => out.push(json!({"path":path_value(path),"value":current})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(base: &Value, delta: &Value) -> Value {
        if delta["base_revision"].is_null() {
            return delta["value"].clone();
        }
        let mut next = base.clone();
        for op in delta["operations"].as_array().unwrap() {
            let path = op["path"].as_array().unwrap();
            if path.is_empty() {
                next = op["value"].clone();
                continue;
            }
            let mut parent = &mut next;
            for component in &path[..path.len() - 1] {
                parent = if let Some(index) = component.as_u64() {
                    &mut parent[index as usize]
                } else {
                    &mut parent[component.as_str().unwrap()]
                };
            }
            let last = path.last().unwrap();
            if op["remove"] == true {
                parent.as_object_mut().unwrap().remove(last.as_str().unwrap());
            } else if let Some(index) = last.as_u64() {
                parent[index as usize] = op["value"].clone();
            } else {
                parent[last.as_str().unwrap()] = op["value"].clone();
            }
        }
        next
    }

    #[test]
    fn preserves_null_removal_arrays_type_changes_and_property_names() {
        let old = json!({"a":[{"x":1,"drop":2},null],"b":null,"c":[1,2],
            "d":{"nested":"before"},"__proto__":{"safe":1},"slash/key":8});
        let new = json!({"a":[{"x":null,"added":true},3],"c":[4],
            "d":false,"__proto__":{"safe":2},"slash/key":null});
        let mut snapshots = SnapshotDelta::default();
        let first = snapshots.encode(&old);
        assert_eq!(apply(&Value::Null, &first), old);
        snapshots.commit(old.clone());
        let delta = snapshots.encode(&new);
        assert_eq!(delta["base_revision"], 1);
        assert_eq!(delta["revision"], 2);
        assert_eq!(apply(&old, &delta), new);
        snapshots.commit(new.clone());
        assert_eq!(snapshots.encode(&new)["operations"], json!([]));
    }

    #[test]
    fn float_sign_and_integer_float_representation_survive_transport() {
        let old = json!({"zero":0.0,"numeric":1});
        let new = json!({"zero":-0.0,"numeric":1.0});
        let mut snapshots = SnapshotDelta::default();
        snapshots.commit(old.clone());
        let delta = snapshots.encode(&new);
        assert_eq!(delta["operations"].as_array().unwrap().len(), 2);
        let result = apply(&old, &delta);
        assert_eq!(result["zero"].as_f64().unwrap().to_bits(), (-0.0f64).to_bits());
        assert!(result["numeric"].as_number().unwrap().is_f64());
    }

    #[test]
    fn unchanged_large_descriptors_are_not_retransmitted() {
        let old = json!({"vehicle_presentation":[{"entity":5,
            "parts":vec![json!({"mesh_names":["authored_name"],"description":"x".repeat(1024)});1000],
            "cabin":{"horn":false}}]});
        let mut new = old.clone();
        new["vehicle_presentation"][0]["cabin"]["horn"] = json!(true);
        let mut snapshots = SnapshotDelta::default();
        snapshots.commit(old.clone());
        let delta = snapshots.encode(&new);
        assert!(serde_json::to_vec(&delta).unwrap().len() < 300);
        assert_eq!(apply(&old, &delta), new);
    }

    #[test]
    fn preparing_an_unsuccessful_invocation_does_not_advance_the_base() {
        let mut snapshots = SnapshotDelta::default();
        snapshots.commit(json!({"x":1}));
        let _rejected = snapshots.encode(&json!({"x":2}));
        assert_eq!(snapshots.revision(), Some(1));
        assert_eq!(snapshots.encode(&json!({"x":3}))["base_revision"], 1);
        snapshots.commit(json!({"x":3}));
        assert_eq!(snapshots.revision(), Some(2));
    }

    #[test]
    fn dense_physics_arrays_use_one_compact_transaction() {
        let bodies = (0..144).map(|i| json!({"entity_key":i.to_string(),
            "position":[0.0,1.0,2.0],"rotation":[0.0,0.0,0.0,1.0],
            "velocity":[0.0,0.0,0.0],"bounds_min":[0.0,0.0,0.0],
            "bounds_max":[1.0,1.0,1.0],"kind":"dynamic"})).collect::<Vec<_>>();
        let old = json!({"physics":{"bodies":bodies}});
        let mut new = old.clone();
        for (i, body) in new["physics"]["bodies"].as_array_mut().unwrap().iter_mut().enumerate() {
            body["position"] = json!([i as f64 + 0.1,2.1,3.1]);
            body["rotation"] = json!([0.2,0.3,0.4,0.8]);
            body["velocity"] = json!([1.2,2.3,3.4]);
            body["bounds_min"] = json!([1.0,2.0,3.0]);
            body["bounds_max"] = json!([2.0,3.0,4.0]);
        }
        let mut snapshots = SnapshotDelta::default();snapshots.commit(old.clone());
        let delta = snapshots.encode(&new);
        assert_eq!(delta["operations"].as_array().unwrap().len(),1);
        assert_eq!(delta["operations"][0]["path"],json!(["physics","bodies"]));
        assert_eq!(apply(&old,&delta),new);
    }

    #[test]
    fn sparse_record_updates_do_not_retransmit_large_immutable_descriptors() {
        let old=json!({"records":vec![json!({"position":[0.0,0.0,0.0],"description":"x".repeat(4096)});144]});
        let mut new=old.clone();new["records"][73]["position"]=json!([1.0,2.0,3.0]);new["records"][91]["position"]=json!([4.0,5.0,6.0]);
        let mut snapshots=SnapshotDelta::default();snapshots.commit(old.clone());
        let delta=snapshots.encode(&new);
        assert!(serde_json::to_vec(&delta).unwrap().len()<400);
        assert_eq!(apply(&old,&delta),new);
    }

    #[test]
    fn changed_numeric_vectors_are_sent_as_a_single_vector() {
        let old=json!({"position":[0.0,1.0,2.0]});let new=json!({"position":[0.3,1.4,2.5]});
        let mut snapshots=SnapshotDelta::default();snapshots.commit(old.clone());
        let delta=snapshots.encode(&new);
        assert_eq!(delta["operations"].as_array().unwrap().len(),1);
        assert_eq!(delta["operations"][0]["path"],json!(["position"]));
        assert_eq!(apply(&old,&delta),new);
    }
}

