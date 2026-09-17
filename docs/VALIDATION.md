# Workstation validation

The viewer is implemented and runnable. **8K rendering is verified; consistent 8K/120 completion latency has not passed qualification.** The fourth and final independent critic scored **8.4/10**, below the requested 8.5 threshold; see [the complete review](CRITIC.md). An average frame rate or a 120fps counter is not a substitute for the frame-time distribution below. The attached desktop is 3840 × 2160 at approximately 60Hz, so these tests do not establish physical 8K/120 presentation.

## Reproduction

```bash
cargo test --all-targets
./scripts/build.sh
./scripts/benchmark-suite.sh /mnt/d/Codetree/evidence/new-run
```

The suite runs the same Windows executable on this project, ripgrep, llama.cpp and rust-lang/rust. It renders true 7680 × 4320 images on the RTX 5090, records an exact source manifest, and exercises five profiles: overview pan, animated zoom, the cached-image to glyph transition, source reading and panning, and a broad source search. The search profile covers both the overview and a focused result. Test inclusion switches every 15 frames and reverses between successive profile cycles, so each search substate has 180 measured frames per test mode. Both complete scenes stay resident. The shader and interface are identical to the native application.

The paced workload requests 120 frame starts per second with two frames in flight. Completion callbacks record CPU-start-to-GPU-completion latency; GPU timestamps are read in one batch after rendering. `deadline_ms` adds late starts to completion latency, so scheduling delays are retained. `measured_cadence_fps` counts completed frames over elapsed time. GPU execution timestamps, CPU interface time, CPU submission time, source counts, allocation totals and every late frame are retained in `benchmark.json`. Percentiles below use sorted sample index `floor((n - 1) × percentile)`, matching the executable and critic report. The pass flag requires p99 deadline latency ≤ 8.333ms and no recorded GPU validation errors. This is an offscreen completion test, not a monitor presentation test.

For diagnosis, `--pipeline-depth 1` performs a fence and timestamp readback after every frame; `--benchmark-hz 0` measures an unbounded submission workload. Those modes are retained, including failed results.

## Real repositories

The initial clones and revisions are recorded in `D:\Codetree\evidence\repositories.json`. No upstream source was changed. Build outputs, ignored paths, dependency directories and non-source extensions are excluded by the documented scanner rules; source in hidden directories is included. The source manifest is the exact inclusion boundary for each run.

| Repository | Revision |
|---|---|
| ripgrep | `3fce3b5bb0236da2df6d99672afb8a719642eca7` |
| llama.cpp | `df03399b885831b2a1603b3abb0d8c156808e363` |
| rust-lang/rust | `28e8a8c81bf3b37909edac6c2a76e56f30cd492f` |

## Final measured results (round 4)

The same release executable produced all four results below. Each run rendered 3,600 measured frames after 120 warmup frames, at true 8K on the RTX 5090 through DirectX 12. The table distinguishes completed-frame cadence from individual completion deadlines. All four runs had **zero GPU validation errors**, but **all four failed the p99 deadline gate**.

| Repository | Files | Lines | Ready s | Allocated GB | Cadence fps | Completion p99 ms | Deadline p99 ms | GPU p99 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| codetree | 16 | 5,537 | 0.89 | 1.63 | 120.03 | 16.66 | 16.66 | 1.83 |
| ripgrep | 116 | 57,293 | 1.51 | 15.35 | 120.03 | 21.09 | 21.18 | 1.70 |
| llama.cpp | 2,709 | 934,592 | 6.74 | 24.68 | 120.02 | 24.41 | 26.43 | 2.06 |
| rust-lang/rust | 38,322 | 4,417,433 | 48.96 | 29.14 | 120.02 | 30.23 | 31.93 | 2.10 |

Every individual GPU-execution profile p99 is below 3ms. Rust's largest per-profile CPU interface p99 is 3.22ms during overview search; focused search is 1.69ms. The completion delays remain unresolved; GPU timestamps alone cannot establish their cause or demonstrate stable live presentation. Spatial indexing and bounded search drawing reduce some CPU costs, but the results do not show a universal CPU improvement over round 3.

The final native workflow exercised a real Rust search with 54,736 results, overview highlights, source reading, tests on/off, fit, reload, and opening a different folder. Disabling tests leaves 6,413 source files and approximately 2.81 million lines. Initial load took 50.27s and reload 61.17s over WSL's network-style filesystem path. Both retained 29.14GB of source/font/cache allocations. The entire card peaked at **29,230MiB (28.54GiB)**, including driver and desktop allocations, below its 32GiB capacity. Replacing Rust with the three-file encoding fixture took 10.15s, including disposal of the old CPU/GPU data. The final client-area screenshot visibly shows the replacement repository. These load and replacement operations are not instant.

