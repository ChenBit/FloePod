<script setup lang="ts">
/**
 * 常驻浮动窗：两种形态（浮动条 strip / 浮动书签 bookmark），同窗渲染。
 * - 窗口物理尺寸由 Rust 管理（strip 悬停时加宽），视觉动画全部在 webview 内完成
 * - 原生 drag-drop 接收文件路径；HTML5 drop 兜底接收文字
 * - 悬停 -> 面板；单击 -> 固定/收起面板
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ipc } from "@/lib/ipc";
import { Events, listen } from "@/lib/events";
import { springValue, type SpringHandle } from "@/lib/spring";
import { useSettingsStore } from "@/stores/settings";
import { useStagingStore } from "@/stores/staging";
import BrandMark from "@/components/BrandMark.vue";

const settingsStore = useSettingsStore();
const staging = useStagingStore();

const hovering = ref(false);
const dropping = ref(false);
const dropCount = ref(0);
let hoverTimeout: number | undefined;
let capsuleSpring: SpringHandle | null = null;
let pulseSpring: SpringHandle | null = null;
const capsuleWidth = ref(10);
const pulse = ref(1);

const configured = computed(() => settingsStore.configured);
const form = computed(() => settingsStore.settings?.barForm ?? "strip");
const edge = computed(() => settingsStore.settings?.edge ?? "left");
const opacity = computed(() => settingsStore.settings?.opacity ?? 0.85);
const count = computed(
  () => staging.items.filter((i) => i.sceneId === staging.activeSceneId).length,
);
/* strip 展开宽度 = 窗口 48px 时的胶囊宽 */
const expanded = computed(() => hovering.value || dropping.value);

watch(expanded, (v) => {
  void ipc.setBarHover(v);
  capsuleSpring?.setTarget(v ? 46 : 10);
});

/* 条目数量变化 -> 轻微脉冲确认（带惯性的反馈） */
watch(count, (n, o) => {
  if (n > (o ?? 0)) {
    pulseSpring?.setTarget(1);
    requestAnimationFrame(() => pulseSpring?.setTarget(1.14));
    setTimeout(() => pulseSpring?.setTarget(1), 140);
  }
});

function onPointerEnter(e: PointerEvent) {
  hovering.value = true;
  window.clearTimeout(hoverTimeout);
  void ipc.reportPresence("bar", true);
  if (!configured.value) return;
  hoverTimeout = window.setTimeout(() => {
    void ipc.showPanel(e.clientY);
  }, settingsStore.settings?.hoverDelayMs ?? 120);
}

function onPointerLeave() {
  hovering.value = false;
  window.clearTimeout(hoverTimeout);
  void ipc.reportPresence("bar", false);
}

async function onClick() {
  if (!configured.value) {
    await ipc.openSettings();
    return;
  }
  /* 单击 = 切换面板（固定/收起） */
  await ipc.togglePanel();
}

/* ---- 文件拖入（原生） ---- */
async function handleDrop(paths: string[]) {
  dropping.value = false;
  if (!configured.value || paths.length === 0) return;
  const action = settingsStore.settings?.dropAction ?? "ask";
  const mods = await ipc.getModifierState().catch(() => ({
    ctrl: false,
    shift: false,
    alt: false,
  }));
  let chosen: string | null = null;
  if (mods.ctrl) chosen = "copy";
  else if (mods.shift) chosen = "move";
  else if (mods.alt) chosen = "shortcut";
  else if (action !== "ask") chosen = action;

  try {
    if (chosen) {
      await ipc.stagePaths(paths, chosen as never);
    } else {
      await ipc.holdPendingDrop(paths);
    }
  } catch (err) {
    console.error("stage failed", err);
  }
}

/* ---- 文字拖入（HTML5 兜底；原生只拦文件） ---- */
async function onHtmlDrop(e: DragEvent) {
  const dt = e.dataTransfer;
  if (!dt || dt.files?.length) return; // 文件走原生通道
  const text = dt.getData("text/plain");
  if (text && configured.value) {
    e.preventDefault();
    try {
      await ipc.stageText(text);
    } catch (err) {
      console.error("stage text failed", err);
    }
  }
}

