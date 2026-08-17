import { defineStore } from "pinia";
import { ipc } from "@/lib/ipc";
import { Events, listen } from "@/lib/events";
import type { Settings, ThemeMode } from "@/types";

function resolvedTheme(mode: ThemeMode): "light" | "dark" {
  if (mode !== "system") return mode;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    settings: null as Settings | null,
    dark: false,
  }),

  getters: {
    configured: (s) => !!s.settings?.stagingFolder,
    activeItemsCount: (s) => s.settings,
  },

  actions: {
    async load() {
      const boot = await ipc.getBootstrap();
      this.apply(boot.settings);
    },

    apply(settings: Settings) {
      this.settings = settings;
      this.dark = resolvedTheme(settings.theme);
      document.documentElement.classList.toggle("dark", this.dark);
    },

    async save(patch: Partial<Settings>) {
      const next = await ipc.saveSettings(patch);
      this.apply(next);
      return next;
    },

    watchSystemTheme() {
      window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
        if (this.settings && this.settings.theme === "system") {
          this.dark = resolvedTheme("system");
          document.documentElement.classList.toggle("dark", this.dark);
        }
      });
    },

    listenChanges() {
      listen<Settings>(Events.SettingsChanged, (settings) => this.apply(settings));
    },
  },
});
