#!/bin/sh

if [ -z "${HERA_ROOT:-}" ]; then
  echo "HERA_ROOT must be set before sourcing tools/demo-env.sh" >&2
  return 1
fi

if [ -f "$HERA_ROOT/.env" ]; then
  set -a
  # shellcheck disable=SC1091
  . "$HERA_ROOT/.env"
  set +a
elif [ -f "$HERA_ROOT/.env.example" ]; then
  set -a
  # shellcheck disable=SC1091
  . "$HERA_ROOT/.env.example"
  set +a
fi

export TMPDIR="${TMPDIR:-/tmp}"
export DATABASE_URL="${DATABASE_URL:-postgres://hera:devpassword@localhost:5432/hera}"
export REDIS_URL="${REDIS_URL:-redis://localhost:6379}"
export AWS_ENDPOINT_URL="${AWS_ENDPOINT_URL:-http://localhost:4566}"
export AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-test}"
export AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-test}"
export AWS_REGION="${AWS_REGION:-us-east-1}"
export S3_BUCKET="${S3_BUCKET:-hera-reports}"
export KMS_KEY_ID="${KMS_KEY_ID:-alias/hera-dev}"
export DEV_KMS_KEY_BASE64="${DEV_KMS_KEY_BASE64:-srX6ZqJwXKgkiFy9v/ZDR6FGnwdJStYiMV7mgRst5eY=}"
export SIGNING_KEY_BASE64="${SIGNING_KEY_BASE64:-EKrpZA7rHuL/2XfaLtww7v2DMUlvkAJzKK5LLetLC6Y=}"
export LIGHTWALLETD_URL="${LIGHTWALLETD_URL:-https://zec.rocks:443}"
export LIGHTWALLETD_FALLBACK_URLS="${LIGHTWALLETD_FALLBACK_URLS:-https://na.zec.rocks:443,https://eu.zec.rocks:443,https://ap.zec.rocks:443,https://sa.zec.rocks:443,https://zcashd.zec.rocks:443}"
export NAMADA_INDEXER_URL="${NAMADA_INDEXER_URL:-https://masp.namada.net}"
export NAMADA_CHAIN_ID="${NAMADA_CHAIN_ID:-namada.5f5de2dd1b88cba30586420}"
export NAMADA_RPC_URL="${NAMADA_RPC_URL:-https://rpc.namada.net}"
export API_BIND_ADDR="${API_BIND_ADDR:-127.0.0.1:3101}"
export HERA_API_URL="${HERA_API_URL:-http://127.0.0.1:3101}"
export HERA_API_KEY="${HERA_API_KEY:-hera-demo-1778618450}"
export HERA_OUTPUT_DIR="${HERA_OUTPUT_DIR:-$HERA_ROOT/tmp/sdk-signed-demo}"
export SCAN_QUEUE_NAME="${SCAN_QUEUE_NAME:-hera:scan:pending:demo}"
export SCAN_PROCESSING_QUEUE_NAME="${SCAN_PROCESSING_QUEUE_NAME:-hera:scan:processing:demo}"
export NAMADA_DECODER_COMMAND="${NAMADA_DECODER_COMMAND:-$HERA_ROOT/tools/mock-namada-empty-decoder.sh}"
