<script setup lang="ts">
/**
 * 单个「匣」的胶囊条：贴在屏幕边缘的圆形短条。
 * - 背景全透明，短条半透明填充
 * - 悬停 -> 弹出该匣面板；移出 -> 面板收回（看门狗）
 * - 拖入文件时短条变为圆角矩形主动接纳，松手后弹出面板
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ipc } from "@/lib/ipc";
import { Events, listenCurrent } from "@/lib/events";
import { springValue, type SpringHandle } from "@/lib/spring";
import { useSettingsStore } from "@/stores/settings";
import { useStagingStore } from "@/stores/staging";

const props = defineProps<{ podId: number }>();
const settingsStore = useSettingsStore();
const staging = useStagingStore();

const pod = computed(() => settingsStore.pod(props.podId));
const vertical = computed(() => pod.value?.edge === "left" || pod.value?.edge === "right");
const hovering = ref(false);
const accepting = ref(false);
let hoverTimeout: number | undefined;
let shortSpring: SpringHandle | null = null;
const short = ref(44);
const unlisteners: Array<() => void> = [];

/* ---- 边缘拖动 ---- */
const dragging = ref(false);
let justDragged = false; // 记录刚刚完成拖动，防止触发点击
let dragStartPos = 0;
let dragStartOffset = 0;
let dragStartScreenLength = 1080; // 拖动开始时的屏幕尺寸

const count = computed(
  () => staging.items.filter((i) => i.podId === props.podId).length,
);

const capsuleStyle = computed(() =>
  vertical.value
    ? { width: short.value + "px", height: "100%", opacity: pod.value?.opacity ?? 0.85 }
    : { height: short.value + "px", width: "100%", opacity: pod.value?.opacity ?? 0.85 },
);

/* 仅在拖入接纳时短条变宽（圆角矩形）；悬停弹出面板不改变形状。
   目标 62 与 Rust 侧 POD_BAR_ACCEPT 一致：胶囊填满窗口，圆角矩形完整显示。 */
watch(accepting, (v) => shortSpring?.setTarget(v ? 62 : 44));

function onPointerEnter() {
  hovering.value = true;
  window.clearTimeout(hoverTimeout);
  void ipc.reportPresence(props.podId, "bar", true);
  // 如果正在拖动，不弹出面板
  if (!dragging.value) {
    hoverTimeout = window.setTimeout(() => {
      void ipc.showPanel(props.podId);
    }, pod.value?.hoverDelayMs ?? 120);
  }
}

function onPointerLeave() {
  hovering.value = false;
  window.clearTimeout(hoverTimeout);
  void ipc.reportPresence(props.podId, "bar", false);
}

function onClick() {
  // 如果刚完成拖动，不触发点击
  if (justDragged) {
    justDragged = false;
    return;
  }
  // 点击时取消挂起的hover定时器，防止旧定时器重新打开刚关闭的面板
  window.clearTimeout(hoverTimeout);
  void ipc.togglePanel(props.podId);
}

/* ---- 边缘拖动处理 ---- */
async function onPointerDown(e: PointerEvent) {
  // 只响应主按钮（左键）
  if (e.button !== 0 || !pod.value) return;

  // 记录起始位置和当前 offset
  dragStartPos = vertical.value ? e.clientY : e.clientX;
  dragStartOffset = pod.value.offset;
  dragging.value = false;
  justDragged = false;

  // 获取显示器尺寸用于计算 offset
  try {
    const monitors = await ipc.getMonitors();
    const monitorName = pod.value.monitor;
    const monitor = monitors.find(m => m.name === monitorName);
    // 使用实际显示器的尺寸，如果没有找到则使用 window.screen
    if (monitor) {
      // TODO: 需要从 Rust 端获取显示器实际尺寸
      dragStartScreenLength = vertical.value ? window.screen.height : window.screen.width;
    } else {
      dragStartScreenLength = vertical.value ? window.screen.height : window.screen.width;
    }
    screenLength = dragStartScreenLength;
  } catch {
    dragStartScreenLength = vertical.value ? window.screen.height : window.screen.width;
    screenLength = dragStartScreenLength;
  }

  // 添加全局监听
  window.addEventListener("pointermove", onPointerMove);
  window.addEventListener("pointerup", onPointerUp);
  window.addEventListener("pointercancel", onPointerUp);

  // 阻止默认行为和事件冒泡
  e.preventDefault();
  e.stopPropagation();
}

