import type { NextConfig } from "next";

const nextConfig: NextConfig = {
	// The dashboard reads live state from the Rust service (NEXT_PUBLIC_SERVICE_URL)
	// and, optionally, directly from the chain via PAPI websockets. No server-side
	// data fetching is required, so the defaults are sufficient.
	reactStrictMode: true,
};

export default nextConfig;
