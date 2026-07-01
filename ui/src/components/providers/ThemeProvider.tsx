"use client";

import {
	ThemeProvider as NextThemesProvider,
	useTheme as useNextTheme,
} from "next-themes";

type Theme = "dark" | "light" | "system";

type ThemeProviderProps = {
	children: React.ReactNode;
	defaultTheme?: Theme;
	storageKey?: string;
};

export function ThemeProvider({
	children,
	defaultTheme = "system",
	storageKey = "paseo-downsizer-theme",
}: ThemeProviderProps) {
	return (
		<NextThemesProvider
			attribute="class"
			defaultTheme={defaultTheme}
			storageKey={storageKey}
			enableSystem
			disableTransitionOnChange
		>
			{children}
		</NextThemesProvider>
	);
}

export const useTheme = () => {
	const { theme, setTheme, resolvedTheme } = useNextTheme();

	return {
		theme: (theme as Theme) ?? "system",
		setTheme: (newTheme: Theme) => setTheme(newTheme),
		resolvedTheme: resolvedTheme as "dark" | "light" | undefined,
	};
};
