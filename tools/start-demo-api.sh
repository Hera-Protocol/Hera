#!/bin/sh
set -eu

cd "$(dirname "$0")/.."

export TMPDIR=/tmp
export DATABASE_URL="${DATABASE_URL:-postgres://hera:devpassword@localhost:5433/hera}"
export REDIS_URL="${REDIS_URL:-redis://localhost:6380}"
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
export SCAN_QUEUE_NAME="${SCAN_QUEUE_NAME:-hera:scan:pending:mock}"

cargo run -p hera-api
