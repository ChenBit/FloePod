<script setup lang="ts">
/**
 * 多窗口共享一个 Vue 入口：按 Tauri 窗口 label 选择视图。
 * - settings          -> 设置 / OOBE
 * - pod_{id}          -> 匣的胶囊条（贴在屏幕边缘）
 * - pod_{id}_panel    -> 匣的弹出面板
 * 浏览器开发时用 location.hash（#/settings /#/pod_1 /#/pod_1_panel）。
 */
import { computed, onMounted, ref } from "vue";
import PodBar from "@/windows/PodBar.vue";
import PodPanel from "@/windows/PodPanel.vue";
import SettingsWindow from "@/windows/SettingsWindow.vue";

const label = ref<string>("pod_1");
const podId = ref<number>(1);
const isPanel = ref(false);

onMounted(async () => {
  if ("__TAURI_INTERNALS__" in window) {
    const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
    label.value = getCurrentWebviewWindow().label;
  } else {
    label.value = location.hash.replace(/^#\/?/, "") || "pod_1";
  }
  const m = label.value.match(/^pod_(\d+)(?:_panel)?$/);
  if (m) {
    podId.value = Number(m[1]);
    isPanel.value = !!m[2];
  }
});

const view = computed(() => {
  if (label.value === "settings") return SettingsWindow;
  if (label.value.match(/^pod_\d+_panel$/)) return PodPanel;
  return PodBar;
});
</script>

<template>
  <component :is="view" :pod-id="podId" />
</template>
