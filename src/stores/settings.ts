import { defineStore } from "pinia";
import { ipc } from "@/lib/ipc";
import { Events, listen } from "@/lib/events";
import type { Pod, Settings, ThemeMode } from "@/types";

function resolvedTheme(mode: ThemeMode): "light" | "dark" {
  if (mode !== "system") return mode;
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

export const useSettingsStore = defineStore("settings", {
  state: () => ({
    settings: null as Settings | null,
    monitors: [] as import("@/types").MonitorInfo[],
    dark: false,
  }),

  getters: {
    configured: (s) => !!s.settings?.pods.length,
  },

  actions: {
    async load() {
      const boot = await ipc.getBootstrap();
      this.monitors = boot.monitors;
      this.apply(boot.settings);
    },

    apply(settings: Settings) {
      this.settings = settings;
      this.dark = resolvedTheme(settings.theme) === "dark";
      document.documentElement.classList.toggle("dark", this.dark);
    },

    async refreshPods() {
      const boot = await ipc.getBootstrap();
      this.monitors = boot.monitors;
      this.apply(boot.settings);
    },

    async save(patch: Partial<Settings>) {
      const next = await ipc.saveSettings(patch);
      this.apply(next);
      return next;
    },

    pod(id: number): Pod | undefined {
      return this.settings?.pods.find((p) => p.id === id);
    },

    watchSystemTheme() {
      window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
        if (this.settings && this.settings.theme === "system") {
          this.dark = resolvedTheme("system") === "dark";
          document.documentElement.classList.toggle("dark", this.dark);
        }
      });
    },

    listenChanges() {
      listen<Settings>(Events.SettingsChanged, (settings) => this.apply(settings));
      listen<void>(Events.PodsChanged, () => void this.refreshPods());
    },
  },
});
