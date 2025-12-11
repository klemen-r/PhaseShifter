"use client";
import { useSidebar } from "@/components/ui/sidebar";
import { PanelLeft } from "lucide-react";
export function CustomTrigger() {
  const { toggleSidebar } = useSidebar();

  return (
    <div className="sticky top-0 h-8 w-8 ml-4 mt-4 hover:bg-neutral-700 rounded-sm animate__animated animate__fadeInLeft">
      <button
        onClick={toggleSidebar}
        className="text-neutral-950 dark:text-zinc-50 w-full h-full flex items-center justify-center"
      >
        <PanelLeft size={20}></PanelLeft>
      </button>
    </div>
  );
}
