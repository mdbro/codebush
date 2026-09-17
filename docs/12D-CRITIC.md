# Independent final 12D review

**12D critic round 4 of 4 — 8.4/10, FAIL. Final review cycle closed.**

The final revision repairs the verified Cargo target omission, makes overlapping files discoverable, proves reference navigation with actual file IDs, and removes the multi-second native replacement freezes. It remains below the 8.5 acceptance threshold. Every repository still fails the sustained 120 fps frame-deadline gate, and the broader requirement to resolve every direct reference remains unestablished. No rendering corruption was observed in the inspected images; all recorded GPU validation error lists are empty. An oversized persistent path tooltip still obstructs source reading.

This verdict covers the [frozen fourth candidate](/mnt/d/Codetree/evidence/12d-frozen-4/) and executable SHA256 `1706e3451508c0648e38c73f912446dbae19533d7df36833dc3ff07b321ed9c3`. I independently confirmed that the frozen and installed executables match. I inspected **all 47 individual 7680×4320 captures** across [Codetree](/mnt/d/Codetree/evidence/12d-round-4/codetree/), [ripgrep](/mnt/d/Codetree/evidence/12d-round-4/ripgrep/), [llama.cpp](/mnt/d/Codetree/evidence/12d-round-4/llama/), and [Rust](/mnt/d/Codetree/evidence/12d-round-4/rust/), plus **all 17 native captures** in [the completed native workflow](/mnt/d/Codetree/evidence/12d-native-4/). Source, contract tests, dependency manifests, benchmark frame arrays, native state snapshots, and native timing arrays were also reviewed. No critic GPU workload or implementation edit was made. Earlier preflight failures and successes are diagnostic history, not final acceptance evidence.

| Criterion | Score | Assessment |
|---|---:|---|
| Visual and design quality | 8.3 | Consistent typography and controls, improved identifying paths, and readable selected source. Large overviews remain dense meshes, and the full-path tooltip can cover reading content. |
| Source and reference fidelity | 8.2 | All concrete resolver regressions found in preceding reviews are repaired, including real Cargo integration roots and test ancestry. Compiler-complete direct-reference coverage is still absent. |
| Navigation and usability | 8.7 | Native overlap cycling, an explicit candidate list, reference following, paused scrubbing, tests, reload, and folder replacement are demonstrated. |
| Rendering and mathematical correctness | 8.9 | Proper 12D rotation structure and visibility contracts are retained. No observed glyph, texture, or selected-card compositing corruption. |
| Performance and scale | 7.5 | The large corpus remains resident below the allocation cap; GPU execution is fast. Full-frame tails miss 120 fps, including steady native rotations. Replacement responsiveness improves substantially. |
| Engineering and reliability | 9.0 | Frozen provenance, regression tests, all-plane workload assertions, command-scoped native checks, and phase diagnostics provide strong evidence. Reported boundaries remain explicit. |

The arithmetic mean is **8.433**, reported as **8.4**. Zero observed rendering corruption does not override the failed score or performance requirement.

The approved dimension convention is implemented consistently: references are partitioned deterministically across 66 plane layers, while fixed 12D coordinates and proper rotations determine projected anchors and edges. The graph partition does not imply that an edge has displacement only along its assigned two axes. Source cards remain screen-facing annotations at projected anchors. The rotation contract tests cover all 4,356 ordered coordinate-plane transitions, interrupted rotations, distance preservation, orthogonality, determinant, and continuity. All 11 contract tests are reported passing. The final benchmark reports have maximum orthogonality error approximately 6.66e-16.

I independently reconstructed every file's visibility masks from the included directed edges for both test modes in all four dependency manifests. They match exactly. Edges have valid endpoints and plane indices, and the masks exclude files without included incident links. This proves the display/filter contract for the resolved graph; it does not prove that the resolver recovered every real reference.