async function onPointerMove(e: PointerEvent) {
  if (!pod.value) return;

  const currentPos = vertical.value ? e.clientY : e.clientX;
  const delta = currentPos - dragStartPos;

  // 拖动阈值：3px
  if (!dragging.value && Math.abs(delta) < 3) return;

  // 标记为拖动中
  if (!dragging.value) {
    dragging.value = true;
    // 拖动时取消 hover 定时器，避免弹出面板
    window.clearTimeout(hoverTimeout);
    // 隐藏面板（如果已显示）
    void ipc.hidePanel(props.podId);
  }

  // 计算新的 offset（0-1 范围）
  const deltaRatio = delta / screenLength;
  const newOffset = Math.max(0, Math.min(1, dragStartOffset + deltaRatio));

  // 使用轻量级命令实时移动窗口（不写数据库，性能更好）
  await ipc.movePodBar(props.podId, newOffset);

  // 阻止默认行为
  e.preventDefault();
}

async function onPointerUp(e: PointerEvent) {
  // 移除全局监听
  window.removeEventListener("pointermove", onPointerMove);
  window.removeEventListener("pointerup", onPointerUp);
  window.removeEventListener("pointercancel", onPointerUp);

  if (dragging.value) {
    dragging.value = false;
    justDragged = true; // 标记刚刚完成拖动，防止触发点击
    
    // 计算最终的 offset
    const currentPos = vertical.value ? e.clientY : e.clientX;
    const delta = currentPos - dragStartPos;
    const deltaRatio = delta / screenLength;
    const finalOffset = Math.max(0, Math.min(1, dragStartOffset + deltaRatio));
    
    // 保存到数据库
    if (pod.value) {
      await ipc.updatePod(props.podId, { offset: finalOffset });
    }
    
    // 阻止拖动结束后触发 onClick
    e.preventDefault();
    e.stopPropagation();
  }
}

/* ---- 文件拖入（原生） ---- */
async function handleDrop(paths: string[]) {
  accepting.value = false;
  void ipc.setPodAccept(props.podId, false);
  if (!pod.value || paths.length === 0) return;
  const action = pod.value.dropAction ?? "ask";
  const mods = await ipc
    .getModifierState()
    .catch(() => ({ ctrl: false, shift: false, alt: false }));
  let chosen: string | null = null;
  if (mods.ctrl) chosen = "copy";
  else if (mods.shift) chosen = "move";
  else if (mods.alt) chosen = "shortcut";
  else if (action !== "ask") chosen = action;

  try {
    if (chosen) {
      await ipc.stagePaths(props.podId, paths, chosen as never);
    } else {
      await ipc.holdPendingDrop(props.podId, paths);
    }
    // 拖入完成后弹出面板
    void ipc.showPanel(props.podId);
  } catch (err) {
    console.error("stage failed", err);
  }
}

/* ---- 文字拖入（HTML5 兜底；原生只拦文件） ---- */
async function onHtmlDrop(e: DragEvent) {
  const dt = e.dataTransfer;
  if (!dt || dt.files?.length) return;
  const text = dt.getData("text/plain");
  if (text && pod.value) {
    e.preventDefault();
    try {
      await ipc.stageText(props.podId, text);
      void ipc.showPanel(props.podId);
    } catch (err) {
      console.error("stage text failed", err);
    }
  }
}

