# Deploying paseo-downsizer (systemd + Caddy)

Run the downsizer on a single hardened Linux VM: the Rust service (with the hot
proxy key) and the Next.js dashboard as systemd services, behind Caddy for
automatic HTTPS. Only 443 + SSH are exposed; the service API and UI bind to
localhost.

```
Internet ──443──> Caddy ─┬─ /api/*  ─> 127.0.0.1:8080  (service, holds PROXY_SURI)
                         └─ /*      ─> 127.0.0.1:3000  (dashboard)
```

> ⚠️ **This VM holds a key that can move sudo on Paseo for ~48h.** Treat it as
> high-value: SSH key-only, minimal packages, firewall to 443+22, and **destroy
> the box after shutdown** so the key doesn't linger.

## 0. Prerequisites

- A small VM (2 vCPU / 2 GB is plenty), Ubuntu/Debian.
- A DNS record (`downsizer.<domain>`) pointing at the VM's public IP.
- Node.js ≥ 20 and `pnpm` (for building/running the UI).
- Rust toolchain (to build the service) — or build it elsewhere and copy the binary.
- Caddy installed (`https://caddyserver.com/docs/install`).

## 1. Build

```bash
git clone git@github.com:r0gue-io/paseo-downsizer.git && cd paseo-downsizer

# service (release binary)
cargo build --release --manifest-path service/Cargo.toml
#   -> service/target/release/paseo-downsizer-service

# dashboard — NEXT_PUBLIC_SERVICE_URL="" makes the UI call /api same-origin
cd ui
NEXT_PUBLIC_SERVICE_URL="" pnpm install --frozen-lockfile
NEXT_PUBLIC_SERVICE_URL="" pnpm build
cd ..
```

## 2. Provision the VM

```bash
sudo useradd --system --home /var/lib/paseo-downsizer --shell /usr/sbin/nologin paseo
sudo mkdir -p /opt/paseo-downsizer /etc/paseo-downsizer /var/lib/paseo-downsizer
sudo chown paseo:paseo /var/lib/paseo-downsizer

# binary
sudo install -m 0755 service/target/release/paseo-downsizer-service /usr/local/bin/

# dashboard (built .next + node_modules + public + package.json)
sudo cp -r ui /opt/paseo-downsizer/ui
sudo chown -R paseo:paseo /opt/paseo-downsizer

# plan (the schedule the service drives toward)
sudo cp plan/downsizing-plan.toml /etc/paseo-downsizer/

# secrets file (edit it next)
sudo cp deploy/service.env.example /etc/paseo-downsizer/service.env
sudo chown root:root /etc/paseo-downsizer/service.env
sudo chmod 600 /etc/paseo-downsizer/service.env

# systemd units
sudo cp deploy/paseo-downsizer.service deploy/paseo-downsizer-ui.service /etc/systemd/system/
sudo systemctl daemon-reload

# Caddy (edit the domain first)
sudo cp deploy/Caddyfile /etc/caddy/Caddyfile
sudo $EDITOR /etc/caddy/Caddyfile   # set downsizer.<domain>
sudo systemctl reload caddy
```

## 3. Configure secrets

Edit `/etc/paseo-downsizer/service.env`:
- `SUDO_ACCOUNT` — the sudo key's ss58 the proxy acts for.
- `CONTROL_TOKEN` — `openssl rand -hex 32`.
- **Leave `PROXY_SURI` unset for now** (monitor-only until go-live).

## 4. Validate (monitor-only — cannot dispatch)

```bash
sudo systemctl enable --now paseo-downsizer.service paseo-downsizer-ui.service
sudo systemctl status paseo-downsizer          # "monitor-only mode: ... no calls dispatched"
curl -s localhost:8080/api/health              # {"status":"ok","dispatcher":"idle",...}
```

- Open `https://downsizer.<domain>` — the dashboard should show live Paseo state
  (152 validators, 56 cores, 31 paras) with `dispatcher: idle`.
- **Dry-run the real key against live chain** (does NOT submit) to confirm the
  encodings before you trust it — run it as a one-off with the key in the env:
  ```bash
  sudo RUST_LOG=info PROXY_SURI="<12-word mnemonic>" SUDO_ACCOUNT=<ss58> \
    /usr/local/bin/paseo-downsizer-service --plan /etc/paseo-downsizer/downsizing-plan.toml --dry-run
  ```
  Expect the per-item and `utility.batch_all` dry-runs to pass (or fail only on
  `Proxy::NotProxy` if that account isn't actually a delegate of the sudo key).

## 5. Go-live — Thursday 2 July 2026, 12:00 CEST

The service anchors its schedule clock (`start_era`) on first real run, so start
it **fresh** exactly at T0:

```bash
sudo systemctl stop paseo-downsizer.service
sudo rm -f /var/lib/paseo-downsizer/state.json      # clear the monitor-only anchor
sudoedit /etc/paseo-downsizer/service.env           # uncomment/set PROXY_SURI
sudo systemctl start paseo-downsizer.service
```

Step 1 (152→100) then fires at the next era boundary (~T+6h, Thu 18:00 CEST), each
step dry-run-gated and applied atomically. Floor (20) at Fri 12:00; shutdown
milestone at Sat 12:00.

## 6. Monitor & control

```bash
journalctl -u paseo-downsizer -f          # dispatch log, dry-runs, health pauses
```
- Dashboard: live progress, timeline countdowns, per-core packing, dispatch log.
- **Pause / resume** (if you need to hold the schedule):
  ```bash
  curl -X POST https://downsizer.<domain>/api/control \
    -H "authorization: Bearer $CONTROL_TOKEN" -d '{"action":"pause"}'
  ```

## 7. Shutdown & teardown

- At the shutdown milestone (Sat 4 Jul 12:00 CEST) the service logs the shutdown
  notice — it does **not** stop validators (that's the operators' coordinated
  action). The chain halts when nodes go offline.
- After the network is down: `sudo systemctl disable --now paseo-downsizer*`, then
  **destroy the VM** (it still holds the proxy key). Rotate/retire the delegate
  proxy on-chain if it won't be reused.

## Security checklist

- [ ] `service.env` is `root:root 600`; `PROXY_SURI` only set at go-live.
- [ ] Firewall allows only 443 + SSH (key-only); `:8080`/`:3000` are localhost.
- [ ] `CONTROL_TOKEN` set and long; never in the repo.
- [ ] VM destroyed after shutdown; delegate proxy retired if unused.
