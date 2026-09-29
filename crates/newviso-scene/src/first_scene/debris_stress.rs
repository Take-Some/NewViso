use super::*;
use std::time::Instant;

fn chip(index: usize) -> Cube {
    let gx = (index % 4096) as f32;
    let gz = ((index / 4096) % 4096) as f32;
    let jitter = ((index.wrapping_mul(1103515245).wrapping_add(12345) >> 8) & 1023) as f32 / 1023.0;
    Cube {
        position: Vec3::new(
            gx * 0.06 + jitter * 0.012,
            0.02 + (index % 7) as f32 * 0.001,
            gz * 0.06 - jitter * 0.012,
        ),
        rotation_degrees: Vec3::new(
            (index % 37) as f32 * 3.7,
            (index % 89) as f32 * 2.1,
            (index % 53) as f32 * 4.3,
        ),
        scale: Vec3::new(
            0.025 + (index % 11) as f32 * 0.002,
            0.010 + (index % 5) as f32 * 0.001,
            0.008 + (index % 3) as f32 * 0.001,
        ),
        base_color: [0.56, 0.31, 0.09, 1.0],
    }
}

fn append_compact_record(cube: &Cube, out: &mut Vec<f32>) {
    let rx = cube.rotation_degrees.x.to_radians() * 0.5;
    let ry = cube.rotation_degrees.y.to_radians() * 0.5;
    let rz = cube.rotation_degrees.z.to_radians() * 0.5;
    let (sx, cx) = rx.sin_cos();
    let (sy, cy) = ry.sin_cos();
    let (sz, cz) = rz.sin_cos();
    let qx = sx * cy * cz - cx * sy * sz;
    let qy = cx * sy * cz + sx * cy * sz;
    let qz = cx * cy * sz - sx * sy * cz;
    let qw = cx * cy * cz + sx * sy * sz;

    // 14 floats / 56 bytes:
    // position.xyz + scale.x
    // quaternion.xyzw
    // scale.yz + color.rg
    // color.ba
    out.extend_from_slice(&[
        cube.position.x,
        cube.position.y,
        cube.position.z,
        cube.scale.x,
        qx,
        qy,
        qz,
        qw,
        cube.scale.y,
        cube.scale.z,
        cube.base_color[0],
        cube.base_color[1],
        cube.base_color[2],
        cube.base_color[3],
    ]);
}

#[test]
#[ignore = "manual multi-million debris stress benchmark"]
fn debris_mass_instance_stress() {
    println!(
        "DEBRIS_STRESS contract legacy_bytes_per_chip={} matrix_instance_bytes={} compact_bytes_per_chip={}",
        CUBE_VERTEX_COUNT as usize * FLOATS_PER_VERTEX * std::mem::size_of::<f32>(),
        INSTANCE_FLOATS * std::mem::size_of::<f32>(),
        14 * std::mem::size_of::<f32>(),
    );

    // Exercise the exact current CPU-expanded cube path at bounded sizes.
    // The million-scale legacy payload is projected rather than allocated:
    // 1M ~= 2.28 GiB and 5M ~= 11.40 GiB per main-view frame before shadows.
    for count in [10_000usize, 100_000usize] {
        let started = Instant::now();
        let mut vertices =
            Vec::<f32>::with_capacity(count * CUBE_VERTEX_COUNT as usize * FLOATS_PER_VERTEX);
        for index in 0..count {
            chip(index).append_vertices(&mut vertices);
        }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "DEBRIS_STRESS legacy count={} ms={:.3} floats={} bytes={} mib={:.2}",
            count,
            elapsed,
            vertices.len(),
            vertices.len() * std::mem::size_of::<f32>(),
            vertices.len() as f64 * 4.0 / (1024.0 * 1024.0),
        );
        assert_eq!(
            vertices.len(),
            count * CUBE_VERTEX_COUNT as usize * FLOATS_PER_VERTEX
        );
    }

    // Real multi-million pass through the production MassInstance ABI:
    // one existing 4x4 model matrix per instance, persistent on GPU.
    for count in [1_000_000usize, 5_000_000usize] {
        let started = Instant::now();
        let mut matrices = Vec::<f32>::with_capacity(count * INSTANCE_FLOATS);
        for index in 0..count {
            let cube = chip(index);
            matrices.extend_from_slice(&geometry::instance_model_matrix(
                cube.position,
                cube.rotation_degrees,
                cube.scale,
            ));
        }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "DEBRIS_STRESS production_matrix count={} ms={:.3} bytes={} mib={:.2}",
            count,
            elapsed,
            matrices.len() * std::mem::size_of::<f32>(),
            matrices.len() as f64 * 4.0 / (1024.0 * 1024.0),
        );
        assert_eq!(matrices.len(), count * INSTANCE_FLOATS);
    }

    // Real multi-million pass for the candidate compact representation.
    for count in [1_000_000usize, 5_000_000usize] {
        let started = Instant::now();
        let mut compact = Vec::<f32>::with_capacity(count * 14);
        for index in 0..count {
            let cube = chip(index);
            append_compact_record(&cube, &mut compact);
        }
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        println!(
            "DEBRIS_STRESS compact count={} ms={:.3} floats={} bytes={} mib={:.2}",
            count,
            elapsed,
            compact.len(),
            compact.len() * std::mem::size_of::<f32>(),
            compact.len() as f64 * 4.0 / (1024.0 * 1024.0),
        );
        assert_eq!(compact.len(), count * 14);
    }

    for count in [1_000_000usize, 5_000_000usize] {
        let legacy_bytes = count as u64 * CUBE_VERTEX_COUNT as u64 * FLOATS_PER_VERTEX as u64 * 4;
        let compact_bytes = count as u64 * 14 * 4;
        println!(
            "DEBRIS_STRESS projected count={} legacy_mib={:.2} compact_mib={:.2} reduction_x={:.2}",
            count,
            legacy_bytes as f64 / (1024.0 * 1024.0),
            compact_bytes as f64 / (1024.0 * 1024.0),
            legacy_bytes as f64 / compact_bytes as f64,
        );
    }
}
