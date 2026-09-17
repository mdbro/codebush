# Codetree — independent critic, round 4 of 4

**Final verdict: FAIL — 8.4/10 overall (8.43 unrounded).** The visual product meets a professional standard in the reviewed states, and no rendering defects were observed. The requested smooth 8K/120 fps behavior remains unqualified: every final repository benchmark fails its 8.333 ms p99 deadline gate, and the native trace also contains substantial frame-interval spikes. This is the fourth and final review; no fifth iteration is implied.

| Criterion | Score / 10 | Assessment |
|---|---:|---|
| Visual/design quality | 8.6 | Coherent hierarchy, readable source at reading zoom, restrained color, useful tree and minimap. Overview file boundaries remain visible; captions no longer collide in the inspected states. |
| Source fidelity | 8.8 | Full included source is represented, including long files and malformed-byte fixtures. Tests change both map and navigation. File records for all three upstream repositories are unchanged from the previously checked round-three manifests. |
| Navigation/usability | 8.6 | Native search, result traversal, reading, test filtering, reload and folder selection work. Broad overview search now stays responsive in the measured UI phase. Loading and replacement pauses remain material. |
| Rendering correctness | 8.9 | No observed text, geometry, clipping, cache-transition or highlight corruption in the final captures. All four offscreen reports and the native trace record zero GPU validation errors. |
| Performance/scale | 7.0 | Impressive scale and short GPU execution times, within the memory budget. Completion tails and native frame intervals miss the central 120 fps requirement; large reloads are slow. |
| Engineering/reliability | 8.7 | Frozen provenance, source-contract tests, real repositories, native automation and separated CPU/GPU/completion measurements provide credible evidence. Replacement still blocks the window thread, and physical 8K/120 presentation is unverified. |

Scores use equal weighting. The rendering criterion passes for the inspected evidence; it does not cancel the failed performance requirement.

## Evidence boundary

Reviewed frozen source, tests and scripts in `critic-round-4-corrected`, executable SHA256 `691b5acb60a60bf5211733e4c1633b2f33188198ad462f6635782e25b9107568`. Final runtime evidence is exclusively `round-4-corrected` and `native-round-4-corrected`. Earlier round-four runs remain diagnostic history and are not substituted for these results.

I inspected all 36 final offscreen capture states: eight per repository through the contact sheets, plus each separate overview-search image; selected source/search images were also inspected individually. PNG headers confirm all 36 originals are 7680×4320. I inspected all seven corrected native screenshots. The video reference had been examined at multiple timeline frames in the earlier review. The final implementation remains a 2D source atlas.

The supplied test log records four passing source-contract tests. Source inspection confirms the culling regression compares indexed results, including paint order, with linear results over 1,152 viewport/test-mode combinations. I did not run competing GPU work or modify the implementation.

## Final scale and timing results

Each repository ran 3,600 measured frames after 120 warmup frames, paced at 120 Hz with two frames in flight. Source search includes both overview and focused reading. I independently counted 180 measured frames for each search-state/test-mode combination per repository. All runs used the RTX 5090 through DX12 and recorded zero rendering validation errors.

| Repository | Source files / lines | Ready | Resident GB¹ | GPU execution p99 | Completion latency p99 | Scheduled deadline p99 | Deadlines >8.333 ms |
|---|---:|---:|---:|---:|---:|---:|---:|
| Codetree | 16 / 5,537 | 0.89 s | 1.63 | 1.83 ms | 16.66 ms | 16.66 ms | 144 / 3,600 |
| ripgrep | 116 / 57,293 | 1.51 s | 15.35 | 1.70 ms | 21.09 ms | 21.18 ms | 226 / 3,600 |
| llama.cpp | 2,709 / 934,592 | 6.74 s | 24.68 | 2.06 ms | 24.41 ms | 26.43 ms | 345 / 3,600 |
| rust-lang/rust | 38,322 / 4,417,433 | 48.96 s | 29.14 | 2.10 ms | 30.23 ms | 31.93 ms | 519 / 3,600 |

¹ Decimal GB of reported application residency. Native whole-card sampling peaked at 29,230 MiB; no budget breach was observed.

Mean measured cadence is approximately 120.02–120.03 fps. This is throughput, not proof that frames finish within each 8.333 ms interval. The completion measurement includes submission/queue/callback latency; it is distinct from GPU timestamp execution and from physical presentation. Rust GPU execution never exceeded 5.27 ms in this run, yet completion latency reached 53.94 ms. Rust's reading-profile deadline p99 is 36.98 ms; overview search is 31.56 ms. Short shaders alone do not establish smooth delivery.

## Native workflow and the final correction

The corrected native workflow exited successfully: Rust overview, a broad `return` search with 54,736 matches, result traversal, reading, test exclusion to 6,413 files, reload, then actual replacement with the three-file encoding fixture. The fixture visibly retains `\xFF` and `\u{0}` representations. Tree, source, minimap, counts and test state agree in the reviewed images.

The final overview-search correction is effective. With all 54,736 results present, native overview-search UI work is **2.18 ms median / 3.14 ms p99** over 288 frames, compared with roughly 15 ms median in the preceding diagnostic overview-search segment. Focused search UI p99 is 1.48 ms over 72 frames. Matching files receive a subtle overview tint; exact matching lines remain highlighted at reading zoom. Text remains underneath the tint. Source inspection also confirms worker-side result conversion/disposal and stale-query suppression.

The final trace contains 7,891 successful present calls, no skipped surface frames, and no recorded rendering errors. It still does not demonstrate fixed-rate delivery:

| Native Rust state | Sample frames | Frame-interval p99 | UI p99 | Surface-acquire p99 |
|---|---:|---:|---:|---:|
| Overview | 1,751 | 19.69 ms | 5.91 ms | 16.41 ms |
| Overview search, completed 54,736-match result | 288 | 14.49 ms | 3.14 ms | 7.59 ms |
| Focused search | 72 | 12.31 ms | 1.48 ms | 9.91 ms |
| Reading | 138 | 13.18 ms | 1.64 ms | 11.76 ms |

These are CPU scheduling and API-call observations at a 1898×1224 client size on the available 4K/60 Hz display. They neither measure GPU completion nor certify physical 8K/120 Hz presentation. The short focused samples should not be generalized into a sustained performance guarantee.

## Ranked remaining issues

1. **Acceptance blocker: consistent 120 fps delivery remains unmet.** All four 8K deadline gates fail, including the small repository. Native surface acquisition and frame scheduling also have tails above budget. Any future performance work should trace queue/completion and presentation behavior together, rather than infer success from mean fps or GPU execution. Validation would require sustained per-frame timing through all existing interaction states, retained deadline misses, and an actual 8K/120 presentation measurement before making that display claim. The current artifact should be described as a 120 fps target, not a proven locked 120 fps result.

2. **Long preparation and UI-blocking replacement prevent an “instant” large-codebase experience.** Final native Rust readiness was 50.27 s initially and 61.17 s on reload. The trace records an 8.827 s Rust replacement tick and a 10.086 s tick when replacing Rust with the tiny fixture; the fixture's total readiness was 10.15 s despite only about 80 ms of CPU preparation. `src/main.rs:941` performs device creation, old-device waiting/destruction, upload and cache creation inside `tick`. If this is continued later, move or stage that transition so the window remains responsive while preserving the measured memory budget. Validate the longest UI-thread pause separately from total load time; current functional success does not imply responsive replacement.

No additional rendering defect warrants a new corrective round. Source-only indexing, the tests toggle, resident source, the native workflow and large real-codebase coverage are substantively delivered. The remaining performance failures must stay explicit in the handoff after the four-round limit.
