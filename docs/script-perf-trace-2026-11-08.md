# NorthStar Script Profiling (2026-11-08)

## Status

The installed provider DLL is `typeScriptScriptEngine-0.4.1-release.dll`, and `scripts.defaults.xml` selects `engine.scripting.typescript`. The provider source uses Oxc and V8 and currently **rejects ES imports**. A running process has not been checked for its actually loaded module.

## Opt-in trace

Set `NEWVISO_PERF_TRACE=1`. `newviso.perf.trace.script.stages` reports microsecond stage totals: `host_encode_us`, `host_gateway_us`, `host_decode_us`, `provider_roundtrip_us`, `v8_input_bridge_us`, `v8_javascript_us` and `v8_output_bridge_us`. Existing `newviso.perf.trace` reports `script_commands_ms`, the native command dispatch stage. Existing `newviso.perf.trace.script` reports delta generation, overall provider time and response merge. Wall-time stages are nested and must not be added or subtracted as if they were disjoint CPU work.

## Verification

- `cargo check --offline -p newviso-script-client`: passed.
- `cargo check --offline -p newviso-scripting`: passed.
- TypeScript provider `cargo test --offline -p engine-scripting-typescript --lib`: blocked by missing workspace dependency `NewEngine/neocore2/crates/newengine-plugin-kit/Cargo.toml`.
- No post-change runtime benchmark has been taken. The proposed p95 budgets of 2 ms (60 FPS) and 1 ms (120 FPS) remain unverified.

## Next acceptance gates

1. Restore the exact source-owned provider dependencies, build and run tests.
2. Check the actual DLL loaded by a live process, and test an imported `.ysc` fixture; the inspected source rejects imports.
3. Run fixed-seed release workloads with warmup and report p50/p95/p99, trace-overhead A/B, NPC-count scaling and error rates.
4. Only then implement tested event batching, binary command batches, scheduler budgets, and opt-in REDengine class/RTTI adapters.
