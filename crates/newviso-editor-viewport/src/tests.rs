use super::*;
fn camera() -> Camera {
    Camera {
        position: [0.0, 0.0, 8.0],
        target: [0.0; 3],
        up: [0.0, 1.0, 0.0],
        fov_y_degrees: 60.0,
        near: 0.1,
        far: 1000.0,
        size: [1280, 800],
    }
}
fn close(a: V3, b: V3) {
    assert!(length(sub(a, b)) < 1e-7, "{a:?} != {b:?}");
}
#[test]
fn projection_and_ray_agree_off_center_and_after_navigation() {
    let mut c = camera();
    c.fly([35.0, -10.0], [1.0, 0.0, 1.0], 0.016, 8.0);
    for p in [[640.0, 400.0], [355.0, 210.0], [1000.0, 670.0]] {
        let projected = c.project(c.ray(p).at(20.0)).unwrap();
        assert!((p[0] - projected[0]).abs() < 1e-8 && (p[1] - projected[1]).abs() < 1e-8);
    }
}
#[test]
fn inverse_transform_preserves_world_ray_parameter_with_mirrored_nonuniform_scale() {
    let t = Transform {
        position: [3.0, -2.0, 1.0],
        rotation_degrees: [25.0, 70.0, -38.0],
        scale: [-2.0, 0.3, 4.0],
    };
    let r = Ray {
        origin: [7.0, 3.0, 9.0],
        direction: normal([-2.0, 1.0, -7.0]),
    };
    close(t.point(t.inverse_ray(r).at(5.0)), r.at(5.0));
}
#[test]
fn translation_is_relative_to_drag_start_and_snaps_without_accumulation() {
    let c = camera();
    let mut g = Gizmo::default();
    let t = Transform::default();
    let p = c.project([0.7, 0.0, 0.0]).unwrap();
    let mut d = g.begin(c, t, p, Handle::Axis(0)).unwrap();
    let snap = Snap {
        enabled: true,
        ..Snap::default()
    };
    let next = c.project([1.9, 0.0, 0.0]).unwrap();
    close(d.update(next, snap).position, [1.0, 0.0, 0.0]);
    close(d.update(p, snap).position, [0.0; 3]);
    assert_eq!(d.original, t);
}
#[test]
fn plane_translation_locks_third_axis() {
    let c = camera();
    let mut g = Gizmo::default();
    let mut d = g
        .begin(c, Transform::default(), [640.0, 400.0], Handle::Plane(0, 1))
        .unwrap();
    let t = d.update(c.project([2.0, 1.0, 0.0]).unwrap(), Snap::default());
    close(t.position, [2.0, 1.0, 0.0]);
}
#[test]
fn local_basis_and_rotation_composition_match_render_euler_order() {
    let t = Transform {
        rotation_degrees: [20.0, 45.0, 35.0],
        ..Transform::default()
    };
    let g = Gizmo {
        space: Space::Local,
        ..Gizmo::default()
    };
    close(g.basis(t)[0], rotate(axis(0), t.rotation_degrees));
    let next = compose_rotation(t.rotation_degrees, g.basis(t)[1], 37.0);
    for i in 0..3 {
        close(
            rotate(axis(i), next),
            rotate_axis(
                rotate(axis(i), t.rotation_degrees),
                g.basis(t)[1],
                37f64.to_radians(),
            ),
        );
    }
}
#[test]
fn axis_rotation_unwraps_and_snaps() {
    let c = camera();
    let mut g = Gizmo {
        tool: Tool::Rotate,
        ..Gizmo::default()
    };
    let mut d = g
        .begin(
            c,
            Transform::default(),
            c.project([1.0, 0.0, 0.0]).unwrap(),
            Handle::Axis(2),
        )
        .unwrap();
    for a in [90f64, 175.0, 185.0, 270.0] {
        d.update(
            c.project([a.to_radians().cos(), a.to_radians().sin(), 0.0])
                .unwrap(),
            Snap {
                enabled: true,
                ..Snap::default()
            },
        );
    }
    close(
        rotate(axis(0), d.current.rotation_degrees),
        [0.0, -1.0, 0.0],
    );
}
#[test]
fn scale_never_crosses_zero_and_keeps_mirror_sign() {
    let c = camera();
    let mut g = Gizmo {
        tool: Tool::Scale,
        ..Gizmo::default()
    };
    let mut d = g
        .begin(
            c,
            Transform {
                scale: [-2.0, 1.0, 1.0],
                ..Transform::default()
            },
            [640.0, 400.0],
            Handle::Center,
        )
        .unwrap();
    let t = d.update(
        [-5000.0, 5000.0],
        Snap {
            enabled: true,
            ..Snap::default()
        },
    );
    assert!(t.valid() && t.scale[0] < 0.0 && t.scale[1] > 0.0);
}
#[test]
fn bounds_parallel_rays_and_behind_camera_are_rejected() {
    assert_eq!(
        Ray {
            origin: [2.0, 0.0, 5.0],
            direction: [0.0, 0.0, -1.0]
        }
        .bounds([-1.0; 3], [1.0; 3]),
        None
    );
    assert!(camera().project([0.0, 0.0, 9.0]).is_none());
}
#[test]
fn focus_pan_and_fly_keep_a_valid_camera() {
    let mut c = camera();
    c.focus(Bounds {
        min: [30.0, 1.0, -3.0],
        max: [34.0, 5.0, 1.0],
    });
    close(c.target, [32.0, 3.0, -1.0]);
    c.pan([40.0, 15.0], 8.0);
    let d = length(sub(c.target, c.position));
    c.orbit(c.target, [100.0, 50.0]);
    assert!((d - length(sub(c.target, c.position))).abs() < 1e-7);
    c.fly([0.0, 100000.0], [1.0, 1.0, 1.0], 9.0, 12.0);
    assert!(c.forward()[1].abs() < 1.0);
}

#[test]
fn focus_fits_the_visible_docks_and_orbit_preserves_the_screen_pivot() {
    let mut c = camera();
    let b = Bounds {
        min: [-1.0; 3],
        max: [1.0; 3],
    };
    let rect = [310.0, 38.0, 810.0, 620.0];
    c.focus_in_rect(b, rect);
    let center = c.project(b.center()).unwrap();
    assert!((center[0] - 560.0).abs() < 1e-7 && (center[1] - 329.0).abs() < 1e-7);
    for corner in b.corners() {
        let p = c.project(corner).unwrap();
        assert!(
            p[0] > rect[0] + 10.0
                && p[0] < rect[2] - 10.0
                && p[1] > rect[1] + 10.0
                && p[1] < rect[3] - 10.0
        );
    }
    c.orbit(b.center(), [40.0, 20.0]);
    let next = c.project(b.center()).unwrap();
    assert!((center[0] - next[0]).abs() < 1e-7 && (center[1] - next[1]).abs() < 1e-7);
}
#[test]
fn uniform_center_wins_over_a_foreshortened_depth_axis() {
    let mut c = camera();
    c.position = [0.0, 2.0, 6.0];
    let g = Gizmo {
        tool: Tool::Scale,
        ..Gizmo::default()
    };
    assert_eq!(
        g.hit(c, Transform::default(), [640.0, 400.0]),
        Some(Handle::Center)
    );
}
