use super::*;
use serde_json::{json, Value};

fn scene() -> Value {
    let r = SceneTypeRegistry::builtin();
    json!({"schema":"newviso.scene.v1","version":1,"metadata":{"keep":true},
        "entities":[r.types["camera"].template,r.types["cube"].template]})
}

#[test]
fn transaction_rolls_back_entire_batch_and_history() {
    let r = SceneTypeRegistry::builtin();
    let mut d = SceneDocument::new(scene(), r).unwrap();
    let before = d.value().clone();
    assert!(d
        .apply(
            0,
            &[
                SceneEdit::Set {
                    entity: 1,
                    path: "/transform/position".into(),
                    value: json!([2, 3, 4])
                },
                SceneEdit::Set {
                    entity: 0,
                    path: "/camera/near".into(),
                    value: json!(900)
                },
            ],
            r
        )
        .is_err());
    assert_eq!(d.value(), &before);
    assert_eq!(d.revision(), 0);
    assert!(!d.can_undo());
}

#[test]
fn edit_undo_redo_preserves_extension_data_and_checks_revision() {
    let r = SceneTypeRegistry::builtin();
    let mut s = scene();
    s["entities"][1]["plugin.custom"] = json!({"opaque":[1,2,3]});
    let mut d = SceneDocument::new(s.clone(), r).unwrap();
    let edit = SceneEdit::Set {
        entity: 1,
        path: "/transform/position".into(),
        value: json!([2, 3, 4]),
    };
    d.apply(0, &[edit.clone()], r).unwrap();
    assert!(d.apply(0, &[edit], r).is_err());
    let after = d.value().clone();
    assert!(d.undo());
    assert_eq!(d.value(), &s);
    assert!(d.redo());
    assert_eq!(d.value(), &after);
    assert_eq!(after["metadata"], s["metadata"]);
    assert_eq!(
        after["entities"][1]["plugin.custom"],
        s["entities"][1]["plugin.custom"]
    );
}

#[test]
fn component_and_entity_lifecycle_are_validated() {
    let r = SceneTypeRegistry::builtin();
    let mut d = SceneDocument::new(scene(), r).unwrap();
    assert!(d.apply(0, &[SceneEdit::Remove { entity: 0 }], r).is_err());
    assert!(d
        .apply(
            0,
            &[SceneEdit::RemoveComponent {
                entity: 1,
                component: "mesh".into()
            }],
            r
        )
        .is_err());
    d.apply(
        0,
        &[
            SceneEdit::Duplicate { entity: 1 },
            SceneEdit::AddComponent {
                entity: 2,
                component: "lod".into(),
            },
        ],
        r,
    )
    .unwrap();
    assert_eq!(d.value()["entities"][2]["name"], "Cube 2");
    assert!(d.value()["entities"][2]["lod"].is_object());
    d.undo();
    assert_eq!(d.value()["entities"].as_array().unwrap().len(), 2);
}

#[test]
fn invalid_vectors_references_and_registry_collisions_are_rejected() {
    let r = SceneTypeRegistry::builtin();
    for value in [json!([1, 2]), json!([1, "x", 3]), json!([1, 2, 1e300])] {
        assert!(r
            .property("/transform/position")
            .unwrap()
            .validate(&value)
            .is_err());
    }
    assert!(r
        .property("/mesh/asset")
        .unwrap()
        .validate(&json!("../model"))
        .is_err());
    let mut registry = r.clone();
    assert!(registry.register(r.clone()).is_err());
    assert_eq!(registry.components.len(), r.components.len());
}

#[test]
fn deletion_cannot_silently_retarget_parent_handles() {
    let r = SceneTypeRegistry::builtin();
    let mut s = scene();
    s["entities"][1]["parent_id"] = json!(1);
    let mut d = SceneDocument::new(s.clone(), r).unwrap();
    assert!(d.apply(0, &[SceneEdit::Remove { entity: 1 }], r).is_err());
    assert_eq!(d.value(), &s);
}

#[test]
fn inspector_cannot_save_native_invalid_destruction_or_camera_settings() {
    let r = SceneTypeRegistry::builtin();
    let mut d = SceneDocument::new(scene(), r).unwrap();
    d.apply(
        0,
        &[SceneEdit::AddComponent {
            entity: 1,
            component: "destructible".into(),
        }],
        r,
    )
    .unwrap();
    let before = d.value().clone();
    for path in [
        "/destructible/health",
        "/destructible/break_impulse",
        "/destructible/density",
    ] {
        assert!(d
            .apply(
                d.revision(),
                &[SceneEdit::Set {
                    entity: 1,
                    path: path.into(),
                    value: json!(0)
                }],
                r
            )
            .is_err());
        assert_eq!(d.value(), &before);
    }
    assert!(r.property("/destructible/max_health").is_none());
    assert!(d
        .apply(
            d.revision(),
            &[SceneEdit::Set {
                entity: 0,
                path: "/transform/up".into(),
                value: json!([0, 0, 0])
            }],
            r
        )
        .is_err());
    assert_eq!(d.value(), &before);
}

#[test]
fn component_applicability_prevents_editing_unused_native_fields() {
    let r = SceneTypeRegistry::builtin();
    let mut d = SceneDocument::new(scene(), r).unwrap();
    assert!(d
        .apply(
            0,
            &[SceneEdit::AddComponent {
                entity: 0,
                component: "collider".into()
            }],
            r
        )
        .is_err());
    assert!(d
        .apply(
            0,
            &[SceneEdit::AddComponent {
                entity: 1,
                component: "camera".into()
            }],
            r
        )
        .is_err());
    let asset = serde_json::json!({"kind":"mesh","mesh":{"asset":"models/existing"}});
    assert!(!r.components["material"].applies_to(&asset));
    assert!(r.components["collider"].applies_to(&asset));
}

#[test]
fn character_capability_is_authored_transactionally_and_survives_undo() {
    let r = SceneTypeRegistry::builtin();
    let mut d = SceneDocument::new(scene(), r).unwrap();
    d.apply(
        0,
        &[SceneEdit::AddComponent {
            entity: 1,
            component: "character".into(),
        }],
        r,
    )
    .unwrap();
    let disabled = d.value().clone();
    assert!(d
        .apply(
            d.revision(),
            &[SceneEdit::Set {
                entity: 1,
                path: "/character/enabled".into(),
                value: json!(true)
            }],
            r
        )
        .is_err());
    assert_eq!(d.value(), &disabled);
    d.apply(
        d.revision(),
        &[
            SceneEdit::Set {
                entity: 1,
                path: "/character/foot_placement_profile".into(),
                value: json!("rigs/custom.feet.json"),
            },
            SceneEdit::Set {
                entity: 1,
                path: "/character/enabled".into(),
                value: json!(true),
            },
        ],
        r,
    )
    .unwrap();
    let configured = d.value().clone();
    assert!(d.undo());
    assert_eq!(d.value(), &disabled);
    assert!(d.redo());
    assert_eq!(d.value(), &configured);
    d.apply(
        d.revision(),
        &[SceneEdit::RemoveComponent {
            entity: 1,
            component: "character".into(),
        }],
        r,
    )
    .unwrap();
    assert!(d.value()["entities"][1].get("character").is_none());
}