Native timing was recorded at a 1898 × 1224 client size on the attached 4K/60Hz desktop. It measures CPU frame starts, interface construction, surface acquisition, submission and present calls, not physical display or GPU completion. The workflow recorded zero GPU errors and zero skipped surface frames.

| Rust native state | Measured frames | UI p99 ms | Surface acquire p99 ms | Frame interval p99 ms |
|---|---:|---:|---:|---:|
| Overview | 1,751 | 5.91 | 16.41 | 19.69 |
| Overview search | 328 | 3.14 | 7.59 | 15.32 |
| Focused search | 72 | 1.48 | 9.91 | 12.31 |
| Reading | 138 | 1.64 | 11.76 | 13.18 |

The overview-search row includes typing and pending-result frames. The critic separately measures the 288 frames after all 54,736 matches arrive: UI p99 remains 3.14ms, with frame-interval p99 14.49ms.

An earlier fourth-round native run exposed expensive per-line highlights at overview scale and result disposal on the window thread. The final build highlights matching files at overview scale, keeps exact line highlights at reading scale, and moves result preparation/disposal to workers. Native overview-search UI p99 fell from 21.72ms to 3.14ms. Surface-acquisition stalls remain. During background reload, frame-interval p99 is 54.77ms; replacing the GPU scene blocks for roughly 9–10 seconds. The short focused-state samples and the physical display limit preclude a sustained 8K/120 presentation claim.

The replacement path drops the old graphics device and its allocator before allocating the next repository. Ready reports follow actual view replacement and disposal of the old CPU model; the automated check also waits for the matching window title.

## Earlier diagnostic runs

Round 2 used 18,000 measured frames per repository and successfully loaded all declared files with zero GPU validation errors, but failed the frame-budget gate. Its Rust cache-to-glyph profile had 16.35ms GPU p99. Tiled cache images now use several textures within the configured allocation budget, with filter borders to prevent seams. The corresponding round-3 profile is 2.31ms GPU p99. This repaired the GPU overload; completion delays still fail independently.

Serial fence/readback probes, paced asynchronous probes, earlier cache sizes, native input failures and their repaired reruns remain in the evidence directory. Historical runs use older inclusion rules and do not supersede the current manifests.

An explicit encoding fixture verifies that an invalid UTF-8 byte is displayed as `\xFF`, an embedded NUL is represented, its warning is written to the native load report, and Markdown stays excluded. All four integration tests pass. They cover source inclusion, test classification and inline Rust tests, full line retention and non-overlapping layout, encoding preservation, and cursor-anchored zoom. The layout test also compares indexed culling against brute-force culling across 1,152 viewports/test modes, including paint order.

## Review history and artifacts

- Critic round 1: **6.6/10 — fail**. Fixed the large loader stack overflow, overview boundaries and caption collisions, reload caches, search work, and native diagnostics.
- Critic round 2: **8.0/10 — fail**. Identified the large crossover cost, completion stalls, exact `tests.rs` names, and an outdated backend badge. The filename and badge errors are fixed. Round 3 repaired the measured GPU overload but still fails completion deadlines.
- Critic round 3: **8.4/10 — fail**. Visuals, source fidelity and rendering correctness exceed 8.5; unresolved completion delays and remaining repeated CPU work prevent acceptance. Round 4 adds spatial indexes, visible-file search highlighting, cached minimap geometry and tighter native timer scheduling.
- Critic round 4: **8.4/10 — fail** (8.43 unrounded). Visual/design 8.6, source fidelity 8.8, navigation 8.6, rendering correctness 8.9, performance/scale 7.0, engineering/reliability 8.7. The final overview-search fix is effective and no rendering defects were observed. Consistent frame delivery and blocking repository replacement remain unresolved. Work stops at the requested four-round limit; [the ranked remaining issues](CRITIC.md#ranked-remaining-issues) are preserved for a future continuation.
- A 3,600-frame timing probe exceeded the per-query-set timestamp limit. It is rejected and preserved under `evidence/round-3`; subsequent instrumentation splits query sets at the documented limit.

All external evidence is under `D:\Codetree\evidence` (`/mnt/d/Codetree/evidence` in WSL): executable and source hashes, complete JSON reports, 8K screenshots, native client-area screenshots, test logs, diagnostic failures and the independent critic reports. The reference video was inspected at 0, 1, 6, 13, 20, 30 and 40 seconds; extracted frames are in the project's `reference` directory.

The final four-repository suite is `round-4-corrected`, the final native workflow is `native-round-4-corrected`, and its frozen source, test log and independent review are in `critic-round-4-corrected`. Nine 8K states per repository and seven native states were visually inspected; no rendering defects were observed. Earlier fourth-round directories remain diagnostic history. The executable SHA-256 is `691b5acb60a60bf5211733e4c1633b2f33188198ad462f6635782e25b9107568`; the installed executable, tested source, scripts and dependency lockfile match the frozen build. Final handoff documentation is updated separately from that frozen source snapshot.
