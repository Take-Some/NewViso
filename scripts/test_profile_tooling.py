#!/usr/bin/env python3
"""Offline regression tests for truthful NewViso CPU/GPU benchmark reporting."""
from __future__ import annotations

import unittest

import analyze_frame_trace as trace
import profile_live_runtime as live


class ProfilerQualityTests(unittest.TestCase):
    def test_empty_gameplay_never_reports_success(self) -> None:
        issues = live.validate_benchmark(
            process_code=0, timed_out=False, autostart_confirmed=True,
            gameplay_ready=False, steady_samples=0, minimum_samples=120,
            warmup=30, gpu_requested=False, completed_gpu_frames=0,
        )
        self.assertTrue(any("steady gameplay" in issue for issue in issues))
        self.assertTrue(any("no confirmed gameplay" in issue for issue in issues))

    def test_cpu_only_run_can_be_valid_with_real_gameplay(self) -> None:
        self.assertEqual(live.validate_benchmark(
            process_code=0, timed_out=False, autostart_confirmed=True,
            gameplay_ready=True, steady_samples=500, minimum_samples=120,
            warmup=30, gpu_requested=False, completed_gpu_frames=0,
        ), [])

    def test_requested_gpu_queries_are_mandatory(self) -> None:
        issues = live.validate_benchmark(
            process_code=0, timed_out=False, autostart_confirmed=True,
            gameplay_ready=True, steady_samples=500, minimum_samples=120,
            warmup=30, gpu_requested=True, completed_gpu_frames=0,
        )
        self.assertTrue(any("Vulkan timestamp" in issue for issue in issues))

    def test_startup_progress_keeps_actual_gates_and_backlog(self) -> None:
        log = ("[INFO] progress=0.8200 interests=100 claimed=100 resident=49 "
               "materialized=47 deps_ready=8 deps_total=10 queued=120 loading=8 "
               "waiting_deps=3 failed=0 prepare_queue=0 prepare_pending=2 "
               "model_prepare=0 collision_prepare=0 gpu_textures=2/3 "
               "gpu_materials=3/4 gpu_pending=1 gpu_missing=1 "
               "gpu_jobs=0 gpu_failed=0 all_claimed=true all_resident=false "
               "all_materialized=false cpu_idle=false gpu_idle=true "
               "gpu_ready=false anim_ready=true ready_streak=0")
        result = live.parse_startup_progress([log])
        self.assertEqual(result["samples"], 1)
        self.assertEqual(result["last"]["queued"], 120)
        self.assertEqual(result["last"]["resident"], 49)
        self.assertFalse(result["last"]["all_resident"])
        self.assertTrue(result["last"]["gpu_idle"])

    def test_gpu_query_frame_is_not_relabelled_as_host_frame(self) -> None:
        lines = [
            '[newviso.perf.trace.gpu] app_frame=41 gpu_stats={"enabled":true,"frame_index":36,"profiled_ms":3.3}',
            '[newviso.perf.trace.gpu] app_frame=51 gpu_stats={"enabled":true,"frame_index":36,"profiled_ms":3.3}',
            '[newviso.perf.trace.gpu] app_frame=61 gpu_stats={"enabled":true,"frame_index":56,"profiled_ms":7.0}',
        ]
        rows = trace.parse_gpu_trace(lines)
        self.assertEqual(int(rows[0]["app_frame"]), 41)
        self.assertEqual(int(rows[0]["gpu_frame"]), 36)
        summary = trace.summarize_gpu_trace(rows)
        self.assertEqual(summary["inspector_polls"], 3)
        self.assertEqual(summary["unique_completed_gpu_frames"], 2)
        self.assertEqual(summary["metrics"]["profiled_ms"]["p99"], 7.0)


if __name__ == "__main__":
    unittest.main()
