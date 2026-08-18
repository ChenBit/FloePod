<script setup lang="ts">
/**
 * 单个「匣」的弹出面板：列表 / 拖入询问 / 冲突解决 三种模式。
 * 不抢焦点显示（Rust 侧 SW_SHOWNOACTIVATE），悬停离开自动收回（Rust 看门狗）。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ipc } from "@/lib/ipc";
import { Events, listen } from "@/lib/events";
import { useSettingsStore } from "@/stores/settings";
import { useStagingStore } from "@/stores/staging";
import type { ConflictStrategy, DropAction, ExportMode, PanelMode, StagedItem } from "@/types";
import ItemRow from "@/components/ItemRow.vue";
import ActionChooser from "@/components/ActionChooser.vue";
import ConflictDialog from "@/components/ConflictDialog.vue";
import SegmentedControl from "@/components/SegmentedControl.vue";

const props = defineProps<{ podId: number }>();

const settingsStore = useSettingsStore();
const staging = useStagingStore();

const pod = computed(() => settingsStore.pod(props.podId));
const edge = computed(() => pod.value?.edge ?? "left");

const mode = ref<PanelMode>("list");
const pendingPaths = ref<string[]>([]);
const dragMode = ref<"copy" | "move">("copy");
const textOpen = ref(false);
const textValue = ref("");
const toast = ref("");
let toastTimer: number | undefined;
let anchorId: number | null = null;

/* ---------- 固定 / 滑入 ---------- */
const pinned = ref(false);
const rootEl = ref<HTMLElement | null>(null);
let lastSlideIn = 0;

function slideDir(): string {
  return edge.value;
}

function playSlideIn() {
  const el = rootEl.value;
  if (!el) return;
  // 首挂载时 onMounted 与 PANEL_SHOWN 会先后触发，短窗内去重避免动画重播闪烁
  const now = performance.now();
  if (now - lastSlideIn < 100) return;
  lastSlideIn = now;
  // 先清除隐藏后遗留的「待显示」透明态，再从头播放滑入
  el.classList.remove("pre-show");
  el.classList.remove("slide-in", ...slideDirs());
  void el.offsetWidth;
  el.classList.add("slide-in", `slide-in-${slideDir()}`);
}

function slideDirs(): string[] {
  return ["left", "right", "top", "bottom"].map((d) => `slide-in-${d}`);
}

function onTogglePinned() {
  void ipc.setPanelPinned(props.podId, !pinned.value);
}

/* ---------- 冲突上下文 ---------- */
const conflict = ref<{ names: string[]; ids: number[]; dest: string; mode: ExportMode } | null>(
  null,
);

const items = computed(() => staging.activeItems);
const selectedCount = computed(() => staging.selectedIds.size);

function showToast(msg: string) {
  toast.value = msg;
  window.clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => (toast.value = ""), 2200);
}

/* ---------- 选择 ---------- */
function onSelect(id: number, m: "set" | "toggle" | "range") {
  if (m === "set") {
    staging.clearSelection();
    staging.selectedIds.add(id);
    anchorId = id;
  } else if (m === "toggle") {
    if (staging.selectedIds.has(id)) staging.selectedIds.delete(id);
    else staging.selectedIds.add(id);
    anchorId = id;
  } else if (m === "range" && anchorId != null) {
    const ids = items.value.map((i) => i.id);
    const a = ids.indexOf(anchorId);
    const b = ids.indexOf(id);
    if (a >= 0 && b >= 0) {
      const [lo, hi] = a < b ? [a, b] : [b, a];
      for (const i of ids.slice(lo, hi + 1)) staging.selectedIds.add(i);
    }
  } else {
    staging.selectedIds.add(id);
    anchorId = id;
  }
}

function selectedOrSingle(item: StagedItem): string[] {
  if (staging.selectedIds.has(item.id)) {
    return staging.selectedItems.map((i) => i.stagingPath);
  }
  return [item.stagingPath];
}

