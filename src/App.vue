<script setup lang="ts">
/**
 * 多窗口共享一个 Vue 入口：按 Tauri 窗口 label 选择视图。
 * 浏览器开发时用 location.hash（#/bar /#/panel /#/settings）。
 */
import { computed, onMounted, ref } from "vue";
import BarWindow from "@/windows/BarWindow.vue";
import PanelWindow from "@/windows/PanelWindow.vue";
import SettingsWindow from "@/windows/SettingsWindow.vue";

const label = ref<string>("bar");

onMounted(async () => {
  if ("__TAURI_INTERNALS__" in window) {
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    label.value = getCurrentWebviewWindow().label;
  } else {
    label.value = location.hash.replace(/^#\/?/, "") || "bar";
  }
});

const view = computed(() => {
  switch (label.value) {
    case "panel":
      return PanelWindow;
    case "settings":
      return SettingsWindow;
    default:
      return BarWindow;
  }
});
</script>

<template>
  <component :is="view" />
</template>
