import type { Metadata, Viewport } from "next";
import { Geist, Geist_Mono } from "next/font/google";
import { ThemeProvider } from "@/components/providers/ThemeProvider";
import { TooltipProvider } from "@/components/ui/tooltip";
import "./globals.css";

const geistSans = Geist({
	variable: "--font-geist-sans",
	subsets: ["latin"],
});

const geistMono = Geist_Mono({
	variable: "--font-geist-mono",
	subsets: ["latin"],
});

export const metadata: Metadata = {
	title: "Paseo Downsizer — Live Dashboard",
	description:
		"Live progress, step timeline, finality health, core packing map and dispatch log for the controlled downsizing of the Paseo relay chain.",
	keywords: ["Paseo", "Polkadot", "Downsizing", "Validators", "Coretime"],
};

export const viewport: Viewport = {
	colorScheme: "light dark",
};

export default function RootLayout({
	children,
}: Readonly<{
	children: React.ReactNode;
}>) {
	return (
		<html lang="en" suppressHydrationWarning>
			<body
				className={`${geistSans.variable} ${geistMono.variable} antialiased`}
			>
				<ThemeProvider defaultTheme="system">
					<TooltipProvider delayDuration={200}>{children}</TooltipProvider>
				</ThemeProvider>
			</body>
		</html>
	);
}
