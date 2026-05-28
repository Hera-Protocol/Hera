#!/bin/sh
set -eu

HERA_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export HERA_ROOT
# shellcheck source=tools/demo-env.sh
. "$HERA_ROOT/tools/demo-env.sh"

cd "$HERA_ROOT"

cargo run -p hera-worker
