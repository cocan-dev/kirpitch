#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Runs `kicad-cli` from an official KiCad container image, so tests can use any
# KiCad version on a Linux machine without installing it:
#
#   export KIRPITCH_KICAD_IMAGE=kicad/kicad:10.0.8
#   export KIRPITCH_KICAD_CLI="$PWD/scripts/kicad-cli-docker.sh"
#   cargo test --all
#
# The temporary folder and the repository are made visible in the container at
# the same paths, and the tool runs as the current user.
set -euo pipefail

image="${KIRPITCH_KICAD_IMAGE:?set KIRPITCH_KICAD_IMAGE, for example kicad/kicad:10.0.8}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="${TMPDIR:-/tmp}"

exec docker run --rm \
  --user "$(id -u):$(id -g)" \
  -e HOME=/tmp \
  -v "$tmp:$tmp" \
  -v "$repo:$repo" \
  -w "$PWD" \
  "$image" kicad-cli "$@"
