# Deploying paseo-downsizer (systemd + Caddy)

Run the downsizer on a single hardened Linux VM: the Rust service (with the hot
proxy key) as a systemd service, and the dashboard as a **static site**, both
behind Caddy for automatic HTTPS. Only 443 + SSH are exposed; the service API
binds to localhost. **The VM needs neither Rust nor Node** — you build on a
separate host and copy two artifacts over.

```
Internet ──443──> Caddy ─┬─ /api/*  ─> 127.0.0.1:8080         (service, holds PROXY_SURI)
                         └─ /*      ─> /var/www/paseo-downsizer (static dashboard)
```

> ⚠️ **This VM holds a key that can move sudo on Paseo for ~48h.** Treat it as
> high-value: SSH key-only, minimal packages, firewall to 443+22, and **destroy
> the box after shutdown** so the key doesn't linger.

## 0. Prerequisites

- A small VM (1 vCPU / 2 GB / 10 GB), Ubuntu 22.04+ / Debian 12+ (glibc ≥ 2.34),
  public IP. **Only Caddy** is installed on it.
- A DNS record (`downsizer.<domain>`) → the VM's public IP.
- A **build host** (your laptop / CI, x86_64 Linux) with the Rust toolchain and
  Node ≥ 20 + pnpm — used once to produce the two artifacts.

## 1. Build (on the build host)

```bash
git clone git@github.com:r0gue-io/paseo-downsizer.git && cd paseo-downsizer

# service — stripped release binary (~11 MB; needs glibc >= 2.34 on the VM)
cargo build --release --manifest-path service/Cargo.toml
strip service/target/release/paseo-downsizer-service

# dashboard — static export; NEXT_PUBLIC_SERVICE_URL="" => same-origin /api
( cd ui && NEXT_PUBLIC_SERVICE_URL="" pnpm install --frozen-lockfile \
        && NEXT_PUBLIC_SERVICE_URL="" pnpm build )     # -> ui/out (~1 MB static site)
```

## 2. Copy artifacts to the VM

One command from the build host copies the binary, static dashboard, plan, and
the installer:
```bash
deploy/push-artifacts.sh USER@VM
```
(First deploy only: also `scp deploy/paseo-downsizer.service deploy/Caddyfile
deploy/service.env.example USER@VM:/tmp/` for the one-time systemd/Caddy/secrets
setup below.)

## 3. Install on the VM

```bash
# Caddy (if not already installed) — see https://caddyserver.com/docs/install
sudo useradd --system --home /var/lib/paseo-downsizer --shell /usr/sbin/nologin paseo
sudo mkdir -p /etc/paseo-downsizer /var/lib/paseo-downsizer /var/www/paseo-downsizer
sudo chown paseo:paseo /var/lib/paseo-downsizer

# service binary + static dashboard (verify the binary's checksum first)
sudo install -m0755 /tmp/paseo-downsizer-service /usr/local/bin/
/usr/local/bin/paseo-downsizer-service --help >/dev/null && echo "binary runs OK"
sudo cp -r /tmp/dashboard/* /var/www/paseo-downsizer/

# plan, secrets, unit, Caddy
sudo cp /tmp/downsizing-plan.toml /etc/paseo-downsizer/
sudo cp /tmp/service.env.example /etc/paseo-downsizer/service.env
sudo chown root:root /etc/paseo-downsizer/service.env && sudo chmod 600 /etc/paseo-downsizer/service.env
sudo cp /tmp/paseo-downsizer.service /etc/systemd/system/ && sudo systemctl daemon-reload
sudo cp /tmp/Caddyfile /etc/caddy/Caddyfile
sudo nano /etc/caddy/Caddyfile          # set downsizer.<domain>
sudo systemctl reload caddy
```

### Updating later (one command each)
After the first-time setup, every redeploy is just:
```bash
# build host — build (if changed) then push:
deploy/push-artifacts.sh USER@VM
# VM — swap binary/dashboard/plan in place + restart (leaves secrets/Caddy alone):
sudo bash /tmp/install-on-vm.sh
```

