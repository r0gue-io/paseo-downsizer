"use client";

import { Monitor, Moon, Sun } from "lucide-react";
import { useEffect, useState } from "react";
import { useTheme } from "@/components/providers/ThemeProvider";
import { Button } from "@/components/ui/button";

const ORDER = ["light", "dark", "system"] as const;

export function ThemeToggle() {
	const { theme, setTheme } = useTheme();
	const [mounted, setMounted] = useState(false);
	useEffect(() => setMounted(true), []);

	// Avoid hydration mismatch: render a stable placeholder until mounted.
	if (!mounted) {
		return (
			<Button variant="outline" size="icon" aria-label="Toggle theme" disabled>
				<Sun />
			</Button>
		);
	}

	const cycle = () => {
		const idx = ORDER.indexOf(theme as (typeof ORDER)[number]);
		setTheme(ORDER[(idx + 1) % ORDER.length]);
	};

	const Icon = theme === "dark" ? Moon : theme === "system" ? Monitor : Sun;

	return (
		<Button
			variant="outline"
			size="icon"
			onClick={cycle}
			aria-label={`Theme: ${theme}. Click to change.`}
			title={`Theme: ${theme}`}
		>
			<Icon />
		</Button>
	);
}
