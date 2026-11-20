#!/usr/bin/env python3
"""Bounded live NewViso profiling, preserving original project files.

This tool reports *observed* gameplay, render wall time and optional Vulkan
query results. It explicitly refuses to treat startup/loading frames as FPS.
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time

import analyze_frame_trace as trace


def numeric_stats(samples: list[float]) -> dict[str, float | int] | None:
    samples = sorted(x for x in samples if math.isfinite(x))
    if not samples:
        return None
    return {
        "count": len(samples),
        "mean": round(sum(samples) / len(samples), 4),
        "p50": round(trace.percentile(samples, 0.5), 4),
        "p95": round(trace.percentile(samples, 0.95), 4),
        "p99": round(trace.percentile(samples, 0.99), 4),
        "max": round(samples[-1], 4),
    }


def parse_gpu_timestamps(lines: list[str]) -> dict:
    rows = []
    for line in lines:
        if "vulkan gpu shadow timing:" not in line:
            continue
        values = {k: float(v) for k, v in
                  re.findall(r"([a-z_]+)=([0-9.]+)", line)}
        if "frame" in values:
            rows.append(values)
    return {
        "samples": len(rows),
        "phases_ms": {key: numeric_stats([row[key] for row in rows if key in row])
                      for key in ("directional_ms", "local_ms", "shadow_total_ms",
                                  "opaque_ms", "postfx_ms", "ui_ms", "profiled_ms")},
    }


def parse_startup_progress(lines: list[str]) -> dict:
    """Summarize readiness blockers without treating loading frames as FPS."""
    samples = []
    fields = (
        "progress", "interests", "claimed", "resident", "materialized",
        "deps_ready", "deps_total", "queued", "loading", "waiting_deps",
        "failed", "prepare_queue", "prepare_pending", "model_prepare",
        "collision_prepare", "gpu_pending", "gpu_missing",
        "gpu_jobs", "gpu_failed", "ready_streak",
    )
    for line in lines:
        if " progress=" not in line or " interests=" not in line or " ready_streak=" not in line:
            continue
        entries = dict(re.findall(r"\b([a-z_]+)=([0-9]+(?:\.[0-9]+)?|true|false)", line))
        if not all(field in entries for field in fields):
            continue
        sample = {field: float(entries[field]) if field == "progress"
                  else int(entries[field]) for field in fields}
        for field in ("all_claimed", "all_resident", "all_materialized",
                      "cpu_idle", "gpu_idle", "gpu_ready", "anim_ready"):
            sample[field] = entries.get(field) == "true"
        samples.append(sample)
    if not samples:
        return {"samples": 0, "first": None, "last": None, "stalled": None}
    first, last = samples[0], samples[-1]
    return {
        "samples": len(samples), "first": first, "last": last,
        "stalled": {
            "progress_unchanged": len(samples) > 1 and first["progress"] == last["progress"],
            "resident_unchanged": len(samples) > 1 and first["resident"] == last["resident"],
            "queued_remaining": last["queued"] > 0,
            "loading_remaining": last["loading"] > 0,
        },
    }


def validate_benchmark(
    *, process_code: int, timed_out: bool, autostart_confirmed: bool,
    gameplay_ready: bool, steady_samples: int, minimum_samples: int,
    warmup: int, gpu_requested: bool, completed_gpu_frames: int,
) -> list[str]:
    """Fail closed: platform startup/rendering is not evidence of gameplay FPS."""
    issues: list[str] = []
    if not autostart_confirmed:
        issues.append("benchmark autostart not acknowledged by runtime (rebuild NewViso Release)")
    if process_code != 0:
        issues.append(f"process exited with {process_code}")
    if timed_out:
        issues.append("process timeout")
    if steady_samples < minimum_samples:
        issues.append(
            f"only {steady_samples} steady gameplay frames "
            f"(minimum {minimum_samples}, warmup={warmup})"
        )
    if not gameplay_ready:
        issues.append("no confirmed gameplay frame/world advance")
    if gpu_requested and completed_gpu_frames == 0:
        issues.append("requested Vulkan timestamp samples are missing")
    return issues


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--provider-dir", type=Path)
    parser.add_argument("--codecs-dir", type=Path)
    parser.add_argument("--frames", type=int, default=900)
    parser.add_argument("--timeout", type=int, default=90)
    parser.add_argument("--warmup", type=int, default=100)
    parser.add_argument("--gpu-timestamps", action="store_true")
    parser.add_argument("--min-gameplay-samples", type=int, default=120,
                        help="minimum steady-state gameplay frames for a valid benchmark")
    opts = parser.parse_args()
    if (not 1 <= opts.frames <= 100_000 or not 1 <= opts.timeout <= 900
            or opts.warmup < 0 or opts.min_gameplay_samples < 1):
        parser.error("invalid frames, timeout or warmup")
    engine = Path(__file__).resolve().parents[1]
    workspace = engine.parent
    project = opts.project.resolve()
    if not (project / "project.json").is_file():
        parser.error(f"project manifest not found: {project}")
    exe = engine / "target" / "release" / "newviso.exe"
    if not exe.is_file():
        parser.error(f"release runtime missing: {exe}")
    reports = engine / "cache" / "profiles"
    reports.mkdir(parents=True, exist_ok=True)
    name = f"live-{project.name}-{dt.datetime.now():%Y%m%d-%H%M%S}"
    stderr_file = reports / (name + ".stderr.log")
    report_file = reports / (name + ".json")
    provider_dir = opts.provider_dir.resolve() if opts.provider_dir else workspace / "pluginsRuntime"
    codecs_dir = opts.codecs_dir.resolve() if opts.codecs_dir else provider_dir / "codecs"
    env = os.environ.copy()
    env.update({
        "NEWVISO_ASSETS_DIR": str(workspace / "Shared" / "Content"),
        "NEWVISO_CONTENT_DIR": str(project),
        "NEWVISO_PROVIDER_DIR": str(provider_dir),
        "NEWVISO_CODECS_DIR": str(codecs_dir),
        "NEWVISO_PERF_TRACE": "1",
        "NEWVISO_PERF_TRACE_INTERVAL": "1",
        "NEWVISO_BENCHMARK_AUTOSTART": "1",
    })
    if opts.gpu_timestamps:
        env["NEWENGINE_VULKAN_GPU_TIMESTAMPS"] = "1"
        env["NEWENGINE_VULKAN_GPU_TIMESTAMP_INTERVAL_FRAMES"] = "1"
        env["NEWVISO_GPU_INSPECTOR_TRACE"] = "1"
    command = [str(exe), "--base-dir", str(engine), "--project", str(project),
               "--max-frames", str(opts.frames)]
    t0 = time.perf_counter()
    with stderr_file.open("wb") as stderr:
        proc = subprocess.Popen(command, cwd=engine, env=env,
                                stdout=subprocess.DEVNULL, stderr=stderr)
        timed_out = False
        try:
            code = proc.wait(timeout=opts.timeout)
        except subprocess.TimeoutExpired:
            timed_out = True
            proc.terminate()
            try:
                code = proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                proc.kill()
                code = proc.wait(timeout=5)
    raw = stderr_file.read_text(encoding="utf-8", errors="replace").splitlines()
    # Chronicle console output omits the source tag; recognize frame signatures.
    frames = trace.parse_trace([
        "[newviso.perf.trace] " + line for line in raw
        if " dt_ms=" in line and " total_ms=" in line and " frame=" in line
    ])
    renders = trace.parse_render_trace([
        "[newviso.perf.trace.render] " + line for line in raw
        if " render_frame=" in line and " render_total_ms=" in line
    ])
    gpu_samples = trace.parse_gpu_trace(raw)
    gpu_unique = {int(row["gpu_frame"]): row for row in gpu_samples}
    gpu_rows = list(gpu_unique.values())
    inspector_polls = sum(" gpu_stats=" in line for line in raw)
    gpu_last = gpu_rows[-1] if gpu_rows else None
    states = []
    for line in raw:
        if "pre-shutdown runtime state: " in line:
            try:
                states.append(json.loads(line.split("pre-shutdown runtime state: ", 1)[1]))
            except json.JSONDecodeError:
                pass
    state = states[-1] if states else {}
    physics = state.get("physics", {})
    assets = state.get("asset_streaming", {})
    scene = state.get("scene", {})
    world = scene.get("world", {})
    recorded = [line for line in raw if "platform stopped frames=" in line]
    gameplay_ready = bool(frames) and int(world.get("frame", 0) or 0) > 0
    steady_frames = frames[opts.warmup:]
    steady_render = renders[opts.warmup:]
    # Host app_frame and completed GPU frame_index are separate timelines.
    steady_host_ids = {int(row["frame"]) for row in steady_frames}
    steady_gpu = [row for row in gpu_samples if int(row["app_frame"]) in steady_host_ids]
    gpu_steady_summary = trace.summarize_gpu_trace(steady_gpu)
    autostart_confirmed = any("autostart requested: waiting for ordinary map residency" in line for line in raw)
    issues = validate_benchmark(
        process_code=code, timed_out=timed_out,
        autostart_confirmed=autostart_confirmed,
        gameplay_ready=gameplay_ready, steady_samples=len(steady_frames),
        minimum_samples=opts.min_gameplay_samples, warmup=opts.warmup,
        gpu_requested=opts.gpu_timestamps,
        completed_gpu_frames=gpu_steady_summary["unique_completed_gpu_frames"],
    )
    profile_valid = not issues
    result = {
        "schema": "newviso.live_profile.v1",
        "project": str(project),
        "release_binary": str(exe),
        "exit_code": code,
        "timed_out": timed_out,
        "wall_seconds": round(time.perf_counter() - t0, 2),
        "requested_platform_frames": opts.frames,
        "platform_stop": recorded[-1] if recorded else None,
        "map_ready": any("initial map residency ready" in line for line in raw),
        "gameplay_ready": gameplay_ready,
        "world_frame": world.get("frame"),
        "physics_fixed_tick": physics.get("fixed_tick"),
        "physics_bodies": len(physics.get("bodies", [])),
        "vehicles": state.get("vehicles", {}).get("count"),
        "scene_entities": world.get("entities"),
        "streaming": {k: assets.get(k) for k in (
            "queued", "loading", "waiting_dependencies", "resident",
            "resident_bytes", "failed", "over_budget")},
        "frame_samples": len(frames),
        "render_samples": len(renders),
        "warmup_discarded": min(opts.warmup, len(frames)),
        "steady_gameplay_samples": len(steady_frames),
        "requested_min_gameplay_samples": opts.min_gameplay_samples,
        "benchmark_valid": profile_valid,
        "benchmark_issues": issues,
        "benchmark_autostart_confirmed": autostart_confirmed,
        "startup_progress": parse_startup_progress(raw),
        "frame_stats": trace.summarize(steady_frames) if steady_frames else None,
        "render_stats": trace.render_summary(steady_render) if steady_render else None,
        "gpu": parse_gpu_timestamps(raw),
        "gpu_inspector": {
            "attempts": inspector_polls,
            "enabled": bool(gpu_samples),
            "unique_frame_samples": len(gpu_rows),
            "last": gpu_last,
            "phases_ms": {key: numeric_stats([float(row[key]) for row in gpu_rows
                            if key in row])
                          for key in ("frame_span_ms", "profiled_ms", "shadow_ms",
                                      "opaque_ms", "postfx_ms", "ui_ms")},
            "steady_state": gpu_steady_summary,
            "frame_id_note": "app_frame is inspection time; gpu_frame is completed Vulkan query",
        },
        "errors": [line[:500] for line in raw
            if any(tag in line.lower() for tag in ("[error", "runtime application failed", "panicked at"))][:12],
        "log_file": str(stderr_file),
    }
    report_file.write_text(json.dumps(result, indent=2, ensure_ascii=False), encoding="utf-8")
    keys = ("exit_code", "timed_out", "wall_seconds", "map_ready", "gameplay_ready",
            "benchmark_valid", "benchmark_issues", "steady_gameplay_samples",
            "world_frame", "physics_fixed_tick", "physics_bodies", "vehicles",
            "scene_entities", "streaming", "frame_samples", "render_samples", "gpu", "gpu_inspector")
    print(json.dumps({**{key: result[key] for key in keys}, "report": str(report_file)},
                     ensure_ascii=False))
    # A successfully exited platform is not proof of a successful benchmark.
    return 0 if profile_valid else 2


if __name__ == "__main__":
    sys.exit(main())
