//! Triangle expansion, joint partitioning, tangents and packed output.
use super::*;

pub(in crate::first_scene) fn append_local_asset_vertices(out: &mut Vec<f32>, vertices: &[AssetTriangleVertex]) {
    out.reserve(vertices.len().saturating_mul(FLOATS_PER_VERTEX));
    for vertex in vertices {
        geometry::append_local_asset_vertex(
            out,
            vertex.position,
            vertex.normal,
            vertex.tangent,
            vertex.color,
            vertex.uv,
        );
    }
}

pub(in crate::first_scene) fn expand_model_triangles(
    model: &ModelResource,
) -> Result<(Vec<AssetTriangleVertex>, Vec<AssetDrawRange>), String> {
    let mut out = Vec::new();
    let mut draw_ranges = Vec::new();

    for mesh in &model.meshes {
        let position = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Position)
            .ok_or_else(|| format!("mesh '{}' has no position stream", mesh.name))?;
        if position.format != ModelVertexFormat::Float32x3 {
            return Err(format!(
                "mesh '{}' position stream must be Float32x3",
                mesh.name
            ));
        }

        let normal = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Normal);
        let tangent = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Tangent);
        let color = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::Color(0));
        let texcoord = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::TexCoord(0));
        let joint_indices = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointIndices);
        let joint_weights = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointWeights);
        let joint_indices_extra = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointIndicesExtra);
        let joint_weights_extra = mesh
            .vertex_streams
            .iter()
            .find(|stream| stream.semantic == VertexSemantic::JointWeightsExtra);
        if joint_indices.is_some() != joint_weights.is_some()
            || joint_indices_extra.is_some() != joint_weights_extra.is_some()
        {
            return Err(format!(
                "mesh '{}' has incomplete joint index/weight stream pairs",
                mesh.name
            ));
        }

        let indices = decode_indices(&mesh.index_buffer)?;
        let primitives = if mesh.primitives.is_empty() {
            vec![(0usize, indices.len(), 0i32, None)]
        } else {
            mesh.primitives
                .iter()
                .map(|primitive| {
                    (
                        primitive.first_index as usize,
                        primitive.index_count as usize,
                        primitive.base_vertex,
                        primitive.material_slot,
                    )
                })
                .collect::<Vec<_>>()
        };

        for (first, count, base_vertex, material_slot) in primitives {
            let range_first_vertex = u32::try_from(out.len())
                .map_err(|_| "expanded model vertex offset exceeds u32".to_owned())?;
            let end = first
                .checked_add(count)
                .ok_or_else(|| format!("mesh '{}' index range overflows", mesh.name))?;
            if end > indices.len() {
                return Err(format!(
                    "mesh '{}' primitive index range {}..{} exceeds {} indices",
                    mesh.name,
                    first,
                    end,
                    indices.len()
                ));
            }

            let mut partitions = BTreeMap::<Option<u16>, Vec<AssetTriangleVertex>>::new();
            for tri in indices[first..end].chunks_exact(3) {
                let mut triangle = [AssetTriangleVertex {
                    position: Vec3::ZERO,
                    normal: Vec3::ZERO,
                    tangent: [1.0, 0.0, 0.0, 1.0],
                    color: [1.0, 1.0, 1.0, 1.0],
                    uv: [0.0, 0.0],
                    joints: [0; 8],
                    weights: [0.0; 8],
                    skin_influences: 0,
                }; 3];

                for (corner, raw_index) in tri.iter().copied().enumerate() {
                    let vertex_index = i64::from(raw_index) + i64::from(base_vertex);
                    if vertex_index < 0 {
                        return Err(format!(
                            "mesh '{}' primitive references negative vertex {}",
                            mesh.name, vertex_index
                        ));
                    }
                    let vertex_index = vertex_index as usize;
                    triangle[corner].position = read_vec3(position, vertex_index)?;
                    if let Some(stream) = normal {
                        triangle[corner].normal = read_vec3(stream, vertex_index)?;
                    }
                    if let Some(stream) = tangent {
                        triangle[corner].tangent = read_vec4(stream, vertex_index)?;
                    }
                    if let Some(stream) = color {
                        triangle[corner].color = read_vec4(stream, vertex_index)?;
                    }
                    if let Some(stream) = texcoord {
                        triangle[corner].uv = read_vec2(stream, vertex_index)?;
                    }
                    if let (Some(indices_stream), Some(weights_stream)) =
                        (joint_indices, joint_weights)
                    {
                        read_skin_influences(
                            &mut triangle[corner],
                            indices_stream,
                            weights_stream,
                            joint_indices_extra,
                            joint_weights_extra,
                            vertex_index,
                        )?;
                    }
                }

                if normal.is_none() {
                    let edge_a = triangle[1].position.sub(triangle[0].position);
                    let edge_b = triangle[2].position.sub(triangle[0].position);
                    let face_normal = edge_a.cross(edge_b).normalized();
                    for vertex in &mut triangle {
                        vertex.normal = face_normal;
                    }
                }
                if tangent.is_none() {
                    generate_triangle_tangent(&mut triangle);
                }
                let joint = dominant_triangle_joint(&triangle, model.skeleton.as_ref());
                partitions
                    .entry(joint)
                    .or_default()
                    .extend_from_slice(&triangle);
            }
            let _ = range_first_vertex;
            for (joint, vertices) in partitions {
                let first_vertex = u32::try_from(out.len())
                    .map_err(|_| "expanded model vertex offset exceeds u32".to_owned())?;
                let vertex_count = u32::try_from(vertices.len())
                    .map_err(|_| "expanded model partition exceeds u32".to_owned())?;
                out.extend_from_slice(&vertices);
                if vertex_count > 0 {
                    draw_ranges.push(AssetDrawRange {
                        mesh_name: Arc::from(mesh.name.as_str()),
                        render_passes: mesh.render_passes,
                        joint_lineage: Arc::from(joint_lineage(model.skeleton.as_ref(), joint)),
                        first_vertex,
                        vertex_count,
                        material_slot,
                        local_center: vertices
                            .iter()
                            .fold(Vec3::ZERO, |sum, v| sum.add(v.position))
                            .mul(1.0 / vertices.len() as f32),
                    });
                }
            }
        }
    }
    Ok((out, draw_ranges))
}

