#!/usr/bin/env bash
#
# Deploy utility-protocol-contracts to the local Stellar standalone network.
#
# Requires: stellar CLI 27.x, rustc/cargo with the wasm32v1-none target.
set -euo pipefail

# ---------------------------------------------------------------------------
# Corrected for stellar CLI 27 / Soroban SDK 27:
#   - artifact is built from the *lib* target name, not the package name
#   - wasm32v1-none is the SDK 27 target (not wasm32-unknown-unknown)
#   - `stellar quickstart` was replaced by `stellar container start`
#   - the local network is addressed as `standalone`
# ---------------------------------------------------------------------------

CONTRACT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$CONTRACT_DIR"

WASM_PATH="target/wasm32v1-none/release/utility_protocol_contracts.wasm"

ADMIN_IDENTITY="${ADMIN_IDENTITY:-admin}"

# register_operator() calls operator.require_auth() in addition to the admin
# check, so a single-account deployment only works when the operator is also
# the admin. Override OPERATOR_IDENTITY once a second key can co-sign.
OPERATOR_IDENTITY="${OPERATOR_IDENTITY:-$ADMIN_IDENTITY}"

SOLAR_TARIFF="${SOLAR_TARIFF:-7}"     # stroops per Wh
WATER_TARIFF="${WATER_TARIFF:-12}"    # stroops per L
OPERATOR_STAKE="${OPERATOR_STAKE:-1000000}"  # stroops of hardware collateral

info() { printf '\033[0;36m[info]\033[0m %s\n' "$*"; }
fail() { printf '\033[0;31m[error]\033[0m %s\n' "$*" >&2; exit 1; }

command -v stellar >/dev/null 2>&1 || fail "stellar CLI not found on PATH"
[[ -f "$WASM_PATH" ]] || fail "WASM artifact missing at '$WASM_PATH' (build it first)"

stellar keys public-key "$ADMIN_IDENTITY" >/dev/null 2>&1 \
  || fail "identity '$ADMIN_IDENTITY' not found; run: stellar keys generate $ADMIN_IDENTITY"

ADMIN_ADDRESS="$(stellar keys public-key "$ADMIN_IDENTITY")"
info "admin identity    : $ADMIN_IDENTITY ($ADMIN_ADDRESS)"
if [[ "$OPERATOR_IDENTITY" != "$ADMIN_IDENTITY" ]]; then
  info "operator identity : $OPERATOR_IDENTITY (requires a co-signature)"
fi

# --- 1. Build -------------------------------------------------------------
# Built on the host rather than in a pinned rust image: the Soroban SDK 27
# lockfile will not compile under the rust:1.80 base image that earlier
# revisions of this script used.
info "building contract (wasm32v1-none) ..."
rustup target list --installed | grep -qx wasm32v1-none \
  || fail "target missing; run: rustup target add wasm32v1-none"
cargo build --target wasm32v1-none --release --locked
info "artifact: $WASM_PATH ($(stat -c%s "$WASM_PATH") bytes)"

# --- 2. Local network -----------------------------------------------------
# This step is idempotent: if the standalone RPC is already answering (for
# example because `docker compose up` brought up the `soroban` service), it is
# reused as-is. Only one of the two may bind port 8000 from cold.
if ! stellar contract list --network standalone >/dev/null 2>&1; then
  info "starting local standalone network ..."
  stellar container start
fi
stellar contract list --network standalone >/dev/null 2>&1 \
  || fail "standalone network unreachable; try: stellar container start"

# --- 3. Deploy ------------------------------------------------------------
info "uploading and deploying contract ..."
CONTRACT_ID="$(
  stellar contract deploy \
    --wasm "$WASM_PATH" \
    --source-account "$ADMIN_IDENTITY" \
    --network standalone \
    --yes \
    -- \
    --initialize \
    --admin "$ADMIN_ADDRESS" \
  | awk '/contract id:/{print $NF}'
)"

[[ -n "$CONTRACT_ID" ]] || fail "could not parse contract id from deploy output"
info "contract id: $CONTRACT_ID"

invoke() {
  stellar contract invoke \
    --contract-id "$CONTRACT_ID" \
    --source-account "$ADMIN_IDENTITY" \
    --network standalone \
    --yes \
    -- "$@"
}

# --- 4. Configure ---------------------------------------------------------
# Skipping these leaves record_usage_tick() unusable: it looks up a tariff for
# the resource class and requires the operator to be registered.
info "setting tariffs (solar=$SOLAR_TARIFF, water=$WATER_TARIFF stroops) ..."
invoke --set_tariff SOLAR "$SOLAR_TARIFF"
invoke --set_tariff WATER "$WATER_TARIFF"

info "registering operator '$OPERATOR_IDENTITY' ..."
OPERATOR_ADDRESS="$(stellar keys public-key "$OPERATOR_IDENTITY")"
invoke --register_operator "$OPERATOR_ADDRESS" SOLAR "$OPERATOR_STAKE"

# --- 5. Report ------------------------------------------------------------
cat <<EOF

--------------------------------------------------------------
Deployed utility-protocol-contracts
--------------------------------------------------------------
  contract id  : $CONTRACT_ID
  admin        : $ADMIN_ADDRESS
  tariffs      : solar $SOLAR_TARIFF / water $WATER_TARIFF stroops
  network      : standalone

Wire the frontend and backend to it:

  export NEXT_PUBLIC_CONTRACT_ID=$CONTRACT_ID

Confirm the backend picks up events:

  export CONTRACT_ID=$CONTRACT_ID
  export SOROBAN_RPC_URL=http://localhost:8000
  npm --prefix ../utility-backend start
--------------------------------------------------------------
EOF