onMounted(async () => {
  await settingsStore.load();
  await staging.refresh();
  staging.setActiveScene(settingsStore.settings?.activeSceneId ?? 0);
  settingsStore.listenChanges();
  staging.listenChanges();

  /* 原生拖放事件（文件路径） */
  if (ipc.inTauri) {
    const { getCurrentWebview } = await import("@tauri-apps/api/webview");
    await getCurrentWebview().onDragDrop((event) => {
      const p = event.payload as { type: string; paths: string[] };
      if (p.type === "enter" || p.type === "over") {
        dropping.value = true;
        dropCount.value = p.paths?.length ?? 0;
      } else if (p.type === "leave") {
        dropping.value = false;
      } else if (p.type === "drop") {
        dropCount.value = 0;
        void handleDrop(p.paths ?? []);
      }
    });
  }

  /* 剪贴板收集热键（由 Rust 全局快捷键触发） */
  listen<void>(Events.CollectClipboard, async () => {
    if (!configured.value) return;
    try {
      const { readText } = await import("@tauri-apps/plugin-clipboard-manager");
      const text = await readText();
      if (text.trim()) await ipc.stageText(text);
    } catch (err) {
      console.error("collect clipboard failed", err);
    }
  });

  /* 设置变化时重算形态 */
  listen<never>(Events.SettingsChanged, () => {
    staging.setActiveScene(settingsStore.settings?.activeSceneId ?? 0);
  });

  /* 胶囊宽度弹簧：damping 1.0 / response 0.35 */
  capsuleSpring = springValue(10, 10, (v) => (capsuleWidth.value = v), {
    response: 0.35,
    damping: 1,
  });
  pulseSpring = springValue(1, 1, (v) => (pulse.value = v), {
    response: 0.22,
    damping: 0.62,
  });
});

onBeforeUnmount(() => {
  window.clearTimeout(hoverTimeout);
  capsuleSpring?.stop();
  pulseSpring?.stop();
});
</script>

<template>
  <div
    class="bar-root"
    :class="[`edge-${edge}`, { dropping }]"
    @pointerenter="onPointerEnter"
    @pointerleave="onPointerLeave"
    @click="onClick"
    @dragover.prevent
    @drop="onHtmlDrop"
  >
    <!-- 浮动条形态 -->
    <div v-if="form === 'strip'" class="strip-capsule" :style="{ width: capsuleWidth + 'px', opacity }">
      <div class="strip-inner" :style="{ transform: `scaleX(${pulse})` }">
        <Transition name="fade">
          <div v-if="expanded && !dropping" class="strip-content">
            <div class="brand-rot">浮匣</div>
            <div v-if="count > 0" class="count-chip">{{ count > 99 ? "99+" : count }}</div>
            <svg v-else class="chevron" :class="`chevron-${edge}`" width="10" height="10" viewBox="0 0 10 10" fill="none">
              <path v-if="edge === 'left'" d="M7 2 3.5 5 7 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
              <path v-else d="M3 2 6.5 5 3 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" />
            </svg>
          </div>
        </Transition>
        <Transition name="fade">
          <div v-if="dropping" class="drop-hint">
            <div class="brand-rot accent-text">松手暂存</div>
          </div>
        </Transition>
        <div v-if="!expanded && !dropping && count > 0" class="count-dot" :class="`dot-${edge}`">
          <span>{{ count > 9 ? "9+" : count }}</span>
        </div>
        <div v-if="!configured" class="warn-dot" :class="`dot-${edge}`" />
      </div>
    </div>

    <!-- 浮动书签形态 -->
    <div v-else class="bookmark-capsule" :style="{ opacity }">
      <div class="bookmark-inner" :class="{ lifted: hovering || dropping }" :style="{ transform: `scale(${pulse})` }">
        <BrandMark :size="22" class="bookmark-brand" />
        <div v-if="count > 0" class="bookmark-count">{{ count > 99 ? "99+" : count }}</div>
        <Transition name="fade">
          <div v-if="dropping" class="bookmark-drop">
            <div class="v-text">松手暂存</div>
          </div>
        </Transition>
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