/* ---------- 拖出 ---------- */
function makeDragIcon(paths: string[], ext: string | null): string {
  const c = document.createElement("canvas");
  const dpr = window.devicePixelRatio || 1;
  const w = 44;
  c.width = 64 * dpr;
  c.height = 64 * dpr;
  const ctx = c.getContext("2d");
  if (!ctx) return "";
  ctx.scale(dpr, dpr);
  const dark = document.documentElement.classList.contains("dark");
  ctx.fillStyle = dark ? "#2a2e33" : "#f6f7f8";
  const r = 10;
  roundRect(ctx, 10, 8, w, 48, r);
  ctx.fill();
  ctx.strokeStyle = dark ? "#3d434a" : "#d6dade";
  ctx.lineWidth = 1.5;
  roundRect(ctx, 10, 8, w, 48, r);
  ctx.stroke();
  ctx.fillStyle = dark ? "#dfe3e6" : "#3d434a";
  ctx.font = "600 13px 'Segoe UI', sans-serif";
  ctx.textAlign = "center";
  ctx.fillText((ext ?? "文件").slice(0, 5).toUpperCase(), 32, 36);
  if (paths.length > 1) {
    ctx.fillStyle = "#2d7ca3";
    ctx.beginPath();
    ctx.arc(46, 44, 11, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#fff";
    ctx.font = "600 11px 'Segoe UI', sans-serif";
    ctx.fillText(String(paths.length), 46, 48);
  }
  return c.toDataURL("image/png");
}

function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

async function onDragOut(paths: string[]) {
  if (paths.length === 0) return;
  const first = staging.items.find((i) => i.stagingPath === paths[0]);
  const icon = makeDragIcon(paths, first?.ext ?? null);
  const isCut = dragMode.value === "move";
  try {
    await ipc.setDraggingOut(props.podId, true);
    await ipc.startDragOut(paths, icon, dragMode.value, (dropped) => {
      if (dropped && isCut) {
        void ipc.finalizeDragCut(paths).then(() => staging.refresh(props.podId));
      }
    });
  } catch (err) {
    console.error("drag out failed", err);
  } finally {
    await ipc.setDraggingOut(props.podId, false);
  }
}

/* ---------- 打开 / 移除 ---------- */
async function openItem(item: StagedItem) {
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(item.stagingPath);
}

async function revealItem(item: StagedItem) {
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  await revealItemInDir(item.stagingPath);
}

async function removeItem(item: StagedItem) {
  await staging.removeItems([item.id], true);
  showToast("已移出暂存（文件进回收站）");
}

async function removeSelected() {
  if (!selectedCount.value) return;
  const n = selectedCount.value;
  await staging.removeItems([...staging.selectedIds], true);
  showToast(`已移出 ${n} 项（文件进回收站）`);
}

/* ---------- 导出 ---------- */
async function pickDest(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title: "选择目标文件夹" });
  return typeof dir === "string" ? dir : null;
}

async function exportSelected(exportMode: ExportMode) {
  const ids = [...staging.selectedIds];
  if (!ids.length) return;
  const dest = await pickDest();
  if (!dest) return;
  try {
    const names = await staging.exportItems(ids, dest, exportMode);
    if (names.length > 0) {
      conflict.value = { names, ids, dest, mode: exportMode };
      mode.value = "conflict";
      return;
    }
    if (exportMode === "move") staging.clearSelection();
    await staging.refresh(props.podId);
    showToast(exportMode === "move" ? `已移动 ${ids.length} 项` : `已复制 ${ids.length} 项`);
  } catch (err) {
    console.error(err);
    showToast("导出失败，请重试");
  }
}

async function resolveConflict(strategy: Exclude<ConflictStrategy, "ask">) {
  const ctx = conflict.value;
  if (!ctx) return;
  conflict.value = null;
  mode.value = "list";
  try {
    await ipc.exportItems(ctx.ids, ctx.dest, ctx.mode, strategy);
    if (ctx.mode === "move") staging.clearSelection();
    await staging.refresh(props.podId);
    showToast(ctx.mode === "move" ? "移动完成" : "复制完成");
  } catch {
    showToast("导出失败，请重试");
  }
}

/* ---------- 询问模式 ---------- */
async function chooseAction(action: DropAction, remember: boolean) {
  const paths = pendingPaths.value;
  pendingPaths.value = [];
  mode.value = "list";
  await ipc.setPanelMode(props.podId, "list");
  if (remember && pod.value) {
    await ipc.updatePod(props.podId, { dropAction: action });
  }
  try {
    await ipc.stagePaths(props.podId, paths, action);
    const verb = action === "copy" ? "复制" : action === "move" ? "移动" : "快捷方式";
    showToast(`已暂存 ${paths.length} 项（${verb}）`);
  } catch (err) {
    console.error(err);
    showToast("暂存失败，请重试");
  }
}

async function cancelAsk() {
  pendingPaths.value = [];
  mode.value = "list";
  await ipc.setPanelMode(props.podId, "list");
}

