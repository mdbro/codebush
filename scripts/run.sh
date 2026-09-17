#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "$0")/.." && pwd)
source_dir=${1:-$project_dir}
if (( $# )); then shift; fi
if [[ ! -f /mnt/d/CodeBush/bin/codebush.exe ]]; then "$project_dir/scripts/build.sh"; fi
cd "$project_dir"
exec /mnt/d/CodeBush/bin/codebush.exe "$(wslpath -w "$(realpath "$source_dir")")" "$@"
