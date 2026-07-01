#!/usr/bin/env bash
# Copy the built artifacts (service binary, static dashboard, plan) to the VM.
# Run on the BUILD HOST from the repo root:  deploy/push-artifacts.sh user@vm
#
# Build them first if needed:
#   cargo build --release --manifest-path service/Cargo.toml && strip service/target/release/paseo-downsizer-service
#   ( cd ui && NEXT_PUBLIC_SERVICE_URL="" pnpm install --frozen-lockfile && NEXT_PUBLIC_SERVICE_URL="" pnpm build )
set -euo pipefail

VM="${1:?usage: deploy/push-artifacts.sh <ssh-host>   (e.g. rai or user@1.2.3.4)}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/service/target/release/paseo-downsizer-service"
OUT="$ROOT/ui/out"
# Prefer the local (gitignored) plan carrying the named exit cohorts; fall back
# to the public plan (empty cohorts) if it's absent.
PLAN="$ROOT/plan/downsizing-plan.local.toml"; [[ -f "$PLAN" ]] || PLAN="$ROOT/plan/downsizing-plan.toml"

[[ -x "$BIN" ]]           || { echo "!! missing binary: $BIN  (run cargo build --release && strip)"; exit 1; }
[[ -f "$OUT/index.html" ]]|| { echo "!! missing static UI: $OUT  (run: cd ui && NEXT_PUBLIC_SERVICE_URL='' pnpm build)"; exit 1; }
[[ -f "$PLAN" ]]          || { echo "!! missing plan: $PLAN"; exit 1; }

echo "binary sha256: $(sha256sum "$BIN" | awk '{print $1}')"
echo "pushing to $VM ..."
scp "$BIN"                       "$VM:/tmp/paseo-downsizer-service"
scp -r "$OUT"                    "$VM:/tmp/dashboard"
scp "$PLAN"                      "$VM:/tmp/downsizing-plan.toml"
scp "$ROOT/deploy/install-on-vm.sh" "$VM:/tmp/install-on-vm.sh"
echo
echo "Done. Now on the VM run:   sudo bash /tmp/install-on-vm.sh"