The real ripgrep omission from round 3 is fixed. `tests/tests.rs`, ID 114, now has all ten expected module edges to `macros.rs`, `hay.rs`, `util.rs`, `binary.rs`, `feature.rs`, `index/mod.rs`, `json.rs`, `misc.rs`, `multiline.rs`, and `regression.rs`. The recorded evidence lines match the declarations, and all ten edges are test-only. The nested `tests/index/basic.rs:1` and `tests/index/disallowed.rs:1` imports now resolve to `tests/util.rs`. The root has no visible membership with tests excluded. The new fixture exercises a custom Cargo test path, sibling modules, nested crate imports, and test ancestry while keeping Cargo metadata outside the displayed source. All four final manifests have zero manifest errors. No new concrete local-resolution regression was confirmed in this review.

The before/after overlap captures show selected source raised and readable. Middle ellipsis preserves distinguishing filename suffixes, and complete paths are available on hover and in the selected header. The [native candidate view](/mnt/d/Codetree/evidence/12d-native-4/13-overlap-candidates.png) lists both files under the pointer. I checked that the cycled ID set exactly equals the expected set `{277, 452}`, identifying `compiler/rustc_codegen_cranelift/build_system/prepare.rs` and `compiler/rustc_codegen_gcc/tests/run/unreachable-function.rs`. Following the line-2 `std::hash::Hash` reference from file 277 selects file **3287**, `library/std/src/hash/mod.rs`, exactly as expected. The destination is visibly readable in [capture 15](/mnt/d/Codetree/evidence/12d-native-4/15-followed-reference.png).

Native command-scoped assertions now distinguish the clicked midpoint from the later drag: 525 held midpoint frames at plane index 58 and factor approximately 0.50058, and 195 held drag frames at the same plane and factor approximately 0.71970. Both states are paused. The candidate list, actual selection IDs, expected destination, test toggle, touring, Help, reload, and a replacement with ripgrep all have completed evidence. These checks repair the previous round's overly broad assertion scope.

Each offscreen run contains 7,920 measured frames. All 66 planes have real nonidentity rotation coverage in both test modes, with overview, source-reading, and cache-transition workloads. There are **31,680 measured frames in total**, no identity-rotation samples substituting for animation, and no reported GPU validation errors.

| Repository | Source files / lines | Directed links | Resident allocation, decimal GB | GPU execution p99 | Completion p99 | Deadline p99 |
|---|---:|---:|---:|---:|---:|---:|
| Codetree | 24 / 10,321 | 58 | 3.06 | 0.802 ms | 11.695 ms | 13.338 ms |
| ripgrep | 116 / 57,293 | 240 | 15.35 | 0.791 ms | 9.653 ms | 11.734 ms |
| llama.cpp | 2,709 / 934,592 | 4,575 | 24.68 | 0.726 ms | 10.366 ms | 15.194 ms |
| Rust | 38,322 / 4,417,433 | 38,458 | 29.16 | 1.949 ms | 18.000 ms | 18.573 ms |

All four `passes_120fps_p99` fields are false. Average cadence is approximately 120.01 Hz, but 115, 126, 226, and 399 frames respectively exceed the 8.333 ms deadline: approximately 1.45%, 1.59%, 2.85%, and 5.04%. Rust projection p99 is 0.1366 ms and UI p99 is 0.552 ms. Its source-reading and cache-transition workloads remain measured, rather than inferred from overview performance. The largest report records **29,160,434,299 resident bytes**, within the configured 30 decimal GB allocation cap. This is an allocation report, not an independent certification of whole-card peak usage. Initial offscreen Rust readiness is 51.479 seconds.

The [native trace](/mnt/d/Codetree/evidence/12d-native-4/native-timing.json) has **13,893 frames** in a **2178×1444 window**, with zero reported GPU errors. I recomputed percentiles using the executable's floor((n−1)×p) convention:

