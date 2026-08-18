<script setup lang="ts">
/**
 * 多窗口共享一个 Vue 入口：按 Tauri 窗口 label 选择视图。
 * - settings          -> 设置 / OOBE
 * - pod_{id}          -> 匣的胶囊条（贴在屏幕边缘）
 * - pod_{id}_panel    -> 匣的弹出面板
 * 浏览器开发时用 location.hash（#/settings /#/pod_1 /#/pod_1_panel）。
 */
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import PodBar from "@/windows/PodBar.vue";
import PodPanel from "@/windows/PodPanel.vue";
import SettingsWindow from "@/windows/SettingsWindow.vue";

/**
 * 窗口标签必须在首帧渲染前确定。旧实现先渲染 pod_1，再在 onMounted
 * 中切换真实窗口，导致所有动态窗口短暂挂载错误组件并遗留事件监听。
 */
function resolveWindowLabel(): string {
  if ("__TAURI_INTERNALS__" in window) {
    return getCurrentWebviewWindow().label;
  }
  return location.hash.replace(/^#\/?/, "") || "pod_1";
}

const label = resolveWindowLabel();
const match = label.match(/^pod_(\d+)(?:_panel)?$/);
const podId = match ? Number(match[1]) : 1;
const view = label === "settings"
  ? SettingsWindow
  : /^pod_\d+_panel$/.test(label)
    ? PodPanel
    : PodBar;
</script>

<template>
  <component :is="view" :pod-id="podId" />
</template>