/* ---------- 文字暂存 ---------- */
async function stashText() {
  const content = textValue.value.trim();
  if (!content) return;
  try {
    await ipc.stageText(props.podId, content);
    textValue.value = "";
    textOpen.value = false;
    showToast("文字已暂存");
  } catch {
    showToast("暂存失败，请重试");
  }
}

/* ---------- 清空 ---------- */
const confirmClear = ref(false);
async function clearAll() {
  if (!confirmClear.value) {
    confirmClear.value = true;
    window.setTimeout(() => (confirmClear.value = false), 2500);
    return;
  }
  confirmClear.value = false;
  await staging.clearActivePod(true);
  showToast("已清空（文件进回收站）");
}

/* ---------- 键盘 ---------- */
function onKeydown(e: KeyboardEvent) {
  if ((e.target as HTMLElement).tagName === "TEXTAREA" || (e.target as HTMLElement).tagName === "INPUT") {
    if (e.key === "Escape") (e.target as HTMLElement).blur();
    return;
  }
  if (e.key === "Escape") {
    if (selectedCount.value) staging.clearSelection();
    else void ipc.hidePanel(props.podId);
  } else if (e.ctrlKey && e.key.toLowerCase() === "a") {
    e.preventDefault();
    staging.selectAll();
  } else if (e.key === "Delete" && selectedCount.value) {
    void removeSelected();
  }
}

/* ---------- 面板尺寸自适应（防抖合并，避免连续 resize 造成跳动） ---------- */
const listEl = ref<HTMLElement | null>(null);
let ro: ResizeObserver | null = null;
let sizeTimer: number | undefined;

function scheduleResize() {
  window.clearTimeout(sizeTimer);
  sizeTimer = window.setTimeout(async () => {
    await nextTick();
    const el = listEl.value;
    if (!el) return;
    if (mode.value === "list") {
      const h = Math.min(el.scrollHeight, 560);
      await ipc
        .setPanelSize(props.podId, pod.value?.panelWidth ?? 380, h + 118)
        .catch(() => {});
    } else {
      await ipc
        .setPanelSize(props.podId, pod.value?.panelWidth ?? 380, el.scrollHeight + 16)
        .catch(() => {});
    }
  }, 110);
}

watch(
  () => [mode.value, items.value.length, textOpen.value, selectedCount.value],
  () => scheduleResize(),
);

onMounted(async () => {
  await settingsStore.load();
  staging.setActivePod(props.podId);
  await staging.refresh(props.podId);
  settingsStore.listenChanges();
  staging.listenChanges(props.podId);

  mode.value = "list";
  pendingPaths.value = [];
  pinned.value = false;

  listen<{ mode: PanelMode; paths?: string[] }>(Events.PanelMode, (p) => {
    mode.value = p.mode;
    pendingPaths.value = p.paths ?? [];
  });
  listen<never>(Events.SettingsChanged, () => {});

  /* 面板每次出现都重播滑入动画 */
  listen<never>(Events.PanelShown, () => playSlideIn());
  /* 固定状态同步 */
  listen<{ pinned: boolean }>(Events.PanelPinned, (p) => {
    pinned.value = p.pinned;
  });
  /* 窗口已被隐藏：DOM 置为「待显示」透明态，下次显示第一帧不闪现完整内容 */
  listen<never>(Events.PanelHidden, () => {
    const el = rootEl.value;
    if (!el) return;
    el.classList.remove(
      "slide-out",
      "slide-out-left",
      "slide-out-right",
      "slide-out-top",
      "slide-out-bottom",
    );
    el.classList.remove("slide-in", ...slideDirs());
    el.classList.add("pre-show");
  });

  window.addEventListener("keydown", onKeydown);

  ro = new ResizeObserver(() => scheduleResize());
  if (listEl.value) ro.observe(listEl.value);

  await nextTick();
  playSlideIn();
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  ro?.disconnect();
  window.clearTimeout(toastTimer);
  window.clearTimeout(sizeTimer);
});

function onPointerEnter() {
  void ipc.reportPresence(props.podId, "panel", true);
}
function onPointerLeave() {
  void ipc.reportPresence(props.podId, "panel", false);
}
</script>

