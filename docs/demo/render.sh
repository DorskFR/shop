#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release
docker run --rm \
  -v "$PWD:/vhs" \
  -v /usr/share/fonts/opentype/noto:/usr/share/fonts/noto:ro \
  ghcr.io/charmbracelet/vhs docs/demo/demo.tape
