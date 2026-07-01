import { Dashboard } from "@/components/dashboard/Dashboard";

// Single dashboard route. The page shell is a Server Component; all live,
// auto-refreshing data is fetched client-side inside <Dashboard /> (polling the
// service /api/* and, optionally, the chain via PAPI).
export default function Page() {
	return <Dashboard />;
}