<template>
  <div ref="rootEl" class="panel-root" @pointerenter="onPointerEnter" @pointerleave="onPointerLeave">
    <!-- 头部 -->
    <header class="panel-head">
      <div class="pod-name" :title="pod?.name">{{ pod?.name ?? "匣" }}</div>
      <div class="head-right">
        <button
          v-if="!textOpen"
          type="button"
          class="head-btn"
          title="暂存一段文字"
          @pointerdown="textOpen = true"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round">
            <path d="M4 7h16M4 12h10M4 17h7" />
          </svg>
        </button>
        <button
          type="button"
          class="head-btn"
          :class="{ on: pinned }"
          :title="pinned ? '已固定，移开鼠标面板保持展开' : '固定面板（移开鼠标后保持展开）'"
          @pointerdown="onTogglePinned"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 17v5" />
            <path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1z" />
          </svg>
        </button>
        <button type="button" class="head-btn" title="设置" @pointerdown="ipc.openSettings()">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.55-1 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h.09a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55h.09a1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.09a1.7 1.7 0 0 0 1.55 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.55 1Z" />
          </svg>
        </button>
      </div>
    </header>

    <!-- 主体 -->
    <div ref="listEl" class="panel-body">
      <ActionChooser
        v-if="mode === 'ask' && pendingPaths.length"
        :paths="pendingPaths"
        @choose="chooseAction"
        @cancel="cancelAsk"
      />

      <ConflictDialog
        v-else-if="mode === 'conflict' && conflict"
        :names="conflict.names"
        :mode="conflict.mode"
        @resolve="resolveConflict"
      />

      <div v-else-if="textOpen" class="text-stash">
        <textarea
          v-model="textValue"
          placeholder="粘贴或输入要暂存的文字…"
          rows="5"
          autofocus
        />
        <div class="text-actions">
          <button type="button" class="act primary" @pointerdown="stashText">暂存</button>
          <button type="button" class="act ghost" @pointerdown="textOpen = false">取消</button>
        </div>
      </div>

      <template v-else>
        <div v-if="items.length === 0" class="empty">
          <div class="empty-title">「{{ pod?.name ?? "匣" }}」是空的</div>
          <div class="empty-hint">把文件或图片拖到屏幕边缘的这个匣上<br />松手后会复制一份到这里</div>
        </div>
        <TransitionGroup v-else name="list" tag="div" class="items">
          <ItemRow
            v-for="item in items"
            :key="item.id"
            :item="item"
            :selected="staging.selectedIds.has(item.id)"
            :get-drag-paths="() => selectedOrSingle(item)"
            @select="onSelect"
            @open="openItem"
            @reveal="revealItem"
            @remove="removeItem"
            @drag-out="onDragOut"
          />
        </TransitionGroup>
      </template>
    </div>

    <!-- 底部 -->
    <footer class="panel-foot">
      <template v-if="selectedCount > 0">
        <span class="sel-count">已选 {{ selectedCount }} 项</span>
        <div class="foot-actions">
          <button type="button" class="foot-btn" @pointerdown="exportSelected('copy')">复制到…</button>
          <button type="button" class="foot-btn" @pointerdown="exportSelected('move')">移动到…</button>
          <button type="button" class="foot-btn danger" @pointerdown="removeSelected">移出</button>
          <button type="button" class="foot-btn ghost" @pointerdown="staging.clearSelection()">取消</button>
        </div>
      </template>
      <template v-else-if="items.length > 0">
        <div class="foot-left">
          <SegmentedControl
            :options="[
              { value: 'copy', label: '拖出：复制' },
              { value: 'move', label: '剪切' },
            ]"
            v-model="dragMode"
          />
        </div>
        <div class="foot-right">
          <button type="button" class="foot-btn ghost" @pointerdown="staging.selectAll()">全选</button>
          <button type="button" class="foot-btn ghost danger" @pointerdown="clearAll">
            {{ confirmClear ? "确认清空？" : "清空" }}
          </button>
        </div>
      </template>
    </footer>

    <!-- 轻提示 -->
    <Transition name="toast">
      <div v-if="toast" class="toast">{{ toast }}</div>
    </Transition>
  </div>
</template>

