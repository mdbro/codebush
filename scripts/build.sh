#!/usr/bin/env bash
set -euo pipefail
project_dir=$(cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"
mkdir -p /mnt/d/CodeBush/bin /mnt/d/CodeBush/tmp
TMPDIR=/mnt/d/CodeBush/tmp cargo xwin build --locked --release --target x86_64-pc-windows-msvc
cp target/x86_64-pc-windows-msvc/release/codebush.exe /mnt/d/CodeBush/bin/codebush.exe
printf 'Built D:\\CodeBush\\bin\\codebush.exe\n'
