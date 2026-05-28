#!/bin/sh
set -eu

HERA_ROOT="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
export HERA_ROOT
# shellcheck source=tools/demo-env.sh
. "$HERA_ROOT/tools/demo-env.sh"

escaped_api_key=$(printf "%s" "$HERA_API_KEY" | sed "s/'/''/g")
escaped_name=$(printf "%s" "Hera SDK Demo Tenant" | sed "s/'/''/g")

docker exec -i hera-postgres psql -U hera -d hera -v ON_ERROR_STOP=1 <<SQL
INSERT INTO tenants (name, api_key_hash)
VALUES ('$escaped_name', '$escaped_api_key')
ON CONFLICT (api_key_hash) DO NOTHING;
SQL
