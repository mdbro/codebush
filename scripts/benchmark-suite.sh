#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"
exe=/mnt/d/CodeBush/bin/codebush.exe
out=${1:-/mnt/d/CodeBush/evidence/12d-suite-$(date +%Y%m%d-%H%M%S)}
mkdir -p "$out"
sha256sum "$exe" > "$out/executable.sha256"
for name in codebush ripgrep llama rust; do
  case "$name" in
    codebush) source_dir=$project_dir ;;
    ripgrep) source_dir=/dev/shm/codetree-benchmarks/ripgrep ;;
    llama) source_dir=/home/mdwbr/llama.cpp ;;
    rust) source_dir=/dev/shm/codetree-benchmarks/rust ;;
  esac
  mkdir -p "$out/$name"
  "$exe" "$(wslpath -w "$source_dir")" --benchmark --benchmark-hz 120 --pipeline-depth 2 --frames 7920 --size 7680x4320 \
    --capture "$(wslpath -w "$out/$name")" --manifest "$(wslpath -w "$out/$name/manifest.json")" \
    > "$out/$name/run.log" 2>&1
  python3 - "$out/$name/benchmark.json" <<'PY'
import json,sys
j=json.load(open(sys.argv[1]))
print(f"{sys.argv[1]}: {j['source_files']} files, {j['source_lines']} lines, cadence {j['cadence_fps']:.1f} fps, deadline p99 {j['deadline_p99_ms']:.2f} ms, {len(j['rendering_errors'])} GPU errors, 120fps pass={j['passes_120fps_p99']}",flush=True)
PY
done
