#!/usr/bin/env bash
# Deploy Kitty Ledger to Stellar testnet and seed a demo campaign.
#
# NOT EXECUTED IN THE BUILD ENVIRONMENT: soroban-testnet.stellar.org, horizon-testnet
# and friendbot were unreachable there (see TOOLCHAIN.md). The script follows the
# stellar-cli 28 command shapes and was reviewed, not run. Expect to adjust flag
# names if your stellar-cli version differs.
#
# Requires: stellar-cli 28, built wasm files (scripts/build.sh), network access.
# Usage:    scripts/deploy-testnet.sh            # deploys, registers billers, creates campaign
#           ORGANIZER=alice scripts/deploy-testnet.sh   # reuse an existing identity name
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

NETWORK="${NETWORK:-testnet}"
ORGANIZER="${ORGANIZER:-kitty-organizer}"
ADMIN="${ADMIN:-kitty-registry-admin}"
WASM_DIR="target/wasm32v1-none/release"

identity_exists() { stellar keys address "$1" >/dev/null 2>&1; }

echo "== identities (funded through friendbot)"
for who in "$ORGANIZER" "$ADMIN" kitty-member-1 kitty-member-2 kitty-member-3 kitty-contributor; do
  if ! identity_exists "$who"; then
    stellar keys generate --global "$who" --network "$NETWORK" --fund
  fi
  echo "  $who  $(stellar keys address "$who")"
done
ORGANIZER_PK="$(stellar keys address "$ORGANIZER")"
ADMIN_PK="$(stellar keys address "$ADMIN")"
M1="$(stellar keys address kitty-member-1)"
M2="$(stellar keys address kitty-member-2)"
M3="$(stellar keys address kitty-member-3)"

echo "== test token: wrap a classic asset issued by the admin as a Stellar Asset Contract"
# On mainnet this would be USDC's SAC id instead of a test asset.
TOKEN_ID="$(stellar contract asset deploy --asset "KUSD:${ADMIN_PK}" --source "$ADMIN" --network "$NETWORK" 2>/dev/null \
  || stellar contract id asset --asset "KUSD:${ADMIN_PK}" --network "$NETWORK")"
echo "  TOKEN_CONTRACT_ID=$TOKEN_ID"

echo "== deploy biller_registry"
REGISTRY_ID="$(stellar contract deploy --wasm "$WASM_DIR/biller_registry.wasm" --source "$ADMIN" --network "$NETWORK" --alias kitty-registry)"
echo "  REGISTRY_CONTRACT_ID=$REGISTRY_ID"
stellar contract invoke --id "$REGISTRY_ID" --source "$ADMIN" --network "$NETWORK" -- init --admin "$ADMIN_PK"

echo "== register the seed billers (data/seed/billers.json)"
# name_hash = sha256("<name> / <reference>") as 32 bytes hex; the CLI's hash.ts uses the same rule.
python3 - "$REGISTRY_ID" "$ADMIN" "$NETWORK" <<'PY'
import json, hashlib, subprocess, sys
registry, admin, network = sys.argv[1:4]
for b in json.load(open("data/seed/billers.json"))["billers"]:
    name_hash = hashlib.sha256(f'{b["name"]} / {b["reference"]}'.encode()).hexdigest()
    out = subprocess.check_output(["stellar","contract","invoke","--id",registry,"--source",admin,"--network",network,"--",
        "register_biller","--name_hash",name_hash,"--category",b["category"],"--address",b["address"]], text=True).strip()
    print(f'  biller {out} <- {b["name"]} ({b["category"]})')
    if b["verified"]:
        subprocess.check_call(["stellar","contract","invoke","--id",registry,"--source",admin,"--network",network,"--",
            "set_verified","--id",out.strip('"'),"--verified","true"])
PY

echo "== deploy campaign"
CAMPAIGN_ID="$(stellar contract deploy --wasm "$WASM_DIR/campaign.wasm" --source "$ORGANIZER" --network "$NETWORK" --alias kitty-campaign)"
echo "  CAMPAIGN_CONTRACT_ID=$CAMPAIGN_ID"

echo "== create the seed campaign (goal 3000.00, 30 days, 2-of-3 committee)"
DEADLINE="$(( $(date +%s) + 30*86400 ))"
TITLE_HASH="$(printf 'Mama Njeri surgery fund - Nakuru chapter' | sha256sum | cut -d' ' -f1)"
stellar contract invoke --id "$CAMPAIGN_ID" --source "$ORGANIZER" --network "$NETWORK" -- create \
  --organizer "$ORGANIZER_PK" --token "$TOKEN_ID" --goal 30000000000 --deadline "$DEADLINE" \
  --committee "[\"$M1\",\"$M2\",\"$M3\"]" --threshold 2 --registry "$REGISTRY_ID" --title_hash "$TITLE_HASH"

echo "== fund the demo contributor with 500 KUSD and contribute 25.37"
CONTRIB_PK="$(stellar keys address kitty-contributor)"
stellar contract invoke --id "$TOKEN_ID" --source "$ADMIN" --network "$NETWORK" -- mint --to "$CONTRIB_PK" --amount 5000000000
MEMO_HASH="$(printf 'Pole sana. Get well soon' | sha256sum | cut -d' ' -f1)"
stellar contract invoke --id "$CAMPAIGN_ID" --source kitty-contributor --network "$NETWORK" -- contribute \
  --id 0 --from "$CONTRIB_PK" --amount 253700000 --memo_hash "$MEMO_HASH"

cat <<EOT

== done. Put these in .env:
RPC_URL=https://soroban-testnet.stellar.org
NETWORK_PASSPHRASE="Test SDF Network ; September 2015"
CAMPAIGN_CONTRACT_ID=$CAMPAIGN_ID
REGISTRY_CONTRACT_ID=$REGISTRY_ID
TOKEN_CONTRACT_ID=$TOKEN_ID
SECRET_KEY=$(stellar keys secret "$ORGANIZER" 2>/dev/null || echo "<stellar keys secret $ORGANIZER>")

Ledger page: open app/web/ledger.html, paste $CAMPAIGN_ID and the RPC URL.
Verify: https://stellar.expert/explorer/testnet/contract/$CAMPAIGN_ID
EOT
