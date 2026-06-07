import { Link } from "@tanstack/react-router";
import {
  ChevronLeft,
  ChevronRight,
  FileText,
  FolderOpen,
  Library,
  MessageSquareText,
  Search,
  Settings,
  UploadCloud,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";

type SidebarItem = {
  to: "/folders" | "/upload" | "/library" | "/search" | "/chat" | "/settings";
  label: string;
  icon: LucideIcon;
};

const sidebarItems: SidebarItem[] = [
  { to: "/folders", label: "Folders", icon: FolderOpen },
  { to: "/upload", label: "Upload", icon: UploadCloud },
  { to: "/library", label: "Library", icon: Library },
  { to: "/search", label: "Search", icon: Search },
  { to: "/chat", label: "Chat", icon: MessageSquareText },
  { to: "/settings", label: "Settings", icon: Settings },
];

export function Sidebar() {
  const collapsed = useUiStore((state) => state.sidebarCollapsed);
  const toggleSidebar = useUiStore((state) => state.toggleSidebar);

  return (
    <aside
      className={cn(
        "flex shrink-0 flex-col border-r border-sidebar-border bg-sidebar/95 transition-[width] duration-200",
        collapsed ? "w-[4.25rem]" : "w-60",
      )}
    >
      <div data-tauri-drag-region className="flex h-14 items-center justify-between px-3">
        {!collapsed && (
          <div data-tauri-drag-region className="flex items-center gap-2">
            <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-border bg-background/60 text-foreground shadow-sm shadow-black/20">
              <FileText className="size-4" />
            </span>
            <span className="text-lg font-semibold tracking-[-0.04em] text-foreground">
              PDF<span className="text-muted-foreground">-Parser</span>
            </span>
          </div>
        )}
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          className="ml-auto rounded-md text-muted-foreground"
          onClick={toggleSidebar}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {collapsed ? <ChevronRight /> : <ChevronLeft />}
        </Button>
      </div>
      <nav className="flex flex-1 flex-col gap-1 px-2 py-2">
        {sidebarItems.map((item) => {
          const Icon = item.icon;
          return (
            <Link
              key={item.to}
              to={item.to}
              title={collapsed ? item.label : undefined}
              activeProps={{ "data-active": true }}
              className={cn(
                "group flex h-10 items-center gap-3 rounded-md px-2 text-[0.95rem] text-sidebar-foreground/70 transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-[active=true]:bg-sidebar-accent data-[active=true]:text-sidebar-accent-foreground",
                collapsed && "justify-center",
              )}
            >
              <Icon className="size-[1.15rem] shrink-0" />
              {!collapsed && <span>{item.label}</span>}
            </Link>
          );
        })}
      </nav>
    </aside>
  );
}
