"use client";

import { ThemeProvider } from "next-themes";
import { SidebarProvider } from "@/components/ui/sidebar";
import { WebSocketProvider, TradingDataProvider } from "@/lib/websocket";

// Server URLs for dual-connection architecture
// NQ/ES → Rust server (port 8000)
// Other symbols → Python server (port 8001)
const RUST_SERVER_URL = "ws://localhost:8000";
const PYTHON_SERVER_URL = "ws://localhost:8001";

export function Providers({ children }: { children: React.ReactNode }) {
  return (
    <ThemeProvider attribute="class" defaultTheme="system" enableSystem>
      <WebSocketProvider defaultUrl={RUST_SERVER_URL} autoConnect={true}>
        <TradingDataProvider pythonServerUrl={PYTHON_SERVER_URL}>
          <SidebarProvider defaultOpen={false}>{children}</SidebarProvider>
        </TradingDataProvider>
      </WebSocketProvider>
    </ThemeProvider>
  );
}
