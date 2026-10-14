//! Model geometry, material and fragment regression tests.
use super::*;

#[cfg(test)]
mod dent_tests {
    use super::*;
    #[test]
    fn local_dent_deforms_contact_region_and_preserves_remote_geometry() {
        let dent = SceneModelDent {
            point: [0.0; 3],
            displacement: [0.0, -0.2, 0.0],
            radius: 1.0,
        };
        let (point, normal, tangent) = apply_fragment_dents(
            Vec3::ZERO,
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(1.0, 0.0, 0.0),
            &[dent.clone()],
        );
        assert!((point.y + 0.2).abs() < 1.0e-6);
        assert!(normal.y > 0.99 && tangent.x > 0.99);
        let (remote, _, _) =
            apply_fragment_dents(Vec3::new(2.0, 0.0, 0.0), normal, tangent, &[dent]);
        assert!((remote.x - 2.0).abs() < 1.0e-6 && remote.y == 0.0);
    }
}

#[cfg(test)]
mod joint_partition_tests {
    use super::*;
    use newviso_model::{
        Bounds3, IndexBuffer, MeshPrimitive, MeshResource, ModelJoint, ModelSkeleton, VertexStream,
    };

    fn stream(semantic: VertexSemantic, width: usize, values: &[f32]) -> VertexStream {
        VertexStream {
            semantic,
            format: match width {
                3 => ModelVertexFormat::Float32x3,
                _ => ModelVertexFormat::Float32x4,
            },
            stride: (width * 4) as u32,
            vertex_count: 6,
            data: Arc::from(
                values
                    .iter()
                    .flat_map(|v| v.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
        }
    }

    #[test]
    fn mixed_head_and_hand_mesh_keeps_every_triangle_and_material() {
        let bounds = Bounds3 {
            min: [-1.0; 3],
            max: [1.0; 3],
        };
        let joint = |name: &str, parent: Option<u16>| ModelJoint {
            name: name.into(),
            tag: 0,
            parent,
            inverse_bind_matrix: identity_matrix(),
            bind_translation: [0.0; 3],
            bind_rotation: [0.0, 0.0, 0.0, 1.0],
            bind_scale: [1.0; 3],
        };
        let skeleton = ModelSkeleton {
            name: "test".into(),
            joints: vec![
                joint("root", None),
                joint("neck", Some(0)),
                joint("eye", Some(1)),
                joint("forearm", Some(0)),
                joint("hand", Some(3)),
            ],
        };
        let indices = [0u32, 1, 2, 3, 4, 5];
        let influence: Vec<f32> = [2.0, 2.0, 2.0, 4.0, 4.0, 4.0]
            .into_iter()
            .flat_map(|j| [j, 0.0, 0.0, 0.0])
            .collect();
        let weights: Vec<f32> = (0..6).flat_map(|_| [1.0, 0.0, 0.0, 0.0]).collect();
        let mesh = MeshResource {
            name: "shared_skin".into(),
            render_passes: Default::default(),
            bounds,
            vertex_streams: vec![
                stream(
                    VertexSemantic::Position,
                    3,
                    &[
                        0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0,
                        0.0, 1.0, 1.0,
                    ],
                ),
                stream(VertexSemantic::JointIndices, 4, &influence),
                stream(VertexSemantic::JointWeights, 4, &weights),
            ],
            index_buffer: IndexBuffer {
                format: ModelIndexFormat::U32,
                index_count: 6,
                data: Arc::from(
                    indices
                        .iter()
                        .flat_map(|i| i.to_le_bytes())
                        .collect::<Vec<_>>(),
                ),
            },
            primitives: vec![MeshPrimitive {
                first_index: 0,
                index_count: 6,
                base_vertex: 0,
                material_slot: Some(7),
            }],
        };
        let model = ModelResource {
            id: AssetId(1),
            name: "mixed".into(),
            bounds,
            meshes: vec![mesh],
            material_slots: vec![],
            skin_source_to_model: identity_matrix(),
            skeleton: Some(skeleton),
            animations: vec![],
            fragment: None,
        };
        let (vertices, ranges) = expand_model_triangles(&model).unwrap();
        assert_eq!(vertices.len(), 6);
        assert_eq!(ranges.len(), 2);
        assert!(ranges
            .iter()
            .all(|range| range.mesh_name.as_ref() == "shared_skin"
                && range.material_slot == Some(7)
                && range.vertex_count == 3));
        assert!(ranges[0].joint_lineage.iter().any(|j| j == "neck"));
        assert!(ranges[1].joint_lineage.iter().any(|j| j == "forearm"));
        assert!(!ranges[1].joint_lineage.iter().any(|j| j == "neck"));
        // Model pass coverage survives skin partitioning and local draw expansion.
        let mut shadow_model = model.clone();
        shadow_model.meshes[0].render_passes = newviso_model::MeshRenderPasses {
            color: false, shadow: true,
        };
        let (shadow_vertices, shadow_ranges) = expand_model_triangles(&shadow_model).unwrap();
        assert_eq!(shadow_vertices.len(), vertices.len());
        assert!(shadow_ranges.iter().all(|range| !range.render_passes.color && range.render_passes.shadow));
    }
}

#[cfg(test)]
mod fragment_surface_tests {
    use super::*;
    #[test]
    fn glass_triangle_hit_keeps_the_hit_uv_instead_of_box_overlap() {
        let (hit, barycentric) = closest_fragment_triangle(
            [0.25, 0.25, 0.1],
            [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        )
        .unwrap();
        assert_eq!(hit, [0.25, 0.25, 0.0]);
        assert_eq!(barycentric, [0.5, 0.25, 0.25]);
    }
    #[test]
    fn collapsed_broken_glass_is_not_a_damage_surface() {
        assert!(closest_fragment_triangle([0.0; 3], [[0.0; 3]; 3]).is_none());
    }
}

#[cfg(test)]
mod fragment_capture_tests {
    use super::*;

    fn vertex(position: Vec3) -> AssetTriangleVertex {
        AssetTriangleVertex {
            position,
            normal: Vec3::Y,
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: [1.0; 4],
            uv: [0.0; 2],
            joints: [0; 8],
            weights: [0.0; 8],
            skin_influences: 0,
        }
    }

    #[test]
    fn hidden_fragment_recovers_authored_geometry_with_its_articulated_pose() {
        let pose = SceneModelPartPose {
            visible: false,
            pivot: [1.0, 0.0, 0.0],
            translation: [0.0, 2.0, 0.0],
            rotation_degrees: [0.0, 90.0, 0.0],
            ..Default::default()
        };
        let bind = vertex(Vec3::new(2.0, 0.0, 0.0));
        let hidden = posed_fragment_vertex(bind, &pose, false).0;
        let captured = posed_fragment_vertex(bind, &pose, true).0;
        assert!((hidden.x - 1.0).abs() < 1.0e-6 && (hidden.y - 2.0).abs() < 1.0e-6);
        assert!((captured.x - 1.0).abs() < 1.0e-5 && (captured.y - 2.0).abs() < 1.0e-5);
        assert!((captured.z.abs() - 1.0).abs() < 1.0e-5);
    }

    #[test]
    fn visible_fragment_capture_matches_render_deformation_and_normals() {
        let pose = SceneModelPartPose {
            pivot: [0.5, 0.0, 0.0],
            translation: [0.0, 0.3, 0.0],
            rotation_degrees: [20.0, 40.0, -15.0],
            post_rotation_scale: [1.0, 0.8, 1.0],
            dents: vec![SceneModelDent {
                point: [1.0, 0.0, 0.0],
                displacement: [0.0, -0.1, 0.0],
                radius: 2.0,
            }],
            ..Default::default()
        };
        let bind = vertex(Vec3::new(1.0, 0.0, 0.0));
        let (position, normal, tangent) = posed_fragment_vertex(bind, &pose, false);
        let (captured, captured_normal, captured_tangent) =
            posed_fragment_vertex(bind, &pose, true);
        assert_eq!(position, captured);
        assert_eq!(normal, captured_normal);
        assert_eq!(tangent, captured_tangent);
    }
}


#[cfg(test)]
mod authored_vertex_color_tests {
    use super::*;
    #[test]
    fn vertex_tint_is_explicit_and_does_not_enable_imported_masks() {
        let mut resolved = SceneResolvedMaterial {
            material: Arc::new(MaterialResource {
                id: newviso_resource_runtime::AssetId(1), name: "authored".into(),
                shader: "pbr.default".into(), surface_domain: "surface".into(),
                shading_model: "pbr".into(), blend: BlendMode::Opaque, two_sided: false,
                alpha_cutoff: None, textures: Vec::new(), params: Vec::new(),
            }),
            base_color: None, normal: None, specular: None, emissive: None,
            environment: None, auxiliary_textures: BTreeMap::new(),
        };
        assert_eq!(CpuAssetMaterial::from_resolved(&resolved).uniform_data()[6] as u32 & 64, 0);
        Arc::make_mut(&mut resolved.material).params.push(newviso_materials::MaterialParameter {
            name: "use_vertex_color".into(), value: MaterialParamValue::Bool(true),
        });
        assert_eq!(CpuAssetMaterial::from_resolved(&resolved).uniform_data()[6] as u32 & 64, 64);
        Arc::make_mut(&mut resolved.material).params[0].value = MaterialParamValue::Bool(false);
        assert_eq!(CpuAssetMaterial::from_resolved(&resolved).uniform_data()[6] as u32 & 64, 0);
    }

    #[test]
    fn gltf_factors_opt_into_channel_contract_without_changing_legacy_materials() {
        let mut resolved = SceneResolvedMaterial {
            material: Arc::new(MaterialResource {
                id: AssetId(1), name: "gltf".into(), shader: "pbr.default".into(),
                surface_domain: "surface".into(), shading_model: "pbr".into(),
                blend: BlendMode::Opaque, two_sided: true, alpha_cutoff: None,
                textures: Vec::new(), params: Vec::new(),
            }), base_color: None, normal: None, specular: None, emissive: None,
            environment: None, auxiliary_textures: BTreeMap::new(),
        };
        assert_eq!(CpuAssetMaterial::from_resolved(&resolved).uniform_data()[6] as u32 & 256, 0);
        Arc::make_mut(&mut resolved.material).params.extend([
            newviso_materials::MaterialParameter {name: "metallic_factor".into(), value: MaterialParamValue::Float(0.0)},
            newviso_materials::MaterialParameter {name: "roughness_factor".into(), value: MaterialParamValue::Float(0.8)},
        ]);
        let cpu = CpuAssetMaterial::from_resolved(&resolved);
        let uniforms = cpu.uniform_data();
        assert_ne!(uniforms[6] as u32 & 256, 0);
        assert_eq!(uniforms[9], 0.0);
        assert_eq!(uniforms[10], 0.8);
        assert!(cpu.two_sided);
    }
}

