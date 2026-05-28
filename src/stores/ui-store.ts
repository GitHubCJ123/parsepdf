import { create } from "zustand";

type UiState = {
  sidebarCollapsed: boolean;
  commandPaletteOpen: boolean;
  shortcutHelpOpen: boolean;
  toggleSidebar: () => void;
  setCommandPaletteOpen: (open: boolean) => void;
  openCommandPalette: () => void;
  closeCommandPalette: () => void;
  toggleCommandPalette: () => void;
  setShortcutHelpOpen: (open: boolean) => void;
  openShortcutHelp: () => void;
};

export const useUiStore = create<UiState>((set) => ({
  sidebarCollapsed: false,
  commandPaletteOpen: false,
  shortcutHelpOpen: false,
  toggleSidebar: () =>
    set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
  setCommandPaletteOpen: (open) => set({ commandPaletteOpen: open }),
  openCommandPalette: () => set({ commandPaletteOpen: true }),
  closeCommandPalette: () => set({ commandPaletteOpen: false }),
  toggleCommandPalette: () =>
    set((state) => ({ commandPaletteOpen: !state.commandPaletteOpen })),
  setShortcutHelpOpen: (open) => set({ shortcutHelpOpen: open }),
  openShortcutHelp: () => set({ shortcutHelpOpen: true }),
}));
