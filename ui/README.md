# paseo-downsizer · ui

Live dashboard for the controlled downsizing of the Paseo relay. Next.js 16
(App Router / RSC) + React 19 + Tailwind v4 + shadcn/ui (new-york, neutral) +
next-themes + Geist fonts + Biome. Visually matches `paseo-website`.

## Run

```bash
pnpm install
cp .env.example .env.local        # set NEXT_PUBLIC_SERVICE_URL etc.
pnpm dev                          # http://localhost:3000
```

Scripts: `pnpm dev` · `pnpm build` · `pnpm start` · `pnpm check` (Biome) ·
`pnpm typecheck` (`tsc --noEmit`).

## Data flow — source of truth = on-chain state + planned tasks

The dashboard polls the Rust service (`NEXT_PUBLIC_SERVICE_URL`, default
`http://localhost:8080`) every ~6s:

| Endpoint | Panel(s) |
|---|---|
| `GET /api/state` | headline progress, finality health, current core packing |
| `GET /api/plan` | step timeline, next-step computed packing |
| `GET /api/history` | dispatch log |
| `GET /api/health` | banner + dispatcher badge |

Types in `src/lib/types.ts` match the SPEC.md JSON shapes exactly. Polling lives
in `src/hooks/usePoll.ts`; it keeps the last good data on transient failures and
surfaces a "service unreachable" banner + skeletons. All countdowns
(`src/hooks/useCountdown.ts`) tick live to the next era boundary.

## Chain as source of truth (PAPI)

`src/lib/papi/client.ts` wires a [polkadot-api](https://papi.how) client for the
relay + Asset Hub so live counters can be read **directly from the chain**.

- **Default: OFF** — the UI reads everything from the service `/api/state`, which
  is itself derived from live chain reads by the Rust service.
- **Enable:** set `NEXT_PUBLIC_USE_PAPI=1` (+ `NEXT_PUBLIC_RELAY_WS` /
  `NEXT_PUBLIC_AH_WS`) and ensure `polkadot-api` is installed.

### Why no `papi add` descriptors are committed

`papi add` generates typed descriptor bundles from live metadata into `.papi/`.
That codegen needs network + the `papi` CLI and is impractical in a build
sandbox, so no generated files are committed. The module instead uses the
codegen-less `client.getUnsafeApi()` runtime API, which reads
storage/constants/runtime-APIs dynamically off the live metadata. To move to the
typed API later:

```bash
pnpm papi add paseo -n paseo
pnpm papi add pah   -n paseo_asset_hub
# then swap getUnsafeApi() for getTypedApi(paseo) in src/lib/papi/client.ts
```

The `polkadot-api` package is an **optionalDependency** and is imported via a
non-literal dynamic specifier, so `pnpm typecheck` / `pnpm build` pass whether or
not it is installed.

## Design system

`src/app/globals.css` copies the paseo-website design-token block verbatim
(`:root` + `.dark`, `--chart-1..5` indigo/blue, `--radius: 0.625rem`, shadow
scale, Geist font vars). shadcn primitives under `src/components/ui/` are the
new-york variants. Charts (the online-fraction gauge, share bars) are plain SVG
using the `--chart-*` CSS variables.
