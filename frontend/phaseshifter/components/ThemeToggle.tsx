"use client";

import { useEffect, useState } from "react";
import { useTheme } from "next-themes";
import { cn } from "@/lib/utils";
import { Sun, Moon } from "lucide-react";
import "animate.css";
type Props = {
  className?: string;
};

export function ThemeToggle({ className }: Props) {
  const { resolvedTheme, setTheme } = useTheme();
  const isDark = resolvedTheme === "dark";
  const [mounted, setMounted] = useState(false);

  useEffect(() => setMounted(true), []);
  if (!mounted) return null; // Avoid hydration mismatch when reading theme

  return (
    <div className="ml-4">
      <button
        type="button"
        role="switch"
        aria-checked={isDark}
        aria-label="Toggle theme"
        onClick={() => setTheme(isDark ? "light" : "dark")}
        className={cn(
          "relative inline-flex h-6 w-14 items-center rounded-full",
          "transition-colors duration-300",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/50",
          isDark
            ? "bg-neutral-900  border-2 border-zinc-50"
            : "bg-neutral-200 border-2 border-neutral-900",
          className,
        )}
      >
        <span
          className={cn(
            "pointer-events-none absolute left-1 top-1 h-6 w-6 rounded-full",
            "transition-transform duration-300 will-change-transform shadow-md",
            isDark ? "translate-x-6 bg-transparent" : "translate-x-0 ",
          )}
        >
          {isDark ? (
            <Sun
              size={18}
              className="text-zinc-50 -translate-y-0.5 translate-x-1"
            />
          ) : (
            <Moon size={18} className="border-neutral-900 -translate-y-0.5" />
          )}
        </span>
      </button>
    </div>
  );
}
