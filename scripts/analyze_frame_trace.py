#!/usr/bin/env python3
"""Summarize NEWVISO_PERF_TRACE key=value runtime log lines.

Usage:
    python scripts/analyze_frame_trace.py my-game.log
    python scripts/analyze_frame_trace.py my-game.log --json output.json

Run the game with NEWVISO_PERF_TRACE=1 and NEWVISO_PERF_TRACE_INTERVAL=1
for unaliased frame-cadence distributions. Values are wall-clock stage
durations; scene_render_wall_ms and render_provider_wall_ms are not GPU timers.
"""
from __future__ import annotations

import argparse
import json
import math
import re
from pathlib import Path
from statistics import fmean, median
from typing import Any

TRACE_PREFIX = "newviso.perf.trace"
RENDER_PREFIX = "newviso.perf.trace.render"
STREAM_PREFIX = "newviso.perf.trace.streaming"
STREAM_TIMINGS = ("asset_pump_ms", "residency_ms", "anim_preload_ms")
RENDER_TIMINGS = (
    "render_total_ms", "visibility_ms", "geometry_sync_ms",
    "material_sync_ms", "geometry_build_ms", "residency_sync_ms",
    "acquire_wall_ms", "skin_upload_wall_ms", "primitive_upload_wall_ms",
    "uploads_wall_ms", "graph_record_wall_ms", "graph_cpu_ms",
)
VALUE_RE = re.compile(r"([a-z][a-z0-9_]+)=(-?(?:\d+(?:\.\d*)?|\.\d+))")
TIMINGS = (
    "dt_ms", "total_ms", "pre_physics_ms", "cold_ms", "physics_ms", "endorphin_ms",
    "physics_build_ms", "physics_provider_ms", "bullet_service_ms", "physics_post_ms",
    "scripts_ms", "world_native_ms", "presentations_ms", "script_state_ms",
    "quickjs_ms", "script_commands_ms", "scene_tick_ms",
    "streaming_ms", "interest_ms", "pump_ms",
    "publish_ms", "scene_render_wall_ms", "render_provider_wall_ms",
)
COUNTERS = (
    "physics_steps", "bodies", "joints", "physics_events",
    "active_ragdolls", "scene_bodies", "queued_assets",
    "loading_assets", "resident_assets", "resident_bytes",
    "failed_assets", "over_asset_budget", "scene_prepare_queued",
)


def normalized_trace_lines(lines: list[str]) -> list[str]:
    """Normalize Chronicle ULOG NDJSON into trace-prefixed lines.

    The default console logger can omit the source name, so prefer
    project .newviso/cache/logs/current.ulog.ndjson for actual traces.
    """
    normalized: list[str] = []
    for line in lines:
        if "newviso.perf.trace" in line and line.lstrip().startswith("{"):
            try:
                entry = json.loads(line)
            except json.JSONDecodeError:
                continue
            source = entry.get("source") or {}
            category = source.get("name")
            if category not in (TRACE_PREFIX, RENDER_PREFIX, STREAM_PREFIX):
                continue
            normalized.append(f"[{category}] {entry.get('message', '')}")
        else:
            normalized.append(line)
    return normalized


def parse_trace(lines: list[str]) -> list[dict[str, float]]:
    frames: list[dict[str, float]] = []
    for line in lines:
        if TRACE_PREFIX not in line or RENDER_PREFIX in line or STREAM_PREFIX in line:
            continue
        data = {key: float(value) for key, value in VALUE_RE.findall(line)}
        if {"frame", "total_ms"}.issubset(data) and all(
            math.isfinite(n) for n in data.values()
        ):
            frames.append(data)
    return frames


def parse_render_trace(lines: list[str]) -> list[dict[str, float]]:
    samples = []
    for line in lines:
        if RENDER_PREFIX not in line:
            continue
        record = {key: float(value) for key, value in VALUE_RE.findall(line)}
        if "render_frame" not in record or "render_total_ms" not in record:
            continue
        if not all(math.isfinite(v) for v in record.values()):
            continue
        samples.append(record)
    return samples


def parse_stream_trace(lines: list[str]) -> list[dict[str, float]]:
    samples: list[dict[str, float]] = []
    for line in lines:
        if STREAM_PREFIX not in line:
            continue
        entry = {key: float(value) for key, value in VALUE_RE.findall(line)}
        if "stream_frame" in entry and "asset_pump_ms" in entry and all(
            math.isfinite(v) for v in entry.values()
        ):
            samples.append(entry)
    return samples


def percentile(values: list[float], q: float) -> float:
    if not values:
        raise ValueError("empty metric series")
    values = sorted(values)
    index = max(0, min(len(values) - 1, math.ceil(q * len(values)) - 1))
    return values[index]


