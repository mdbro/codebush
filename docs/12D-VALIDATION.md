# 12D validation

This report describes the frozen fourth critic candidate. The installed application includes subsequent user-requested [view fixes](VIEW-FIXES.md), verified separately. The performance measurements below remain tied to the frozen executable.

The fourth candidate is frozen as executable SHA-256 `1706e3451508c0648e38c73f912446dbae19533d7df36833dc3ff07b321ed9c3`. **Final result: 8.4/10 — FAIL after four critic rounds.** The 8.5 quality gate and sustained 120fps requirement are not met. The final 8K and native suites completed with zero GPU validation errors and no rendering corruption observed across 64 inspected captures. See the [independent final review](12D-CRITIC.md).

The default renderer implements the user-approved 66-plane reference partition and proper orthogonal 12D rotations. Each measured repository run covers 7,920 frames after 120 warmup frames, reaching all 66 planes with tests both off and on. Overview, source reading, and cache/glyph transitions are exercised. Frame cadence is paced at 120Hz with two frames in flight. GPU execution, CPU/UI work, completion latency, and start-deadline misses are reported separately. The pass gate is deadline p99 ≤ 8.333ms plus zero GPU validation errors.

Candidate/evidence paths on this workstation:

- `D:\Codetree\evidence\12d-frozen-4`: exact executable, source, assets, and hashes.
- `D:\Codetree\evidence\12d-round-4`: all four 8K repository runs.
- `D:\Codetree\evidence\12d-native-4`: final mouse/keyboard and reload evidence.
- `D:\Codetree\evidence\12d-critic-4.md`: final independent review.

## Final 8K benchmark suite

All four runs completed on the NVIDIA GeForce RTX 5090, using native Windows DX12 and 7680 × 4320 render targets. There are **31,680 measured frames**, all 66 rotating planes in both test modes per repository, **zero GPU validation errors**, and 47 inspected captures. All four runs **fail** the steady 120fps deadline gate.

| Codebase | Source files | Lines | Resolved links | Resident GB | Ready seconds | GPU p99 ms | Completion p99 ms | Deadline p99 ms |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| codetree | 24 | 10,321 | 58 | 3.06 | 1.01 | 0.802 | 11.695 | 13.338 |
| ripgrep | 116 | 57,293 | 240 | 15.35 | 1.67 | 0.791 | 9.653 | 11.734 |
| llama | 2,709 | 934,592 | 4,575 | 24.68 | 7.50 | 0.726 | 10.366 | 15.194 |
| rust | 38,322 | 4,417,433 | 38,458 | 29.16 | 51.48 | 1.949 | 18.000 | 18.573 |

Resident GB is the application's decimal allocation estimate for persistent assets, with a further 512MiB reserved by the budget check. It is not a measurement of total driver/desktop VRAM. Ready time includes indexing, source layout, reference resolution, and GPU preparation; it is a single observed load, not a cold-load distribution. The largest repository does not load instantly.

Every run averaged about 120 frames/s because starts were paced at 120Hz. That average does not establish smooth deadlines. The measured deadline misses remain in the JSON, including outliers; none were discarded. The largest graph's projection p99 is 0.137ms and GPU execution p99 is 1.949ms, but GPU execution excludes command-queue and OS scheduling delays. The evidence does not isolate the remaining latency to one cause. The current desktop is 4K/60; physical 8K/120 presentation is unverified.

The frozen open-source revisions are ripgrep `3fce3b5bb0236da2df6d99672afb8a719642eca7`, llama.cpp `df03399b885831b2a1603b3abb0d8c156808e363`, and rust-lang/rust `28e8a8c81bf3b37909edac6c2a76e56f30cd492f`. Each checkout was clean when provenance was recorded in `D:\Codetree\evidence\repositories.json`.

## Correctness and coverage

All 11 source/reference/math contract tests pass. The rotation tests exhaust 4,356 ordered coordinate-plane transitions at multiple intermediate times, determinant, orthogonality, distance preservation, endpoint continuity, and interrupted turns. Reference fixtures verify direction and source-line evidence, false-link rejection, parent ownership, partial Rust parse recovery, Cargo custom test roots, and visibility/test masks.

The four critic rounds scored **7.2, 8.0, 8.1, and 8.4**, all FAIL. No fifth iteration was performed. Their frozen artifacts remain under `12d-frozen-1` through `12d-frozen-3`; failed development probes are retained separately rather than included in passing-run evidence.

