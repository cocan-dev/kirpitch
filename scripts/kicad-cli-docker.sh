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
# the same paths, and the tool runs as the current user. Its home and temporary
# folder are private to each call, because KiCad keeps lock files there and
# parallel calls would otherwise warn about each other. Home and temporary
# folder of the tool itself are private to each call, because KiCad keeps lock
# files there and parallel calls would otherwise warn about each other.
set -euo pipefail

image="${KIRPITCH_KICAD_IMAGE:?set KIRPITCH_KICAD_IMAGE, for example kicad/kicad:10.0.8}"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
tmp="${TMPDIR:-/tmp}"

exec docker run --rm \
  --user "$(id -u):$(id -g)" \
  --tmpfs /kicad-tmp:rw,mode=1777 \
  -e HOME=/kicad-tmp \
  -e TMPDIR=/kicad-tmp \
  -v "$tmp:$tmp" \
  -v "$repo:$repo" \
  -w "$PWD" \
  "$image" kicad-cli "$@"
