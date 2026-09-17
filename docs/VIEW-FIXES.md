# View fixes — 16 September 2026

The installed follow-up implements the user's tall source-column request, automatic framing for rotations, and a GUI switch between the directory tree and 12D projection workspace.

- Every file reads down one column. Long lines soft-wrap underneath their original line number; full source, test filtering, syntax coloring and precise glyph rendering are retained. Directory packing considers tall shapes and the viewport's aspect ratio.
- Rotation first eases the camera for 280 ms into a conservative frame for the whole path, then holds that camera while the graph rotates. The bounds include connected files absent at both endpoints, annotation extents, and an analytic displacement margin between orientation samples. Scrubbing, interrupted turns, test changes and resizing retain containment. Reading/panning/zooming pauses motion; the next turn frames the graph again.
- The button beside the logo switches **Tree view / 12D view** in one window, sharing all source buffers, source images and GPU allocations. Each controller retains its camera, selection and search state. Tests follow the user between modes. Filtering tests while reading keeps the selected source anchored on screen.
- Folder reload/open rebuilds both controllers and preserves mode, test inclusion and cursor position. A tooltip is dismissed after source selection until the pointer moves again.

## Verification

`cargo fmt --all --check`, `cargo test --locked --all-targets` and the Windows release build passed. The 12 contract tests cover source fidelity, hidden tests, single-column row order, culling, references and rotation invariants. The framing regression covers all 4,356 ordered plane transitions, redirected intermediate rotations, asymmetric points and tall panels at three viewport sizes including 8K.

The native mouse/keyboard suite ran on the RTX 5090 with Windows DX12. It includes same-position mode-button clicks, three GUI round trips per repository, source search/reading, test filtering, nonadjacent and interrupted rotations, midpoint scrubbing, resizing and tours. Mode switches preserve allocation counts and do not update the load report. Ripgrep additionally starts with `--tree`.

| Repository | Source files | Lines | Resident GB | Evaluated rotation frames | Escaped frames |
|---|---:|---:|---:|---:|---:|
| codetree | 26 | 10,523 | 4.27 | 2,005 | 0 |
| ripgrep | 116 | 57,293 | 8.95 | 2,004 | 0 |
| llama | 2,709 | 934,592 | 17.41 | 2,007 | 0 |
| rust | 38,322 | 4,417,433 | 15.81 | 1,994 | 0 |

Across these runs: **21,407 native frames, 8,010 evaluated rotation frames, zero viewport escapes during those rotations, and zero GPU validation errors.** All 41 native capture states were inspected through contact sheets, with individual reading and return-to-tree captures checked at larger size. Explicit reading zoom intentionally shows a portion of a tall file; containment applies to fitted rotation views.

Artifacts: `D:\Codetree\evidence\view-fixes\verified-{codetree,ripgrep,llama,rust}`. Each directory retains load reports, interaction snapshots, per-frame timing/containment data, screenshots and executable identity. Earlier failures and interim runs remain alongside them.

The four-repository suite used executable SHA-256 `6110501f98ddc1baf7294bd014792365a036c1a743f6580e0a4722d593cfe885`. A subsequent cursor-position preservation fix after folder replacement produces installed SHA-256 `0ac015ed9e513c92796d854299d855506e337cd307fc0d56d60ff3eea5974184`; it leaves the layout, rendering and rotation code unchanged. Its targeted tree reload/open/mode-switch regression passed with zero GPU errors in `verified-reload-v2`.

The installed binary also completed 12 offscreen llama.cpp captures at **7680 × 4320**, covering endpoint/intermediate projections, reading, tests, overlap selection, reference evidence and help. All PNG dimensions were checked, the captures were inspected, and the run exited successfully with zero GPU validation errors. These artifacts are in `verified-8k`; this capture run does not measure a 120fps deadline.

The installed source snapshot and hashes are under `current-source`; `installed.sha256` identifies the binary. `native-view-fixes.ps1` reproduces the main interaction suite. The folder-replacement harness is retained as `native-reload-mode.ps1` in the evidence directory.

## Scope of these results

This follow-up does not introduce another critic round or claim sustained 8K/120fps. The previous four-round qualification remains 8.4/10, below the 8.5 gate; its performance results belong to the previous frozen executable. Physical display refresh and complete compiler-level reference resolution remain outside this follow-up's verification. The attached desktop is 4K/60; native frame intervals do not measure physical scanout.