// Skin components can share one mesh/material (head and hands are common).
// Partition by dominant skeletal influence once, at geometry preparation.
pub(in crate::first_scene) fn dominant_triangle_joint(
    triangle: &[AssetTriangleVertex; 3],
    skeleton: Option<&newviso_model::ModelSkeleton>,
) -> Option<u16> {
    let skeleton = skeleton?;
    let mut weights = BTreeMap::<u16, f32>::new();
    for vertex in triangle {
        for influence in 0..usize::from(vertex.skin_influences) {
            let joint = vertex.joints[influence];
            if usize::from(joint) < skeleton.joints.len() {
                *weights.entry(joint).or_default() += vertex.weights[influence];
            }
        }
    }
    weights
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|entry| entry.0)
}

pub(in crate::first_scene) fn joint_lineage(
    skeleton: Option<&newviso_model::ModelSkeleton>,
    joint: Option<u16>,
) -> Vec<String> {
    let Some(skeleton) = skeleton else {
        return Vec::new();
    };
    let mut cursor = joint;
    let mut names = Vec::new();
    while let Some(index) = cursor {
        let Some(joint) = skeleton.joints.get(usize::from(index)) else {
            break;
        };
        names.push(joint.name.clone());
        if names.len() > skeleton.joints.len() {
            break;
        }
        cursor = joint.parent;
    }
    names
}

pub(in crate::first_scene) fn generate_triangle_tangent(triangle: &mut [AssetTriangleVertex; 3]) {
    let edge1 = triangle[1].position.sub(triangle[0].position);
    let edge2 = triangle[2].position.sub(triangle[0].position);
    let duv1 = [
        triangle[1].uv[0] - triangle[0].uv[0],
        triangle[1].uv[1] - triangle[0].uv[1],
    ];
    let duv2 = [
        triangle[2].uv[0] - triangle[0].uv[0],
        triangle[2].uv[1] - triangle[0].uv[1],
    ];
    let det = duv1[0] * duv2[1] - duv1[1] * duv2[0];

    let normal = triangle[0].normal.normalized();
    let (mut tangent, bitangent) = if det.abs() > 1.0e-8 {
        let inv = 1.0 / det;
        (
            Vec3::new(
                (edge1.x * duv2[1] - edge2.x * duv1[1]) * inv,
                (edge1.y * duv2[1] - edge2.y * duv1[1]) * inv,
                (edge1.z * duv2[1] - edge2.z * duv1[1]) * inv,
            ),
            Vec3::new(
                (edge2.x * duv1[0] - edge1.x * duv2[0]) * inv,
                (edge2.y * duv1[0] - edge1.y * duv2[0]) * inv,
                (edge2.z * duv1[0] - edge1.z * duv2[0]) * inv,
            ),
        )
    } else {
        let axis = if normal.y.abs() < 0.999 {
            Vec3::Y
        } else {
            Vec3::new(1.0, 0.0, 0.0)
        };
        let tangent = axis.cross(normal).normalized();
        (tangent, normal.cross(tangent))
    };
    tangent = tangent.sub(normal.mul(tangent.dot(normal))).normalized();
    let handedness = if normal.cross(tangent).dot(bitangent) < 0.0 {
        -1.0
    } else {
        1.0
    };
    for vertex in triangle {
        vertex.tangent = [tangent.x, tangent.y, tangent.z, handedness];
    }
}