def summarize(frames: list[dict[str, float]]) -> dict[str, Any]:
    if not frames:
        raise ValueError("No newviso.perf.trace samples found")
    result: dict[str, Any] = {"samples": len(frames), "metrics": {}}
    for key in (*TIMINGS, *COUNTERS):
        samples = [f[key] for f in frames if key in f]
        if not samples:
            continue
        result["metrics"][key] = {
            "samples": len(samples),
            "avg": round(fmean(samples), 4),
            "p50": round(median(samples), 4),
            "p95": round(percentile(samples, .95), 4),
            "p99": round(percentile(samples, .99), 4),
            "max": round(max(samples), 4),
        }
    dt_samples = [f["dt_ms"] for f in frames if f.get("dt_ms", 0) > 0]
    if dt_samples:
        # Only a frame-cadence proxy. Not presented-frame or GPU fps.
        result["mean_dt_fps_proxy"] = round(1000.0 / fmean(dt_samples), 2)
    result["frame_sequence"] = {
        "first": int(frames[0]["frame"]), "last": int(frames[-1]["frame"])
    }
    if len(frames) >= 2:
        frame_deltas = [
            b["frame"] - a["frame"] for a, b in zip(frames, frames[1:])
        ]
        result["sample_stride"] = sorted(frame_deltas)[len(frame_deltas) // 2]
    return result


def render_summary(frames: list[dict[str, float]]) -> dict[str, Any]:
    metrics = {}
    for metric in (*RENDER_TIMINGS, *STREAM_TIMINGS, "primitive_upload_bytes", "visible",
                   "instances", "hiz_draws", "graph_passes", "graph_skipped"):
        values = [row[metric] for row in frames if metric in row]
        if values:
            metrics[metric] = {
                "avg": round(fmean(values), 4),
                "p50": round(median(values), 4),
                "p95": round(percentile(values, .95), 4),
                "max": round(max(values), 4),
            }
    return {"samples": len(frames), "metrics": metrics}


def self_test() -> None:
    lines = [
        "[INFO ] [newviso.perf.trace] frame=10 total_ms=20.0 dt_ms=16.7 "
        "physics_ms=8 endorphin_ms=3 scene_render_wall_ms=4 physics_steps=2\n",
        "ignored: other_service frame=10 total_ms=900\n",
        "[INFO ] [newviso.perf.trace] frame=11 total_ms=30.0 dt_ms=18.0 "
        "physics_ms=10 endorphin_ms=5 scene_render_wall_ms=6 physics_steps=3\n",
        "[newviso.perf.trace] frame=12 total_ms=NaN dt_ms=inf\n",
        "[newviso.perf.trace.render] render_frame=11 render_total_ms=8.0 "
        "visibility_ms=1.0 geometry_build_ms=2.0 skin_upload_wall_ms=0.5\n",
    ]
    assert len(parse_render_trace(lines)) == 1
    lines.append("[newviso.perf.trace.streaming] stream_frame=21 asset_pump_ms=1.2 residency_ms=2.3 anim_preload_ms=0.1")
    assert len(parse_stream_trace(lines)) == 1
    chronicle = json.dumps({
        "source": {"kind": "plugin", "name": TRACE_PREFIX},
        "message": "frame=20 total_ms=9 dt_ms=9 pre_physics_ms=1"
    })
    assert parse_trace(normalized_trace_lines([chronicle]))[0]["total_ms"] == 9.0
    assert render_summary(parse_render_trace(lines))["metrics"]["visibility_ms"]["avg"] == 1
    r = summarize(parse_trace(lines))
    assert r["samples"] == 2
    assert r["metrics"]["total_ms"]["avg"] == 25
    assert r["metrics"]["physics_steps"]["p95"] == 3
    assert r["frame_sequence"] == {"first": 10, "last": 11}
    assert r["sample_stride"] == 1
    assert percentile([1, 2, 3, 4], .95) == 4


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("logfile", type=Path, nargs="?")
    parser.add_argument("--json", type=Path, help="write JSON statistics")
    parser.add_argument("--skip-first", type=int, default=0,
                        help="discard initial frames during warmup; use with interval=1")
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        self_test()
        print("trace parser tests passed")
        return
    if args.logfile is None:
        parser.error("logfile required")
    with args.logfile.open(encoding="utf-8", errors="replace") as fh:
        lines = fh.readlines()
    lines = normalized_trace_lines(lines)
    frames = parse_trace(lines)
    render_frames = parse_render_trace(lines)
    stream_samples = parse_stream_trace(lines)
    if args.skip_first < 0:
        parser.error("--skip-first must be >= 0")
    if args.skip_first:
        frames = frames[args.skip_first:]
        render_frames = render_frames[args.skip_first:]
    report = summarize(frames)
    if render_frames:
        report["renderer"] = render_summary(render_frames)
    if stream_samples:
        report["streaming_stages"] = render_summary(stream_samples)
    print(f"Samples: {report['samples']} | frames: {report['frame_sequence']}")
    print("Timings are wall-clock; render stages can include CPU-to-provider waits.")
    for metric in TIMINGS:
        if metric in report["metrics"]:
            v = report["metrics"][metric]
            print(
                f"{metric:28s} mean={v['avg']:9.3f} ms "
                f"p50={v['p50']:9.3f} p95={v['p95']:9.3f} max={v['max']:9.3f}"
            )
    if "mean_dt_fps_proxy" in report:
        print(f"Frame cadence proxy (from dt): {report['mean_dt_fps_proxy']:.2f} FPS")
        if report.get("sample_stride", 1) != 1:
            print("NOTE: sampled frame intervals > 1; enable trace interval 1 for tail statistics")
    if render_frames:
        print(f"Scene renderer samples: {len(render_frames)} (independent scene frame index)")
        for metric in RENDER_TIMINGS:
            if metric in report["renderer"]["metrics"]:
                v = report["renderer"]["metrics"][metric]
                print(f"{metric:28s} mean={v['avg']:9.3f} ms p95={v['p95']:9.3f} max={v['max']:9.3f}")
    if stream_samples:
        print(f"Streaming stage samples: {len(stream_samples)} (slow frames and periodic probes)")
        for metric in STREAM_TIMINGS:
            v = report["streaming_stages"]["metrics"].get(metric)
            if v:
                print(f"{metric:28s} avg={v['avg']:9.3f} ms p95={v['p95']:9.3f} max={v['max']:9.3f}")
    if args.json:
        args.json.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
        print(f"Saved JSON: {args.json}")


if __name__ == "__main__":
    main()