| Native subset | Frames | Interval p99 | Maximum interval | Tick p99 |
|---|---:|---:|---:|---:|
| Entire workflow | 13,893 | 13.775 ms | 133.556 ms | 8.312 ms |
| Not in residency-loading view | 13,317 | 12.880 ms | 96.573 ms | 7.466 ms |
| Plane tour | 1,865 | 11.028 ms | 15.830 ms | 1.615 ms |
| Residency-loading view | 576 | 53.031 ms | 133.556 ms | 52.915 ms |

The native tour is a useful steady-rotation subset: its median interval is 8.333 ms and UI p99 is 0.488 ms, but interval p99 still exceeds the target. These are application CPU intervals and API timings, not displayed-frame measurements. GPU timestamp duration, completion callbacks, application scheduling, and physical presentation must remain distinct. The attached display was documented as 4K60; true 8K offscreen rendering does not establish physical 8K120 presentation.

The replacement change is a substantial verified improvement. Final native initial Rust readiness is **54.201 seconds**, reload **58.407 seconds**, and opening ripgrep **2.462 seconds**. Both replacement operations show an updating loading view and finish with the correct repository, source counts, graph, and allocation. The prior 7–9 second frame gaps are gone: the maximum recorded interval is now 133.556 ms. This establishes responsiveness relative to the previous freeze, not 120 fps during loading. The frozen code keeps Windows surface creation on the window thread while moving resident retirement and upload to a worker.

The ranked remaining issues and requirement gaps are:

1. **P1 — Sustained 120 fps remains unmet.** All four complete 8K runs fail the 8.333 ms deadline p99 gate, and even the native steady tour has interval p99 11.028 ms. Preserve the existing all-plane, both-test-mode, source-detail workload when assessing future performance. Completion and scheduling tails need investigation using their separate phase measurements; low shader times or 120 Hz average cadence are insufficient. Loading tails are also still visible, although the multi-second blocking defect is repaired. Any future claim of physical 8K120 needs presentation evidence from a suitable display path.

2. **P1 requirement gap — Every direct reference is not established.** The demonstrated local Cargo defects are repaired, but the implementation remains a static resolver with documented limits around generated modules, macro expansion, build-specific include paths, package aliases, computed imports, and language-specific binding. Rust records 2,344 partial/failed whole-file parses, with fallback recovery, and 54,355 unresolved entries; unsupported extractors also remain for some indexed languages. These counts include external dependencies and are **not** a measured local-reference miss rate. Do not present the graph as complete. Establish representative, independently checked local-reference coverage and configuration-aware resolution before claiming every direct reference is linked. The source can still be inspected through the original tree mode.

3. **P2 — The full-path tooltip can obstruct the source just selected for reading.** In [native capture 02](/mnt/d/Codetree/evidence/12d-native-4/02-read-source.png), a tooltip for the short `compiler/rustc/build.rs` path covers source lines after the sidebar selection and Enter action. The same behavior is visible after following a reference in [capture 09](/mnt/d/Codetree/evidence/12d-native-4/09-followed-reference.png) and [capture 15](/mnt/d/Codetree/evidence/12d-native-4/15-followed-reference.png). Frozen `src/hyper_app.rs:833` chooses up to 800 logical pixels of width regardless of path length, and the retained hover persists while the pointer remains on the row. Size the tooltip to its content and dismiss or defer it after selection/read navigation until intentional hover resumes. Preserve complete-path access. Validate both short and long paths through the normal click-then-Enter and reference-follow workflows. This is a presentation/usability defect, not GPU corruption.

Dense unselected projections still make overview file identity difficult, particularly in Rust. The candidate list and source raising now give a working inspection route. They do not establish simultaneous unobscured text for every overlapping annotation. Any further visual refinement should retain the approved geometry and every included edge.

**Final verdict: FAIL at 8.4/10.** The four permitted 12D critic rounds are complete. The final candidate has substantial verified functionality and clean observed rendering, but sustained 120 fps, complete reference coverage, and the tooltip obstruction remain explicit. This report does not authorize or request a fifth iteration.
