"use client";

import { ThemeProvider } from "next-themes";
import { SidebarProvider } from "@/components/ui/sidebar";
import { WebSocketProvider, TradingDataProvider } from "@/lib/websocket";

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <WebSocketProvider defaultUrl="ws://localhost:8000" autoConnect={true}>
        <TradingDataProvider>
          <SidebarProvider defaultOpen={false}>{children}</SidebarProvider>
        </TradingDataProvider>
      </WebSocketProvider>
    </ThemeProvider>
  );
}
