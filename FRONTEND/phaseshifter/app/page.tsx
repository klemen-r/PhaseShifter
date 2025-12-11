"use client";
import "animate.css";
import { AppSidebar } from "@/components/AppSidebar";
import { CustomTrigger } from "@/components/customSideBarTrigger";
import { useSidebar } from "@/components/ui/sidebar";

import AnimatedHeadline from "@/components/AnimatedHeadline";

export default function Home() {
  const { open, toggleSidebar } = useSidebar();

  return (
    <div className="relative min-h-screen w-full font-sans">
      {/* Background */}
      <div className="absolute inset-0 -z-10 bg-[url('/bg4.png')] bg-cover bg-center" />

      {/* Soft contrast overlay */}
      <div className="absolute inset-0 -z-10 bg-black/35" />

      <div className="flex min-h-screen w-full bg-transparent">
        <AppSidebar />
        <CustomTrigger />

        <main
          className="flex flex-1 items-center justify-center"
          onClick={() => open && toggleSidebar()}
        >
          <AnimatedHeadline />
        </main>
      </div>
    </div>
  );
}