<style scoped>
.panel-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: var(--glass);
  /* 圆角由 Windows 系统窗口自带（DWM），CSS 再切一次会双重圆角导致边缘不平 */
  box-shadow: inset 0 0 0 1px var(--glass-line);
  overflow: hidden;
}
/* 隐藏后保持的「待显示」透明态：窗口显示第一帧不闪现完整内容 */
.panel-root.pre-show {
  opacity: 0;
  transform: translateY(6px) scale(0.98);
}
/* 滑入 / 滑出：方向由匣所在屏幕边缘决定 */
.panel-root.slide-in {
  animation-duration: 260ms;
  animation-timing-function: var(--ease-out);
  animation-fill-mode: both;
}
.panel-root.slide-in-left {
  animation-name: slide-in-left;
}
.panel-root.slide-in-right {
  animation-name: slide-in-right;
}
.panel-root.slide-in-top {
  animation-name: slide-in-top;
}
.panel-root.slide-in-bottom {
  animation-name: slide-in-bottom;
}
/* 收回动画已移除：直接隐藏窗口，交由 Windows 自带的窗口关闭动画 */
@keyframes slide-in-left {
  from { opacity: 0; transform: translateX(-30px) scale(0.98); }
}
@keyframes slide-in-right {
  from { opacity: 0; transform: translateX(30px) scale(0.98); }
}
@keyframes slide-in-top {
  from { opacity: 0; transform: translateY(-30px) scale(0.98); }
}
@keyframes slide-in-bottom {
  from { opacity: 0; transform: translateY(30px) scale(0.98); }
}
@media (prefers-reduced-motion: reduce) {
  .panel-root.slide-in {
    animation: fade-only 150ms ease;
  }
  @keyframes fade-only {
    from { opacity: 0; }
  }
}

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 6px;
  flex-shrink: 0;
}
.pod-name {
  font-size: 13px;
  font-weight: 600;
  color: var(--ink);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 180px;
}
.head-right {
  display: flex;
  gap: 2px;
}
.head-btn {
  border: 0;
  background: transparent;
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--ink-2);
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.head-btn:hover {
  background: var(--surface-2);
  color: var(--ink);
}
.head-btn.on {
  color: var(--accent);
}

.panel-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 4px 8px;
}

.empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 44px 20px;
  text-align: center;
}
.empty-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--ink);
}
.empty-hint {
  font-size: 12px;
  line-height: 1.7;
  color: var(--ink-3);
}

.items {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.list-enter-active {
  transition: opacity 220ms ease, transform 280ms var(--ease-out);
}
.list-leave-active {
  transition: opacity 140ms ease;
  position: absolute;
  width: calc(100% - 16px);
}
.list-enter-from {
  opacity: 0;
  transform: translateY(8px) scale(0.98);
}
.list-leave-to {
  opacity: 0;
}
.list-move {
  transition: transform 280ms var(--ease-out);
}

.panel-foot {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 12px 10px;
  border-top: 1px solid var(--glass-line);
  min-height: 44px;
}
.sel-count {
  font-size: 12px;
  color: var(--ink-2);
  font-weight: 550;
  white-space: nowrap;
}
.foot-actions,
.foot-right {
  display: flex;
  gap: 6px;
  align-items: center;
}
.foot-btn {
  border: 1px solid var(--line-strong);
  background: var(--surface);
  color: var(--ink);
  border-radius: 8px;
  padding: 5px 10px;
  font-size: 12px;
  font-weight: 520;
  cursor: pointer;
  font-family: inherit;
  transition: transform 100ms ease, background 120ms ease;
}
.foot-btn:active {
  transform: scale(0.97);
}
.foot-btn:hover {
  background: var(--surface-2);
}
.foot-btn.ghost {
  border-color: transparent;
  color: var(--ink-2);
}
.foot-btn.ghost:hover {
  border-color: var(--line-strong);
}
.foot-btn.danger {
  color: var(--danger);
}
.foot-btn.ghost.danger:hover {
  border-color: color-mix(in oklab, var(--danger) 45%, transparent);
}

.text-stash {
  padding: 14px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.text-stash textarea {
  width: 100%;
  resize: none;
  border: 1px solid var(--line-strong);
  border-radius: 10px;
  padding: 10px 12px;
  font-size: 13px;
  font-family: inherit;
  background: var(--surface);
  color: var(--ink);
  outline: none;
  line-height: 1.6;
  box-sizing: border-box;
}
.text-stash textarea:focus {
  border-color: var(--accent);
}
.text-actions {
  display: flex;
  gap: 8px;
}
.act {
  border: 1px solid var(--line-strong);
  background: var(--surface);
  color: var(--ink);
  border-radius: 9px;
  padding: 7px 14px;
  font-size: 12.5px;
  font-weight: 550;
  cursor: pointer;
  font-family: inherit;
}
.act.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--on-accent);
}
.act.ghost {
  border-color: transparent;
  color: var(--ink-2);
}

.toast {
  position: absolute;
  bottom: 52px;
  left: 50%;
  transform: translateX(-50%);
  background: var(--ink);
  color: var(--surface);
  font-size: 12px;
  padding: 7px 14px;
  border-radius: 999px;
  box-shadow: var(--shadow-pop);
  white-space: nowrap;
  pointer-events: none;
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 180ms ease, transform 240ms var(--ease-out);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(6px);
}
</style>
