#!/bin/sh
set -eu

HERA_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export HERA_ROOT
# shellcheck source=tools/demo-env.sh
. "$HERA_ROOT/tools/demo-env.sh"

cd "$HERA_ROOT"
mkdir -p "$HERA_ROOT/tmp"

API_LOG="$HERA_ROOT/tmp/hera-api-demo.log"
WORKER_LOG="$HERA_ROOT/tmp/hera-worker-demo.log"

cleanup() {
  status=$?

  if [ -n "${WORKER_PID:-}" ] && kill -0 "$WORKER_PID" 2>/dev/null; then
    kill "$WORKER_PID" 2>/dev/null || true
  fi

  if [ -n "${API_PID:-}" ] && kill -0 "$API_PID" 2>/dev/null; then
    kill "$API_PID" 2>/dev/null || true
  fi

  if [ -n "${WORKER_PID:-}" ]; then
    wait "$WORKER_PID" 2>/dev/null || true
  fi

  if [ -n "${API_PID:-}" ]; then
    wait "$API_PID" 2>/dev/null || true
  fi

  exit "$status"
}

show_logs() {
  if [ -f "$API_LOG" ]; then
    echo "Recent API log lines:" >&2
    tail -n 40 "$API_LOG" >&2 || true
  fi

  if [ -f "$WORKER_LOG" ]; then
    echo "Recent worker log lines:" >&2
    tail -n 40 "$WORKER_LOG" >&2 || true
  fi
}

wait_for_api() {
  attempt=0
  while [ "$attempt" -lt 90 ]; do
    if ! kill -0 "$API_PID" 2>/dev/null; then
      echo "Hera API exited before becoming ready." >&2
      show_logs
      return 1
    fi

    if curl -fsS \
      -H "Authorization: Bearer $HERA_API_KEY" \
      "$HERA_API_URL/v1/workspaces?limit=1&offset=0" >/dev/null 2>&1; then
      return 0
    fi

    sleep 1
    attempt=$((attempt + 1))
  done

  echo "Timed out waiting for Hera API readiness on $HERA_API_URL." >&2
  show_logs
  return 1
}

trap cleanup EXIT INT TERM

echo "Starting local Hera infrastructure with Docker Compose..."
docker compose -f infra/docker/docker-compose.yml --env-file infra/docker/.env.example up -d

echo "Applying database migrations..."
cargo run -p hera-db --bin migrate

echo "Seeding demo tenant for SDK authentication..."
"$HERA_ROOT/tools/seed-demo-tenant.sh"

echo "Starting Hera API..."
"$HERA_ROOT/tools/start-demo-api.sh" >"$API_LOG" 2>&1 &
API_PID=$!

echo "Starting Hera worker..."
"$HERA_ROOT/tools/start-demo-worker.sh" >"$WORKER_LOG" 2>&1 &
WORKER_PID=$!

echo "Waiting for API readiness..."
wait_for_api

echo "Running Stage 2 SDK signed-report demo..."
node "$HERA_ROOT/tools/run-sdk-signed-demo.mjs"

echo "SDK demo completed."
echo "Artifacts: $HERA_OUTPUT_DIR"
echo "API log: $API_LOG"
echo "Worker log: $WORKER_LOG"
