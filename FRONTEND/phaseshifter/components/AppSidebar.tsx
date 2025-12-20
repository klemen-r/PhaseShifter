"use client";

import Link from "next/link";
import { useEffect, useMemo, useState } from "react";
import { usePathname } from "next/navigation";
import { Folder, Pencil, Plus, Trash2 } from "lucide-react";

import {
  Sidebar,
  SidebarHeader,
  SidebarContent,
  SidebarGroup,
  SidebarGroupLabel,
  SidebarGroupContent,
  SidebarMenu,
  SidebarMenuItem,
  SidebarMenuButton,
  SidebarSeparator,
} from "@/components/ui/sidebar";
import { ScrollArea } from "@/components/ui/scroll-area";
import { ThemeToggle } from "@/components/ThemeToggle";
import { NewPath } from "./newPath";
import { useSidebar } from "@/components/ui/sidebar";
import { cn } from "@/lib/utils"; // if you don't have this, tell me and I'll paste it

type PathEntry = { id: string; path: string };

export function AppSidebar() {
  const pathname = usePathname();
  const { toggleSidebar } = useSidebar();

  const [open, setOpen] = useState(false);
  const [paths, setPaths] = useState<PathEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [editMode, setEditMode] = useState(false);

  async function deletePath(id: string) {
    try {
      const res = await fetch("/api/paths", {
        method: "DELETE",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ patId: id }),
      });
      if (res.ok) {
        loadPaths();
      }
    } catch (e) {
      console.error(e);
    }
  }

  async function loadPaths() {
    try {
      setLoading(true);
      const res = await fetch("/api/paths", { cache: "no-store" });
      if (!res.ok) return;
      const data = (await res.json()) as PathEntry[];
      setPaths(data);
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadPaths();
  }, []);

  // normalize href + active check
  const items = useMemo(() => {
    const withHome: PathEntry[] = [{ id: "home", path: "home" }, ...paths];
    return withHome.map((entry) => {
      const href =
        entry.id === "home"
          ? "/"
          : entry.path.startsWith("/")
            ? entry.path
            : `/${entry.path}`;
      const active = pathname === href;
      return { ...entry, href, active };
    });
  }, [paths, pathname]);

  return (
    <Sidebar className="border-r border-zinc-800/60 bg-zinc-950/60 backdrop-blur">
      {/* Header */}
      <SidebarHeader className="px-3 py-3">
        <div className="text-sm font-semibold tracking-wide text-zinc-200">
          PhaseShifter
        </div>
        <div className="text-xs text-zinc-400">Navigation</div>
      </SidebarHeader>

      <SidebarContent className="gap-2">
        {/* Paths group */}
        <SidebarGroup>
          <div className="flex items-center justify-between px-2">
            <SidebarGroupLabel className="text-zinc-400">
              Paths
            </SidebarGroupLabel>
            <button
              onClick={() => setEditMode(!editMode)}
              className={cn(
                "p-1 rounded-md transition-colors",
                "hover:bg-zinc-800/60",
                editMode && "bg-zinc-800/80 text-zinc-200",
              )}
              title={editMode ? "Done editing" : "Edit paths"}
            >
              <Pencil className="h-3.5 w-3.5" />
            </button>
          </div>

          <SidebarGroupContent>
            <ScrollArea className="h-[45vh] pr-2">
              <SidebarMenu className="px-1">
                {loading && (
                  <div className="px-3 py-2 text-xs text-zinc-500">
                    Loading paths…
                  </div>
                )}

                {!loading && items.length === 0 && (
                  <div className="px-3 py-2 text-xs text-zinc-500">
                    No paths yet.
                  </div>
                )}

                {items.map((entry) => (
                  <SidebarMenuItem key={entry.id}>
                    <SidebarMenuButton
                      asChild
                      isActive={entry.active}
                      className={cn(
                        "h-9 gap-2 rounded-md px-3",
                        "transition-colors",
                        "hover:bg-zinc-800/60 dark:hover:bg-zinc-800/60",
                        entry.active &&
                          "bg-zinc-800/70 ring-1 ring-zinc-600/60",
                      )}
                    >
                      <Link
                        href={entry.href}
                        onClick={editMode ? (e) => e.preventDefault() : toggleSidebar}
                        className="flex items-center gap-2 w-full"
                      >
                        <Folder className="h-4 w-4 opacity-80 shrink-0" />
                        <span className="truncate flex-1">
                          {entry.id === "home" ? "home" : entry.path}
                        </span>
                        {editMode && entry.id !== "home" && (
                          <button
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              deletePath(entry.id);
                            }}
                            className={cn(
                              "p-1 rounded-md transition-colors shrink-0",
                              "hover:bg-red-500/20 hover:text-red-400",
                              "text-zinc-500",
                            )}
                            title={`Delete ${entry.path}`}
                          >
                            <Trash2 className="h-4 w-4" />
                          </button>
                        )}
                      </Link>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                ))}
              </SidebarMenu>
            </ScrollArea>

            {/* Add path button */}
            <div className="px-2 pt-2">
              <SidebarMenu>
                <SidebarMenuItem>
                  <SidebarMenuButton
                    onClick={() => setOpen(true)}
                    className={cn(
                      "h-9 gap-2 rounded-md px-3",
                      "bg-transparent",
                      "hover:bg-zinc-800/60 transition-colors",
                    )}
                  >
                    <Plus className="h-4 w-4" />
                    <span>Add path</span>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              </SidebarMenu>
            </div>
          </SidebarGroupContent>
        </SidebarGroup>

        <SidebarSeparator className="bg-zinc-800/60" />

        {/* Theme group */}
        <SidebarGroup>
          <SidebarGroupLabel className="px-2 text-zinc-400">
            Theme
          </SidebarGroupLabel>
          <SidebarGroupContent className="px-2">
            <ThemeToggle />
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>

      {/* Dialog */}
      <NewPath open={open} onOpenChange={setOpen} onCreated={loadPaths} />
    </Sidebar>
  );
}
