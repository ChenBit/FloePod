<script setup lang="ts">
/**
 * 暂存面板：列表 / 拖入询问 / 冲突解决 三种模式。
 * 不抢焦点显示（Rust 侧 SW_SHOWNOACTIVATE），悬停离开自动隐藏（Rust 看门狗）。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { ipc } from "@/lib/ipc";
import { Events, listen } from "@/lib/events";
import { useSettingsStore } from "@/stores/settings";
import { useStagingStore } from "@/stores/staging";
import type { ConflictStrategy, DropAction, ExportMode, PanelMode, StagedItem } from "@/types";
import SceneSwitcher from "@/components/SceneSwitcher.vue";
import ItemRow from "@/components/ItemRow.vue";
import ActionChooser from "@/components/ActionChooser.vue";
import ConflictDialog from "@/components/ConflictDialog.vue";
import SegmentedControl from "@/components/SegmentedControl.vue";

const settingsStore = useSettingsStore();
const staging = useStagingStore();

const mode = ref<PanelMode>("list");
const pendingPaths = ref<string[]>([]);
const dragMode = ref<"copy" | "move">("copy");
const textOpen = ref(false);
const textValue = ref("");
const toast = ref("");
let toastTimer: number | undefined;
let anchorId: number | null = null;

/* 冲突上下文（进入 conflict 模式时记录） */
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
    await ipc.startDragOut(paths, icon, dragMode.value, (dropped) => {
      if (dropped && isCut) {
        void ipc.finalizeDragCut(paths).then(() => staging.refresh());
      }
    });
  } catch (err) {
    console.error("drag out failed", err);
  }
}

/* ---------- 打开 / 显示 / 移除 ---------- */
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
    await staging.refresh();
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
    await staging.refresh();
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
  await ipc.setPanelMode("list");
  if (remember) await settingsStore.save({ dropAction: action });
  try {
    await ipc.stagePaths(paths, action);
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
  await ipc.setPanelMode("list");
}

/* ---------- 文字暂存 ---------- */
async function stashText() {
  const content = textValue.value.trim();
  if (!content) return;
  try {
    await ipc.stageText(content);
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
  await staging.clearActiveScene(true);
  showToast("已清空当前场景（文件进回收站）");
}

/* ---------- 键盘 ---------- */
function onKeydown(e: KeyboardEvent) {
  if ((e.target as HTMLElement).tagName === "TEXTAREA" || (e.target as HTMLElement).tagName === "INPUT") {
    if (e.key === "Escape") (e.target as HTMLElement).blur();
    return;
  }
  if (e.key === "Escape") {
    if (selectedCount.value) staging.clearSelection();
    else void ipc.hidePanel();
  } else if (e.ctrlKey && e.key.toLowerCase() === "a") {
    e.preventDefault();
    staging.selectAll();
  } else if (e.key === "Delete" && selectedCount.value) {
    void removeSelected();
  }
}

/* ---------- 面板尺寸自适应 ---------- */
const listEl = ref<HTMLElement | null>(null);
let ro: ResizeObserver | null = null;

watch(
  () => [mode.value, items.value.length, textOpen.value, selectedCount.value],
  async () => {
    await nextTick();
    if (listEl.value && mode.value === "list") {
      const h = Math.min(listEl.value.scrollHeight, 560);
      await ipc.setPanelSize(settingsStore.settings?.panelWidth ?? 380, h + 118).catch(() => {});
    } else if (listEl.value) {
      await ipc.setPanelSize(settingsStore.settings?.panelWidth ?? 380, listEl.value.scrollHeight + 16).catch(() => {});
    }
  },
);

onMounted(async () => {
  await settingsStore.load();
  await staging.refresh();
  staging.setActiveScene(settingsStore.settings?.activeSceneId ?? 0);
  settingsStore.listenChanges();
  staging.listenChanges();

  const boot = await ipc.getBootstrap();
  mode.value = boot.panelMode;
  pendingPaths.value = boot.pendingDrop?.paths ?? [];
  dragMode.value = "copy";

  listen<{ mode: PanelMode; paths?: string[] }>(Events.PanelMode, (p) => {
    mode.value = p.mode;
    pendingPaths.value = p.paths ?? [];
  });
  listen<never>(Events.SettingsChanged, () => {
    staging.setActiveScene(settingsStore.settings?.activeSceneId ?? 0);
  });

  window.addEventListener("keydown", onKeydown);

  ro = new ResizeObserver(() => {
    /* 高度变化时同步窗口尺寸 */
    if (listEl.value && mode.value === "list") {
      void ipc
        .setPanelSize(settingsStore.settings?.panelWidth ?? 380, listEl.value.scrollHeight + 118)
        .catch(() => {});
    }
  });
  if (listEl.value) ro.observe(listEl.value);
});

onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
  ro?.disconnect();
  window.clearTimeout(toastTimer);
});

function onPointerEnter() {
  void ipc.reportPresence("panel", true);
}
function onPointerLeave() {
  void ipc.reportPresence("panel", false);
}
</script>

<template>
  <div class="panel-root" @pointerenter="onPointerEnter" @pointerleave="onPointerLeave">
    <!-- 头部 -->
    <header class="panel-head">
      <SceneSwitcher :scenes="staging.scenes" :active-id="staging.activeSceneId" @changed="staging.refresh()" />
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
      <!-- 拖入询问 -->
      <ActionChooser
        v-if="mode === 'ask' && pendingPaths.length"
        :paths="pendingPaths"
        @choose="chooseAction"
        @cancel="cancelAsk"
      />

      <!-- 冲突解决 -->
      <ConflictDialog
        v-else-if="mode === 'conflict' && conflict"
        :names="conflict.names"
        :mode="conflict.mode"
        @resolve="resolveConflict"
      />

      <!-- 文字暂存 -->
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

      <!-- 列表 -->
      <template v-else>
        <div v-if="items.length === 0" class="empty">
          <div class="empty-title">暂存箱是空的</div>
          <div class="empty-hint">把文件或图片拖到屏幕边缘的浮匣上<br />松手后会复制一份到这里</div>
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
  /* 桌面级模糊由 Rust 侧 windowEffects(Acrylic) 提供；此处只做半透明着色 */
  border-radius: var(--radius-panel);
  box-shadow: inset 0 0 0 1px var(--glass-line);
  overflow: hidden;
  animation: panel-in 260ms cubic-bezier(0.3, 1, 0.4, 1);
}
@keyframes panel-in {
  from {
    opacity: 0;
    transform: scale(0.97) translateX(-6px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
@media (prefers-reduced-motion: reduce) {
  .panel-root {
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
  transition: opacity 220ms ease, transform 280ms cubic-bezier(0.3, 1, 0.4, 1);
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
  transition: transform 280ms cubic-bezier(0.3, 1, 0.4, 1);
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
  transition: opacity 180ms ease, transform 220ms cubic-bezier(0.3, 1, 0.4, 1);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(6px);
}
</style>
