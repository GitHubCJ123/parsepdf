import { Link } from "@tanstack/react-router";
import {
  ChevronLeft,
  ChevronRight,
  Inbox,
  Library,
  MessageSquareText,
  Search,
  Settings,
  type LucideIcon,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";

type SidebarItem = {
  to: "/inbox" | "/library" | "/search" | "/chat" | "/settings";
  label: string;
  icon: LucideIcon;
};

const sidebarItems: SidebarItem[] = [
  { to: "/inbox", label: "Inbox", icon: Inbox },
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
      <div className="flex h-14 items-center justify-between px-3">
        {!collapsed && (
          <div className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
            Local library
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
                "group flex h-9 items-center gap-3 rounded-md px-2 text-sm text-sidebar-foreground/70 transition-colors hover:bg-sidebar-accent hover:text-sidebar-accent-foreground data-[active=true]:bg-sidebar-accent data-[active=true]:text-sidebar-accent-foreground",
                collapsed && "justify-center",
              )}
            >
              <Icon className="size-4 shrink-0" />
              {!collapsed && <span>{item.label}</span>}
            </Link>
          );
        })}
      </nav>
    </aside>
  );
}
