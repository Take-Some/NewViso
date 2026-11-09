"""Repeated, input-free NewViso scene benchmark. Does not treat platform frames as gameplay frames."""
from __future__ import annotations

import argparse
import json
import math
import os
from pathlib import Path
import re
import subprocess
import sys
import time

from analyze_frame_trace import normalized_trace_lines, parse_trace, parse_render_trace, percentile

ROOT = Path(__file__).resolve().parents[1]
WORKSPACE = ROOT.parent


def statistics(values):
    ordered = sorted(values)
    return {
        "samples": len(ordered),
        "mean_ms": round(sum(ordered) / len(ordered), 4),
        "median_ms": round(percentile(ordered, 0.5), 4),
        "p95_ms": round(percentile(ordered, 0.95), 4),
        "p99_ms": round(percentile(ordered, 0.99), 4),
        "max_ms": round(max(ordered), 4),
        "over_33_33_ms": sum(v > 33.3333 for v in ordered),
        "over_50_ms": sum(v > 50 for v in ordered),
        "over_100_ms": sum(v > 100 for v in ordered),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", default="FirstFPS")
    parser.add_argument("--profile", choices=["debug", "release"], default="release")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--max-frames", type=int, default=1400)
    parser.add_argument("--warmup", type=int, default=180)
    parser.add_argument("--tail", type=int, default=500)
    parser.add_argument("--timeout", type=int, default=125)
    parser.add_argument("--tag", default="baseline")
    opts = parser.parse_args()
    if not 1 <= opts.runs <= 12 or not 100 <= opts.max_frames <= 100000:
        parser.error("invalid --runs or --max-frames")
    if opts.warmup < 0 or opts.tail <= 0:
        parser.error("invalid warmup/tail")
    if not re.fullmatch(r"[a-z0-9_-]{1,40}",opts.tag):
        parser.error("tag must contain only lowercase alphanumeric, dash and underscore")
    project = WORKSPACE / "Projects" / opts.project
    if not (project / "project.json").exists():
        parser.error(f"project does not exist: {project}")
    exe = ROOT / "target" / opts.profile / "newviso.exe"
    if not exe.is_file():
        parser.error(f"binary does not exist: {exe}")
    log = project / ".newviso" / "cache" / "logs" / "current.ulog.ndjson"
    dest = ROOT / "cache" / "profiles" / f"benchmark-{opts.project.lower()}-{opts.profile}-{opts.tag}"
    dest.mkdir(parents=True,exist_ok=True)
    environment = os.environ.copy()
    environment["NEWVISO_PERF_TRACE"] = "1"
    environment["NEWVISO_PERF_TRACE_INTERVAL"] = "1"
    output = {"project":opts.project,"profile":opts.profile,"tag":opts.tag,
              "limits":{"platform_frames":opts.max_frames,"warmup_samples":opts.warmup,
                        "last_samples":opts.tail},
              "runs":[]}
    for trial in range(opts.runs):
        before = log.stat().st_size if log.exists() else 0
        with (dest / f"run-{trial+1}.console.log").open("wb") as fh:
            started = time.monotonic()
            try:
                process = subprocess.run(
                    [str(exe),"--project",str(project),"--max-frames",str(opts.max_frames)],
                    cwd=ROOT,env=environment,stdout=fh,stderr=subprocess.STDOUT,
                    timeout=opts.timeout)
                code = process.returncode
            except subprocess.TimeoutExpired:
                code = "timeout"
            elapsed = round(time.monotonic() - started,3)
        if not log.exists():
            raise RuntimeError("Chronicle log was not created")
        with log.open("rb") as fh:
            fh.seek(min(before,log.stat().st_size))
            events = fh.read()
        (dest/f"run-{trial+1}.ulog.ndjson").write_bytes(events)
        traces = normalized_trace_lines(events.decode("utf-8",errors="replace").splitlines())
        frames = parse_trace(traces)
        tail = frames[opts.warmup:][-opts.tail:]
        report = {"index":trial+1,"exit_code":code,"wall_seconds":elapsed,
                  "recorded_gameplay_samples":len(frames),
                  "used_samples":len(tail),
                  "metrics":{key:statistics([f[key] for f in tail if key in f])
                    for key in ("dt_ms","total_ms","scripts_ms","quickjs_ms",
                                "scene_render_wall_ms","streaming_ms",
                                "physics_ms","endorphin_ms","physics_provider_ms")
                    if any(key in f for f in tail)}}
        if tail:
            report["frame_range"] = [int(tail[0]["frame"]),int(tail[-1]["frame"])]
        report["render_samples"] = len(parse_render_trace(traces))
        output["runs"].append(report)
        (dest/"summary.json").write_text(json.dumps(output,indent=2)+"\n",encoding="utf-8")
        print(json.dumps({"trial":trial+1,"exit":code,"gameplay":len(frames),
                          "used":len(tail),"total":report["metrics"].get("total_ms"),
                          "quickjs":report["metrics"].get("quickjs_ms")}),flush=True)
        if code != 0 or not tail:
            print("Bench aborted: nonzero runtime code or no game frames",file=sys.stderr)
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
