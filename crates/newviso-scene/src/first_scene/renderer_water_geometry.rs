use super::FLOATS_PER_VERTEX;
use std::collections::HashMap;

/// Reuse only bit-identical corners. Triangle order, winding, seams, vertex
/// colours, tangents and the near/far water mode are preserved exactly.
pub(super) fn index_water_vertices(vertices: &[f32]) -> Result<(Vec<f32>, Vec<u32>), String> {
    if vertices.len() % FLOATS_PER_VERTEX != 0 {
        return Err("water vertex input is not a complete packed vertex".to_owned());
    }
    let corners = vertices.len() / FLOATS_PER_VERTEX;
    let mut unique = Vec::with_capacity(vertices.len().min(65536 * FLOATS_PER_VERTEX));
    let mut indices = Vec::with_capacity(corners);
    let mut lookup = HashMap::<[u32; FLOATS_PER_VERTEX], u32>::new();
    for vertex in vertices.chunks_exact(FLOATS_PER_VERTEX) {
        let key = std::array::from_fn(|channel| vertex[channel].to_bits());
        let index = if let Some(index) = lookup.get(&key) {
            *index
        } else {
            let index = u32::try_from(unique.len() / FLOATS_PER_VERTEX)
                .map_err(|_| "water vertex index exceeds u32".to_owned())?;
            unique.extend_from_slice(vertex);
            lookup.insert(key, index);
            index
        };
        indices.push(index);
    }
    Ok((unique, indices))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(x: f32, z: f32) -> [f32; FLOATS_PER_VERTEX] {
        let mut out = [0.0; FLOATS_PER_VERTEX];
        out[0] = x;
        out[2] = z;
        out[3] = -1.0;
        out[5] = 1.0;
        out[7..11].fill(1.0);
        out[11] = x;
        out[12] = z;
        out[13] = 1.0;
        out[16] = 1.0;
        out
    }

    #[test]
    fn full_resolution_grid_keeps_every_triangle_and_reuses_shared_corners() {
        let mut expanded = Vec::new();
        for z in 0..255 {
            for x in 0..255 {
                let a = vertex(x as f32, z as f32);
                let b = vertex((x + 1) as f32, z as f32);
                let c = vertex(x as f32, (z + 1) as f32);
                let d = vertex((x + 1) as f32, (z + 1) as f32);
                for corner in [a, c, b, b, c, d] {
                    expanded.extend_from_slice(&corner);
                }
            }
        }
        let (unique, indices) = index_water_vertices(&expanded).unwrap();
        assert_eq!(indices.len(), 255 * 255 * 6);
        assert_eq!(unique.len() / FLOATS_PER_VERTEX, 256 * 256);
        for (corner, index) in indices.iter().enumerate() {
            let source = &expanded[corner * FLOATS_PER_VERTEX..(corner + 1) * FLOATS_PER_VERTEX];
            let start = *index as usize * FLOATS_PER_VERTEX;
            let reconstructed = &unique[start..start + FLOATS_PER_VERTEX];
            assert!(source
                .iter()
                .zip(reconstructed)
                .all(|(a, b)| a.to_bits() == b.to_bits()));
        }
    }

    #[test]
    fn mode_uv_colour_tangent_and_float_bits_remain_distinct() {
        let original = vertex(0.0, 0.0);
        let mut expanded = original.to_vec();
        expanded.extend_from_slice(&original);
        for (channel, value) in [
            (3, 0.0),
            (7, 0.25),
            (11, 0.5),
            (16, -1.0),
            (0, -0.0),
            (1, f32::from_bits(0x7fc00001)),
        ] {
            let mut seam = original;
            seam[channel] = value;
            expanded.extend_from_slice(&seam);
        }
        let (unique, indices) = index_water_vertices(&expanded).unwrap();
        assert_eq!(indices[0], indices[1]);
        assert_eq!(unique.len() / FLOATS_PER_VERTEX, 7);
        for (corner, index) in indices.iter().enumerate() {
            for channel in 0..FLOATS_PER_VERTEX {
                assert_eq!(
                    expanded[corner * FLOATS_PER_VERTEX + channel].to_bits(),
                    unique[*index as usize * FLOATS_PER_VERTEX + channel].to_bits()
                );
            }
        }
        assert!(index_water_vertices(&expanded[..expanded.len() - 1]).is_err());
    }
}