/* ---------- 浮动条 ---------- */
.strip-capsule {
  position: absolute;
  top: 8px;
  bottom: 8px;
  min-width: 10px;
  will-change: width;
  background: var(--glass);
  /* 桌面级模糊由 Rust 侧 windowEffects(Acrylic) 提供；此处只做半透明着色 */
  box-shadow: 0 0 0 1px var(--glass-line), var(--shadow-pop);
  transition: box-shadow 200ms ease;
}
.edge-left .strip-capsule {
  left: 0;
  border-radius: 0 10px 10px 0;
  border-right: 1px solid var(--glass-inner);
}
.edge-right .strip-capsule {
  right: 0;
  border-radius: 10px 0 0 10px;
  border-left: 1px solid var(--glass-inner);
}
.dropping .strip-capsule {
  background: var(--accent-soft);
  box-shadow: 0 0 0 1.5px var(--accent), var(--shadow-pop);
  animation: breathe 1.1s ease-in-out infinite;
}
@keyframes breathe {
  0%, 100% { filter: brightness(1); }
  50% { filter: brightness(1.18); }
}

.strip-inner {
  position: relative;
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
}
.strip-content {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 14px;
}
.brand-rot {
  writing-mode: vertical-rl;
  letter-spacing: 0.35em;
  font-size: 12.5px;
  font-weight: 600;
  color: var(--ink);
  user-select: none;
}
.accent-text {
  color: var(--accent);
  font-weight: 650;
}
.count-chip {
  min-width: 22px;
  height: 22px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: 11.5px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
}
.chevron {
  color: var(--ink-3);
}
.count-dot {
  position: absolute;
  top: 14px;
  min-width: 16px;
  height: 16px;
  padding: 0 4px;
  border-radius: 999px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: 10px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
}
.dot-left { left: -3px; }
.dot-right { right: -3px; }
.warn-dot {
  position: absolute;
  top: 14px;
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: var(--danger);
}
.drop-hint {
  display: flex;
  align-items: center;
}

/* ---------- 浮动书签 ---------- */
.bookmark-capsule {
  position: absolute;
  top: 50%;
  transform: translateY(-50%);
}
.edge-left .bookmark-capsule { left: 0; }
.edge-right .bookmark-capsule { right: 0; }

.bookmark-inner {
  position: relative;
  width: 38px;
  height: 168px;
  border-radius: 12px;
  background: var(--glass);
  box-shadow: 0 0 0 1px var(--glass-line), var(--shadow-pop);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  transition: box-shadow 200ms ease, transform 200ms cubic-bezier(0.3, 1, 0.4, 1);
  will-change: transform;
}
.bookmark-inner.lifted {
  box-shadow: 0 0 0 1.5px var(--accent), var(--shadow-panel);
}
.dropping .bookmark-inner {
  background: var(--accent-soft);
  animation: breathe 1.1s ease-in-out infinite;
}
.bookmark-brand {
  color: var(--accent);
}
.bookmark-count {
  min-width: 22px;
  height: 22px;
  padding: 0 6px;
  border-radius: 999px;
  background: var(--accent);
  color: var(--on-accent);
  font-size: 11.5px;
  font-weight: 600;
  display: flex;
  align-items: center;
  justify-content: center;
}
.bookmark-drop {
  position: absolute;
  inset: 0;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
}
.v-text {
  writing-mode: vertical-rl;
  letter-spacing: 0.3em;
  font-size: 12px;
  font-weight: 650;
  color: var(--accent);
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
  .dropping .strip-capsule,
  .dropping .bookmark-inner {
    animation: none;
  }
}
</style>