## Native interaction and reload

The Windows run completed **13,893 frames** at a 2,178 × 1,444 client resolution, with **zero GPU validation errors** and 17 screenshots. This is native application evidence on the attached 4K/60 desktop, not an 8K presentation measurement.

- The clicked midpoint remained paused for **525 command-scoped frames**. The dragged orientation remained paused for **195 frames**. The assertions bind each hold to its own command sequence, plane, and rotation parameter.
- Shift-click visited exactly both visible overlap candidates, IDs **277 and 452**. O listed both candidates. Following reference evidence selected the expected file, ID **3287**.
- Rust reloaded successfully with full source residency, then the native folder picker opened ripgrep successfully. The window rendered **576 loading frames** during replacement.
- Overall frame-interval p99 was **13.775ms**, tour-only p99 **11.028ms**, and the maximum interval **133.556ms**. Loading interval p99 was **53.031ms**. The former 7.48s/8.60s pauses seen in round three were eliminated in this run; loading still does not maintain 120fps.
- Observed initial Rust readiness was **54.20s**, Rust reload **58.41s**, and replacement with ripgrep **2.46s**. Readiness includes CPU source preparation; the scene-upload loading interface is only part of that interval.

Native tests send input only to the owned, foreground viewer. The process closed successfully after testing. Exact states, frame records, screenshots, and reports remain under `D:\Codetree\evidence\12d-native-4`.

## Remaining boundaries

The reference engine is static and not compiler-complete. Final resolved/unresolved/parse-failure counts are: this project **58 / 90 / 0**, ripgrep **240 / 600 / 0**, llama.cpp **4,575 / 8,717 / 0**, and Rust **38,458 / 54,355 / 2,344**. Unresolved records include external dependencies and unsupported configuration, so these numbers are not a recall score. The full graph and each unresolved item are retained in `dependencies.json`.

The critic independently confirmed all ten module declarations in ripgrep's Cargo-declared `tests/tests.rs`, both nested `crate::util` references, and an empty tests-off visibility mask for that integration root. It reconstructed every visibility mask from graph edges for all four repositories and both test modes, with exact agreement. Every resolved edge belongs to one of the 66 planes.

A remaining UI issue is that a full-path hover tooltip can cover source after selecting a sidebar row. Moving the pointer away dismisses it. This is recorded as a usability issue, rather than hidden as a rendering success.

Example captures: [Rust 8K projection](/mnt/d/Codetree/evidence/12d-round-4/rust/01-connected-plane.png), [read source](/mnt/d/Codetree/evidence/12d-round-4/rust/05-source-reading.png), [overlap candidates](/mnt/d/Codetree/evidence/12d-native-4/13-overlap-candidates.png), and [responsive reload](/mnt/d/Codetree/evidence/12d-native-4/16-loading-reload.png).

## Build and retained baseline

The Windows release build, `cargo check --locked --all-targets`, and all 11 `cargo test --locked --all-targets` tests passed. Shell launch/build/benchmark scripts passed syntax checks. The final source, tests, scripts, Cargo manifest, and lockfile match the frozen fourth candidate; the installed executable hash also matches.

Because shared GPU helpers changed for background loading, the original `--tree` view also received a separate 120-frame 3840 × 2160 regression run on this project's 24 source files. It completed with zero GPU validation errors; overview and source-reading captures were inspected. This short regression is not additional 8K/120 qualification. Its artifacts are under `D:\Codetree\evidence\12d-tree-regression-4`.

The earlier 2D executable and source remain under `D:\Codetree\evidence\12d-baseline`. Development probes and all earlier critic reports remain separate from the final candidate. A final Windows process check found zero `codetree` processes after verification.

## Final critic scorecard

| Criterion | Score / 10 |
|---|---:|
| Visual and design quality | 8.3 |
| Source and reference fidelity | 8.2 |
| Navigation and usability | 8.7 |
| Rendering and mathematical correctness | 8.9 |
| Performance and scale | 7.5 |
| Engineering and reliability | 9.0 |

The mean is 8.433, reported as **8.4**. The final ranked gaps are sustained 120fps deadlines, complete direct-reference coverage, and the persistent tooltip obstruction. The four-review limit is exhausted; the installed binary remains the reviewed fourth candidate.