onMounted(async () => {
  await settingsStore.load();
  staging.setActivePod(props.podId);
  await staging.refresh(props.podId);
  void settingsStore.listenChanges();
  unlisteners.push(await staging.listenChanges(props.podId));

  /* 原生拖放事件（文件路径） */
  if (ipc.inTauri) {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    unlisteners.push(await getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === "enter") {
        accepting.value = true;
        void ipc.setPodAccept(props.podId, true);
      } else if (p.type === "over") {
        accepting.value = true;
      } else if (p.type === "leave") {
        accepting.value = false;
        void ipc.setPodAccept(props.podId, false);
      } else if (p.type === "drop") {
        accepting.value = false;
        void ipc.setPodAccept(props.podId, false);
        void handleDrop(p.paths);
      }
    }));
  }

  /* 剪贴板收集热键：只由本匣处理（事件携带 podId） */
  unlisteners.push(await listenCurrent<{ podId?: number }>(Events.CollectClipboard, async (p) => {
    if (!p || (p.podId && p.podId !== props.podId)) return;
    if (!pod.value) return;
    try {
      const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
      const text = await readText();
      if (text.trim()) await ipc.stageText(props.podId, text);
    } catch (err) {
      console.error("collect clipboard failed", err);
    }
  }));

  /* 胶囊短边弹簧 */
  shortSpring = springValue(44, 44, (v) => (short.value = v), {
    response: 0.32,
    damping: 1,
  });
});

onBeforeUnmount(() => {
  window.clearTimeout(hoverTimeout);
  shortSpring?.stop();
  unlisteners.splice(0).forEach((unlisten) => unlisten());
});
</script>

<template>
  <div
    class="bar-root"
    :class="`edge-${pod?.edge ?? 'left'}`"
    @pointerenter="onPointerEnter"
    @pointerleave="onPointerLeave"
    @pointerdown="onPointerDown"
    @click="onClick"
    @dragover.prevent
    @drop="onHtmlDrop"
  >
    <div class="capsule" :class="{ accepting, hovering, dragging }" :style="capsuleStyle">
      <div class="capsule-inner">
        <Transition name="fade">
          <div v-if="accepting" class="drop-hint">松手暂存</div>
        </Transition>
        <template v-if="!accepting">
          <div v-if="count > 0" class="count-badge">{{ count > 99 ? "99+" : count }}</div>
          <div v-else class="idle-mark" />
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bar-root {
  position: fixed;
  inset: 0;
  overflow: hidden;
  cursor: default;
}

.capsule {
  position: absolute;
  min-width: 44px;
  min-height: 44px;
  will-change: width, height;
  background: var(--bar-glass);
  box-shadow: inset 0 0 0 1px var(--glass-line);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: box-shadow 200ms var(--ease-out), background 200ms var(--ease-out);
}
.edge-left .capsule {
  left: 0;
  top: 0;
  bottom: 0;
  border-radius: 0 22px 22px 0;
}
.edge-right .capsule {
  right: 0;
  top: 0;
  bottom: 0;
  border-radius: 22px 0 0 22px;
}
.edge-top .capsule {
  top: 0;
  left: 0;
  right: 0;
  border-radius: 0 0 22px 22px;
}
.edge-bottom .capsule {
  bottom: 0;
  left: 0;
  right: 0;
  border-radius: 22px 22px 0 0;
}

.capsule.hovering {
  background: var(--bar-glass-hover);
}
.capsule.accepting {
  background: var(--accent);
  box-shadow: inset 0 0 0 1.5px oklch(1 0 0 / 0.25);
  animation: breathe 1.1s ease-in-out infinite;
}
.capsule.dragging {
  background: var(--accent);
  box-shadow: inset 0 0 0 1.5px oklch(1 0 0 / 0.25);
  cursor: grabbing;
  transition: none;
}
@keyframes breathe {
  0%, 100% { filter: brightness(1); }
  50% { filter: brightness(1.18); }
}

.capsule-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
}
.count-badge {
  min-width: 24px;
  height: 24px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: 12px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 2px 8px rgb(0 0 0 / 0.25);
}
.idle-mark {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: var(--ink-3);
}
.drop-hint {
  writing-mode: vertical-rl;
  letter-spacing: 0.3em;
  font-size: 12px;
  font-weight: 650;
  color: var(--on-accent);
  white-space: nowrap;
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity 150ms ease;
}
.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

@media (prefers-reduced-motion: reduce) {
  .capsule.accepting {
    animation: none;
  }
}
</style>
