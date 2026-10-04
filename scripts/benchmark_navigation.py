"""Measure public navmesh endpoint snapping independently of rendering and path length."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
HARNESS = ROOT / ".newviso" / "benchmarks" / "navigation-snap"

RUST_SOURCE = r'''
use newviso_navigation::{NavigationRuntime, NavTileSource, PathStatus};
use std::{hint::black_box, time::Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let queries: usize = args[1].parse().unwrap();
    let sample_count: usize = args[2].parse().unwrap();
    for count in args[3].split(',') {
        let polygons: usize = count.parse().unwrap();
        let squares = polygons / 2;
        let mut vertices = Vec::with_capacity(squares * 4);
        let mut triangles = Vec::with_capacity(polygons);
        for i in 0..squares {
            let x = (i % 128) as f32 * 12.0;
            let z = (i / 128) as f32 * 12.0;
            let first = vertices.len() as u32;
            vertices.extend([[x, 0.0, z], [x+2.0, 0.0, z],
                [x+2.0, 0.0, z+2.0], [x, 0.0, z+2.0]]);
            triangles.extend([[first, first+2, first+1], [first, first+3, first+2]]);
        }
        let mut nav = NavigationRuntime::default();
        nav.upsert_tile(NavTileSource { id: "benchmark".into(), vertices, triangles }).unwrap();
        let mut samples = Vec::with_capacity(sample_count);
        for _ in 0..sample_count {
            let started = Instant::now();
            for q in 0..queries {
                let i = (q * 31) % squares;
                let point = [(i % 128) as f32 * 12.0 + 0.25, 0.1,
                    (i / 128) as f32 * 12.0 + 0.25];
                let result = black_box(nav.solve_now(black_box(point), black_box(point), 0.25));
                assert_eq!(result.status, PathStatus::Found);
            }
            samples.push(started.elapsed().as_secs_f64() * 1_000_000.0 / queries as f64);
        }
        samples.sort_by(f64::total_cmp);
        println!("snap_bench polygons={} median_us={:.3} min_us={:.3} max_us={:.3}",
            polygons, samples[sample_count/2], samples[0], samples[sample_count-1]);
    }
}
'''


def positive_int(value: str) -> int:
    number = int(value)
    if number < 1:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--polygons", default="2048,8192,32768")
    parser.add_argument("--queries", type=positive_int, default=512)
    parser.add_argument("--samples", type=positive_int, default=5)
    args = parser.parse_args()
    try:
        counts = [int(value) for value in args.polygons.split(",")]
    except ValueError:
        parser.error("--polygons must be comma-separated even integers")
    if not counts or any(count < 2 or count % 2 != 0 or count > 1_000_000 for count in counts):
        parser.error("each polygon count must be even and between 2 and 1000000")
    if args.samples % 2 != 1:
        parser.error("--samples must be odd so the median is an observed sample")

    (HARNESS / "src").mkdir(parents=True, exist_ok=True)
    dependency = json.dumps((ROOT / "crates" / "newviso-navigation").as_posix())
    manifest = (
        '[package]\nname = "newviso-navigation-snap-bench"\nversion = "0.0.0"\n'
        'edition = "2021"\n[workspace]\n[dependencies]\n'
        f'newviso-navigation = {{ path = {dependency} }}\n'
    )
    (HARNESS / "Cargo.toml").write_text(manifest, encoding="utf-8")
    (HARNESS / "src" / "main.rs").write_text(RUST_SOURCE, encoding="utf-8")
    result = subprocess.run(
        ["cargo", "run", "--release", "--offline", "--manifest-path", str(HARNESS / "Cargo.toml"),
         "--target-dir", str(ROOT / "target"), "--", str(args.queries), str(args.samples),
         ",".join(map(str, counts))],
        cwd=ROOT, capture_output=True, text=True,
    )
    if result.returncode:
        sys.stderr.write(result.stderr + result.stdout)
        return result.returncode
    rows = [
        {"polygons": int(count), "median_us": float(median), "min_us": float(low), "max_us": float(high)}
        for count, median, low, high in re.findall(
            r"snap_bench polygons=(\d+) median_us=([\d.]+) min_us=([\d.]+) max_us=([\d.]+)",
            result.stdout,
        )
    ]
    if len(rows) != len(counts):
        sys.stderr.write("Benchmark did not report every requested polygon count.\n" + result.stdout)
        return 1
    print(json.dumps({"metric": "same-polygon solve with two endpoint snaps", "queries_per_sample": args.queries,
                      "samples": args.samples, "results": rows}, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
