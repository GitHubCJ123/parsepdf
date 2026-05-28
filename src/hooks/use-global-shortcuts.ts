
import { useEffect } from "react";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import { useUiStore } from "@/stores/ui-store";

export function useGlobalShortcuts() {
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (state) => state.location.pathname });
  const toggleCommandPalette = useUiStore((state) => state.toggleCommandPalette);
  const openShortcutHelp = useUiStore((state) => state.openShortcutHelp);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.metaKey || event.altKey) return;
      const key = event.key.toLowerCase();

      if (key === "k") {
        event.preventDefault();
        toggleCommandPalette();
        return;
      }

      if (key === ",") {
        event.preventDefault();
        void navigate({ to: "/settings" });
        return;
      }

      if (key === "/") {
        event.preventDefault();
        const focused = focusSearchInput();
        if (!focused) {
          void navigate({ to: "/search" }).then(() => window.setTimeout(focusSearchInput, 40));
        }
        return;
      }

      if (key === "n" && pathname === "/chat") {
        event.preventDefault();
        window.dispatchEvent(new CustomEvent("pdf-parser:new-chat"));
        return;
      }

      if (event.key === "?" || (event.shiftKey && key === "/")) {
        event.preventDefault();
        openShortcutHelp();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate, openShortcutHelp, pathname, toggleCommandPalette]);
}

function focusSearchInput() {
  const input = document.querySelector<HTMLInputElement>("[data-global-search='true']");
  if (!input) return false;
  input.focus();
  input.select();
  return true;
}
