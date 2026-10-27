#!/usr/bin/env bash
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
output=${1:-"$root/dist"}
mkdir -p "$output"
output=$(cd -- "$output" && pwd -P)
if [[ -e "$output/codwawzpc-linux-x86_64" || -e "$output/codwawzpc-linux-x86_64.tar.gz" ]]; then
    printf '%s\n' "Refusing to overwrite $output/codwawzpc-linux-x86_64" >&2
    exit 1
fi
if [[ $(uname -m) != x86_64 ]]; then printf '%s\n' 'This build requires an x86_64 host.' >&2; exit 1; fi
docker build -f "$root/packaging/linux/Dockerfile" -t codwawzpc-portable-builder:22.04 "$root"
docker run --rm --platform linux/amd64 -e "SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-0}" -v "$root:/src:ro" -v "$output:/out" \
    codwawzpc-portable-builder:22.04 bash -ec '
      git config --global --add safe.directory /src
      export CARGO_TARGET_DIR=/tmp/codwawzpc-target
      export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
      export CARGO_INCREMENTAL=0
      cargo build --locked --release --bin codwawzpc
      python3 /src/packaging/linux/assemble.py /src "$CARGO_TARGET_DIR" /out/codwawzpc-linux-x86_64
    '
if [[ $(id -u) != 0 ]]; then docker run --rm -v "$output:/out" --entrypoint chown codwawzpc-portable-builder:22.04 -R "$(id -u):$(id -g)" /out/codwawzpc-linux-x86_64; fi
(cd "$output" && tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${SOURCE_DATE_EPOCH:-0}" -czf codwawzpc-linux-x86_64.tar.gz codwawzpc-linux-x86_64)
sha256sum "$output/codwawzpc-linux-x86_64.tar.gz"
