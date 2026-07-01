import type { NextConfig } from "next";

const nextConfig: NextConfig = {
	// The dashboard reads live state from the Rust service (NEXT_PUBLIC_SERVICE_URL)
	// and, optionally, directly from the chain via PAPI websockets. No server-side
	// data fetching is required — so we ship it as a fully static site (`out/`),
	// served directly by Caddy: no Node runtime on the deploy host, tiny footprint.
	output: "export",
	images: { unoptimized: true },
	reactStrictMode: true,
};

export default nextConfig;