## 4. Configure secrets

Edit `/etc/paseo-downsizer/service.env`:
- `SUDO_ACCOUNT` — the sudo key's ss58 the proxy acts for.
- `CONTROL_TOKEN` — `openssl rand -hex 32`.
- **Leave `PROXY_SURI` unset for now** (monitor-only until go-live).

## 5. Firewall + start monitor-only + validate

```bash
sudo ufw allow 22/tcp && sudo ufw allow 80,443/tcp && sudo ufw --force enable
sudo systemctl enable --now paseo-downsizer.service
sudo systemctl status paseo-downsizer          # "monitor-only mode: ... no calls dispatched"
curl -s localhost:8080/api/health              # {"status":"ok","dispatcher":"idle",...}
```

- Open `https://downsizer.<domain>` — the dashboard should show live Paseo state
  (152 validators, 56 cores, 31 paras) with `dispatcher: idle`.
- **Dry-run the real key against live chain** (does NOT submit) to confirm the
  encodings before you trust it:
  ```bash
  sudo RUST_LOG=info PROXY_SURI="<12-word mnemonic>" SUDO_ACCOUNT=<ss58> \
    /usr/local/bin/paseo-downsizer-service --plan /etc/paseo-downsizer/downsizing-plan.toml --dry-run
  ```
  Expect the per-item and `utility.batch_all` dry-runs to pass (or fail only on
  `Proxy::NotProxy` if that account isn't actually a delegate of the sudo key).

## 6. Arm it — go-live is automatic at `start_at`

The plan's `start_at` (`2026-07-02T12:00:00+02:00`) makes go-live **hands-off**:
the service arms immediately, holds (no chain writes) until that instant, then
anchors the schedule and begins — no one needs to be at the keyboard. Do this any
time before Thursday noon:

```bash
sudoedit /etc/paseo-downsizer/service.env           # set PROXY_SURI
sudo systemctl restart paseo-downsizer.service
```
(No `state.json` cleanup needed: while holding for `start_at` the service
actively clears any stale schedule anchor and re-anchors fresh at go-live, so
restarts and binary swaps are safe.)

The dashboard now shows **"Armed · go-live in HH:MM:SS"**. At `start_at` it anchors
and fires step 1 at +6h; each step is dry-run-gated and applied atomically. To
change the go-live time, edit `start_at` in
`/etc/paseo-downsizer/downsizing-plan.toml` and restart. **Kill switch:** pause any
time with `POST /api/control {"action":"pause"}` or `sudo systemctl stop`.

Floor (20) at Fri 12:00; shutdown
milestone at Sat 12:00.

## 7. Monitor & control

```bash
journalctl -u paseo-downsizer -f          # dispatch log, dry-runs, health pauses
```
- Dashboard: live progress, timeline countdowns, per-core packing, dispatch log.
- **Pause / resume** (if you need to hold the schedule):
  ```bash
  curl -X POST https://downsizer.<domain>/api/control \
    -H "authorization: Bearer $CONTROL_TOKEN" -d '{"action":"pause"}'
  ```

## 8. Shutdown & teardown

- At the shutdown milestone (Sat 4 Jul 12:00 CEST) the service logs the shutdown
  notice — it does **not** stop validators (that's the operators' coordinated
  action). The chain halts when nodes go offline.
- After the network is down: `sudo systemctl disable --now paseo-downsizer`, then
  **destroy the VM** (it still holds the proxy key). Rotate/retire the delegate
  proxy on-chain if it won't be reused.

## Security checklist

- [ ] `service.env` is `root:root 600`; `PROXY_SURI` only set at go-live.
- [ ] Firewall allows only 443 + SSH (key-only); `:8080` is localhost-only.
- [ ] `CONTROL_TOKEN` set and long; never in the repo.
- [ ] VM destroyed after shutdown; delegate proxy retired if unused.
