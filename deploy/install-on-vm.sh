#!/usr/bin/env bash
# Install the artifacts that push-artifacts.sh dropped in /tmp, then restart.
# Run on the VM:  sudo bash /tmp/install-on-vm.sh
#
# Safe to re-run (idempotent). Swaps only the binary, dashboard and plan; it does
# NOT touch service.env (secrets) or the Caddyfile (your domain). It does NOT
# delete state.json — the service self-clears its schedule anchor while holding
# for start_at, so binary swaps re-anchor cleanly on their own.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "run with sudo: sudo bash /tmp/install-on-vm.sh"; exit 1; }

BIN=/tmp/paseo-downsizer-service
DASH=/tmp/dashboard
PLAN=/tmp/downsizing-plan.toml

# First-run provisioning (no-ops if already done). Secrets/unit/Caddy are set up
# once per deploy/README.md and are left untouched here.
id paseo &>/dev/null || useradd --system --home /var/lib/paseo-downsizer --shell /usr/sbin/nologin paseo
mkdir -p /etc/paseo-downsizer /var/lib/paseo-downsizer /var/www/paseo-downsizer
chown paseo:paseo /var/lib/paseo-downsizer

echo "== stopping service =="
systemctl stop paseo-downsizer 2>/dev/null || true

if [[ -f "$BIN" ]]; then
  install -m0755 "$BIN" /usr/local/bin/paseo-downsizer-service
  echo "binary   -> /usr/local/bin/paseo-downsizer-service ($(sha256sum "$BIN" | awk '{print $1}'))"
fi
if [[ -d "$DASH" ]]; then
  rm -rf /var/www/paseo-downsizer/*
  cp -r "$DASH"/* /var/www/paseo-downsizer/
  echo "dashboard-> /var/www/paseo-downsizer"
fi
if [[ -f /tmp/providers.toml ]]; then cp /tmp/providers.toml /etc/paseo-downsizer/providers.toml; echo "providers -> /etc/paseo-downsizer/providers.toml"; fi
if [[ -f "$PLAN" ]]; then
  cp "$PLAN" /etc/paseo-downsizer/downsizing-plan.toml
  echo "plan     -> /etc/paseo-downsizer/downsizing-plan.toml"
fi

echo "== starting service =="
systemctl start paseo-downsizer
sleep 3

echo "== status =="
systemctl --no-pager --lines=0 status paseo-downsizer | sed -n '1,4p' || true
echo -n "startsAt: "; curl -s localhost:8080/api/plan | grep -o '"startsAt":"[^"]*"' || echo "(not set)"
echo -n "startEra (should be null until go-live): "
curl -s localhost:8080/api/plan >/dev/null 2>&1
grep -o '"startEra":[^,]*' /var/lib/paseo-downsizer/state.json 2>/dev/null || echo "(no state yet)"
echo "Done."
