<script setup lang="ts">
/**
 * 设置窗口：OOBE 首启引导 / 常规 / 匣管理 / 快捷键 / 关于。
 * 所有修改即时保存（save -> Rust 持久化并广播 settings_changed）。
 */
import { computed, onMounted, ref } from "vue";
import { ipc } from "@/lib/ipc";
import { useSettingsStore } from "@/stores/settings";
import SegmentedControl from "@/components/SegmentedControl.vue";
import ToggleSwitch from "@/components/ToggleSwitch.vue";
import SettingsRow from "@/components/SettingsRow.vue";
import HotkeyRecorder from "@/components/HotkeyRecorder.vue";
import BrandMark from "@/components/BrandMark.vue";
import type { Edge, Material, Pod, ThemeMode } from "@/types";

const settingsStore = useSettingsStore();
const s = computed(() => settingsStore.settings);
const monitors = computed(() => settingsStore.monitors);

const page = ref<"general" | "pods" | "hotkeys" | "about">("general");
const toast = ref("");
const hotkeyError = ref("");

/* ---------- OOBE ---------- */
const oobeDone = ref(false);
const firstRun = computed(
  () =>
    !oobeDone.value &&
    !!s.value &&
    (s.value.pods.length === 0 || !s.value.firstRunDone),
);
const oobeStep = ref(1);
const oobe = ref({
  name: "我的匣",
  edge: "left" as Edge,
  monitor: "",
  folder: "",
  theme: "system" as ThemeMode,
  opacity: 0.85,
  material: "acrylic" as Material,
});
const oobeBusy = ref(false);

const EDGES: { value: Edge; label: string }[] = [
  { value: "top", label: "上" },
  { value: "right", label: "右" },
  { value: "bottom", label: "下" },
  { value: "left", label: "左" },
];

function showToast(msg: string) {
  toast.value = msg;
  window.setTimeout(() => (toast.value = ""), 2400);
}

function appLog(msg: string) {
  void ipc.appLog(msg);
}

/** 带超时的等待：Promise 15 秒不返回则抛错，避免永远卡在「创建中」 */
function withTimeout<T>(p: Promise<T>, label: string, ms = 15000): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const t = window.setTimeout(() => reject(new Error(`${label} 超时（${ms}ms）`)), ms);
    p.then(
      (v) => {
        window.clearTimeout(t);
        resolve(v);
      },
      (e) => {
        window.clearTimeout(t);
        reject(e);
      },
    );
  });
}

/* ---------- 通用 ---------- */
async function save(patch: Parameters<typeof settingsStore.save>[0]) {
  try {
    await settingsStore.save(patch);
  } catch (err) {
    console.error(err);
    showToast("保存失败，请重试");
  }
}

async function pickFolder(): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title: "选择暂存文件夹" });
  return typeof dir === "string" ? dir : null;
}

async function openPodFolder(pod: Pod) {
  if (!pod.stagingFolder) return;
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(pod.stagingFolder);
}

async function savePod(id: number, patch: Partial<Pod>) {
  try {
    await ipc.updatePod(id, patch);
    await settingsStore.refreshPods();
  } catch (err) {
    console.error(err);
    showToast("保存失败，请重试");
  }
}

async function addPod() {
  const folder = await pickFolder();
  if (!folder) return;
  const n = s.value?.pods.length ?? 0;
  const edge = (["left", "right", "top", "bottom"] as Edge[])[n % 4];
  try {
    await ipc.createPod({
      name: `匣 ${n + 1}`,
      edge,
      monitor: "",
      offset: 0.5,
      stagingFolder: folder,
      opacity: 0.85,
      material: "acrylic",
      panelWidth: 380,
      hoverDelayMs: 120,
      dropAction: "ask",
      enabled: true,
    });
    await settingsStore.refreshPods();
    showToast("已创建新匣");
  } catch (err) {
    console.error(err);
    showToast("创建失败，请重试");
  }
}

async function removePod(pod: Pod) {
  const { ask } = await import("@tauri-apps/plugin-dialog");
  const ok = await ask(
    `删除「${pod.name}」？其中的暂存文件会一并移入回收站。`,
    { title: "删除匣", kind: "warning" },
  );
  if (!ok) return;
  try {
    await ipc.deletePod(pod.id, true);
    await settingsStore.refreshPods();
    showToast("已删除");
  } catch (err) {
    console.error(err);
    showToast("删除失败，请重试");
  }
}

/* ---------- OOBE 完成 ---------- */
async function finishOobe() {
  if (oobeBusy.value) return;
  if (!oobe.value.folder) {
    showToast("请先选择保存文件夹");
    return;
  }
  oobeBusy.value = true;
  appLog("finishOobe 开始");
  try {
    appLog("finishOobe: 调用 createPod");
    await withTimeout(
      ipc.createPod({
        name: oobe.value.name || "我的匣",
        edge: oobe.value.edge,
        monitor: oobe.value.monitor,
        offset: 0.5,
        stagingFolder: oobe.value.folder,
        opacity: Number(oobe.value.opacity),
        material: oobe.value.material,
        panelWidth: 380,
        hoverDelayMs: 120,
        dropAction: "ask",
        enabled: true,
      }),
      "createPod",
    );
    appLog("finishOobe: createPod 完成");
    await withTimeout(
      ipc.saveSettings({ theme: oobe.value.theme, firstRunDone: true }),
      "saveSettings",
    );
    appLog("finishOobe: saveSettings 完成");
    await withTimeout(settingsStore.refreshPods(), "refreshPods");
    appLog("finishOobe: refreshPods 完成");
    oobeDone.value = true; // 兜底：确保向导退出
    page.value = "pods";
  } catch (err) {
    console.error("finishOobe failed", err);
    appLog(`finishOobe 失败: ${err}`);
    showToast(`创建失败：${err}`);
  } finally {
    oobeBusy.value = false;
  }
}

/** OOBE 第二步：未选文件夹时不允许进入下一步 */
function nextFromStep2() {
  if (!oobe.value.folder) {
    showToast("请先选择保存文件夹");
    return;
  }
  oobeStep.value = 3;
}

/* ---------- 快捷键 ---------- */
async function saveHotkey(key: "toggleBar" | "collectClipboard" | "openPanel", combo: string) {
  hotkeyError.value = "";
  const next = { ...s.value!.hotkeys, [key]: combo };
  try {
    await settingsStore.save({ hotkeys: next });
  } catch (err) {
    hotkeyError.value = `快捷键「${combo}」注册失败，可能与其他软件冲突`;
    showToast(hotkeyError.value);
  }
}

async function resetHotkeys() {
  const defaults = await ipc.getHotkeyDefaults();
  await settingsStore.save({ hotkeys: defaults }).catch(() => showToast("重置失败"));
}

/* ---------- 自绘标题栏 ---------- */
async function winMinimize() {
  if (!ipc.inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().minimize();
}

async function winClose() {
  if (!ipc.inTauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().hide();
}

onMounted(async () => {
  await settingsStore.load();
  settingsStore.listenChanges();
  appLog(
    `SettingsWindow mounted | firstRun=${firstRun.value} | pods=${s.value?.pods.length ?? 0} | firstRunDone=${s.value?.firstRunDone}`,
  );
  if (firstRun.value) {
    oobeStep.value = 1;
  } else {
    page.value = "general";
  }
});

const PAGES = [
  { id: "general", label: "常规" },
  { id: "pods", label: "匣" },
  { id: "hotkeys", label: "快捷键" },
  { id: "about", label: "关于" },
] as const;
</script>

<template>
  <div class="settings-root" v-if="s">
    <!-- 自绘标题栏 -->
    <div class="titlebar" data-tauri-drag-region>
      <div class="titlebar-title" data-tauri-drag-region>浮匣 设置</div>
      <div class="titlebar-controls">
        <button type="button" class="tb-btn" title="最小化" @pointerdown="winMinimize">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
            <path d="M2 6h8" />
          </svg>
        </button>
        <button type="button" class="tb-btn close" title="关闭" @pointerdown="winClose">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round">
            <path d="m3 3 6 6M9 3l-6 6" />
          </svg>
        </button>
      </div>
    </div>

    <!-- OOBE 首启引导 -->
    <div v-if="firstRun" class="oobe">
      <div class="oobe-card">
        <template v-if="oobeStep === 1">
          <BrandMark :size="56" class="oobe-brand" />
          <h1 class="oobe-title">欢迎使用浮匣</h1>
          <p class="oobe-text">
            浮匣是贴在屏幕边缘的暂存小工具：把任何文件拖到匣上，松手即可暂存；
            需要时再把文件从匣的窗口拖出去继续使用。
          </p>
          <p class="oobe-text dim">现在先创建一个「匣」吧。</p>
          <button type="button" class="btn primary" @pointerdown="oobeStep = 2">开始</button>
        </template>

        <template v-else-if="oobeStep === 2">
          <h2 class="oobe-step-title">创建你的匣</h2>
          <div class="oobe-form">
            <label class="field">
              <span>名称</span>
              <input v-model="oobe.name" class="input" maxlength="12" placeholder="我的匣" />
            </label>
            <label class="field">
              <span>贴在屏幕哪一边</span>
              <SegmentedControl :options="EDGES" v-model="oobe.edge" />
            </label>
            <label class="field">
              <span>显示器</span>
              <select v-model="oobe.monitor" class="input">
                <option value="">主显示器</option>
                <option v-for="m in monitors" :key="m.name" :value="m.name">{{ m.label }}</option>
              </select>
            </label>
            <div class="field">
              <span>保存文件夹</span>
              <div class="folder-line">
                <input :value="oobe.folder" class="input mono" readonly placeholder="选择存放暂存文件的文件夹" />
                <button type="button" class="btn" @pointerdown="async () => (oobe.folder = (await pickFolder()) ?? oobe.folder)">
                  选择…
                </button>
              </div>
            </div>
          </div>
          <div class="oobe-actions">
            <button type="button" class="btn ghost" @pointerdown="oobeStep = 1">上一步</button>
            <button type="button" class="btn primary" :disabled="!oobe.folder" @pointerdown="nextFromStep2">下一步</button>
          </div>
        </template>

        <template v-else>
          <h2 class="oobe-step-title">个性化</h2>
          <div class="oobe-form">
            <label class="field">
              <span>主题</span>
              <SegmentedControl
                :options="[
                  { value: 'system', label: '跟随系统' },
                  { value: 'light', label: '浅色' },
                  { value: 'dark', label: '深色' },
                ]"
                v-model="oobe.theme"
              />
            </label>
            <label class="field">
              <span>不透明度</span>
              <input type="range" class="slider" min="0.55" max="1" step="0.05" v-model.number="oobe.opacity" />
            </label>
            <label class="field">
              <span>材质</span>
              <SegmentedControl
                :options="[
                  { value: 'acrylic', label: '亚克力' },
                  { value: 'plain', label: '纯半透明' },
                ]"
                v-model="oobe.material"
              />
            </label>
          </div>
          <div class="oobe-actions">
            <button type="button" class="btn ghost" @pointerdown="oobeStep = 2">上一步</button>
            <button type="button" class="btn primary" :disabled="oobeBusy" @pointerdown="finishOobe">
              {{ oobeBusy ? "创建中…" : "完成" }}
            </button>
          </div>
        </template>
      </div>
    </div>

    <!-- 常规设置 -->
    <template v-else>
      <div class="settings-body">
        <aside class="nav">
          <div class="nav-brand">
            <BrandMark :size="20" class="brand-icon" />
            <span>浮匣</span>
          </div>
          <nav class="nav-list">
            <button
              v-for="p in PAGES"
              :key="p.id"
              type="button"
              class="nav-item"
              :class="{ active: page === p.id }"
              @pointerdown="page = p.id"
            >
              {{ p.label }}
            </button>
          </nav>
          <div class="nav-foot">FloePod · {{ s.version }}</div>
        </aside>

        <main class="content">
          <!-- 常规 -->
          <section v-show="page === 'general'">
            <h2 class="page-title">常规</h2>
            <SettingsRow label="主题">
              <SegmentedControl
                :options="[
                  { value: 'system', label: '跟随系统' },
                  { value: 'light', label: '浅色' },
                  { value: 'dark', label: '深色' },
                ]"
                :model-value="s.theme"
                @update:model-value="(v) => save({ theme: v as never })"
              />
            </SettingsRow>
            <div class="sep" />
            <SettingsRow label="开机自启" hint="以托盘常驻方式随 Windows 启动">
              <ToggleSwitch
                :model-value="s.autostart"
                @update:model-value="(v) => save({ autostart: v })"
              />
            </SettingsRow>
            <div class="sep" />
            <SettingsRow label="退出浮匣" hint="关闭所有匣并退出程序（托盘仍可退出）">
              <button type="button" class="btn" @pointerdown="ipc.quitApp()">退出</button>
            </SettingsRow>
          </section>

          <!-- 匣管理 -->
          <section v-show="page === 'pods'">
            <div class="page-head">
              <h2 class="page-title">匣</h2>
              <button type="button" class="btn" @pointerdown="addPod">+ 新建匣</button>
            </div>
            <p class="page-desc">
              每个匣是贴在屏幕边缘的独立暂存点，可分别设置位置、显示器和保存文件夹。
            </p>

            <div class="pod-list">
              <div v-for="pod in s.pods" :key="pod.id" class="pod-card" :class="{ off: !pod.enabled }">
                <div class="pod-head">
                  <input
                    :value="pod.name"
                    class="pod-name-input"
                    maxlength="12"
                    @change="(e) => savePod(pod.id, { name: (e.target as HTMLInputElement).value })"
                  />
                  <span class="pod-edge-tag">{{ pod.edge === 'left' ? '左' : pod.edge === 'right' ? '右' : pod.edge === 'top' ? '上' : '下' }}</span>
                  <div class="pod-head-ops">
                    <ToggleSwitch
                      :model-value="pod.enabled"
                      @update:model-value="(v) => savePod(pod.id, { enabled: v })"
                    />
                    <button type="button" class="op-btn danger" title="删除此匣" @pointerdown="removePod(pod)">删除</button>
                  </div>
                </div>

                <div class="pod-grid">
                  <div class="pg-item">
                    <span class="pg-label">屏幕边缘</span>
                    <SegmentedControl :options="EDGES" :model-value="pod.edge" @update:model-value="(v) => savePod(pod.id, { edge: v as Edge })" />
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">显示器</span>
                    <select :value="pod.monitor" class="input" @change="(e) => savePod(pod.id, { monitor: (e.target as HTMLSelectElement).value })">
                      <option value="">主显示器</option>
                      <option v-for="m in monitors" :key="m.name" :value="m.name">{{ m.label }}</option>
                    </select>
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">沿边缘位置 {{ Math.round(pod.offset * 100) }}%</span>
                    <input type="range" class="slider" min="0" max="1" step="0.01" :value="pod.offset" @input="(e) => savePod(pod.id, { offset: Number((e.target as HTMLInputElement).value) })" />
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">保存文件夹</span>
                    <div class="folder-line">
                      <input :value="pod.stagingFolder" class="input mono" readonly :title="pod.stagingFolder" placeholder="未选择" />
                      <button type="button" class="btn" @pointerdown="async () => { const f = await pickFolder(); if (f) await savePod(pod.id, { stagingFolder: f }); }">选择…</button>
                      <button v-if="pod.stagingFolder" type="button" class="btn ghost" @pointerdown="openPodFolder(pod)">打开</button>
                    </div>
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">不透明度 {{ Math.round(pod.opacity * 100) }}%</span>
                    <input type="range" class="slider" min="0.55" max="1" step="0.05" :value="pod.opacity" @input="(e) => savePod(pod.id, { opacity: Number((e.target as HTMLInputElement).value) })" />
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">材质</span>
                    <SegmentedControl
                      :options="[
                        { value: 'acrylic', label: '亚克力' },
                        { value: 'plain', label: '纯半透明' },
                      ]"
                      :model-value="pod.material"
                      @update:model-value="(v) => savePod(pod.id, { material: v as Material })"
                    />
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">面板宽度 {{ pod.panelWidth }}px</span>
                    <input type="range" class="slider" min="300" max="520" step="10" :value="pod.panelWidth" @input="(e) => savePod(pod.id, { panelWidth: Number((e.target as HTMLInputElement).value) })" />
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">悬停展开延迟 {{ pod.hoverDelayMs }}ms</span>
                    <input type="range" class="slider" min="0" max="400" step="20" :value="pod.hoverDelayMs" @input="(e) => savePod(pod.id, { hoverDelayMs: Number((e.target as HTMLInputElement).value) })" />
                  </div>
                  <div class="pg-item">
                    <span class="pg-label">拖入时</span>
                    <SegmentedControl
                      :options="[
                        { value: 'ask', label: '询问' },
                        { value: 'copy', label: '复制' },
                        { value: 'move', label: '移动' },
                        { value: 'shortcut', label: '快捷方式' },
                      ]"
                      :model-value="pod.dropAction"
                      @update:model-value="(v) => savePod(pod.id, { dropAction: v as never })"
                    />
                  </div>
                </div>
              </div>
            </div>
          </section>

          <!-- 快捷键 -->
          <section v-show="page === 'hotkeys'">
            <h2 class="page-title">快捷键</h2>
            <SettingsRow label="显示 / 隐藏全部匣">
              <HotkeyRecorder :model-value="s.hotkeys.toggleBar" @update:model-value="(v) => saveHotkey('toggleBar', v)" />
            </SettingsRow>
            <div class="sep" />
            <SettingsRow label="收集剪贴板文字" hint="把当前剪贴板里的文字存为第一匣的暂存">
              <HotkeyRecorder :model-value="s.hotkeys.collectClipboard" @update:model-value="(v) => saveHotkey('collectClipboard', v)" />
            </SettingsRow>
            <div class="sep" />
            <SettingsRow label="打开第一匣面板">
              <HotkeyRecorder :model-value="s.hotkeys.openPanel" @update:model-value="(v) => saveHotkey('openPanel', v)" />
            </SettingsRow>
            <p v-if="hotkeyError" class="error">{{ hotkeyError }}</p>
            <div class="reset-line">
              <button type="button" class="btn ghost" @pointerdown="resetHotkeys">恢复默认快捷键</button>
            </div>
          </section>

          <!-- 关于 -->
          <section v-show="page === 'about'">
            <div class="about-hero">
              <BrandMark :size="48" class="about-brand" />
              <div class="about-name">浮匣 FloePod</div>
              <div class="about-ver">版本 {{ s.version }}</div>
            </div>
            <p class="about-text">
              本地优先的屏幕边缘暂存工具：拖进来集中保管，拖出去继续使用。
              不联网、不收集数据，所有内容只存在你自己的电脑上。
            </p>
            <div class="about-meta">
              <div class="about-row">
                <span class="about-key">数据位置</span>
                <span class="about-val">{{ s.dataDir }}</span>
              </div>
              <div class="about-row">
                <span class="about-key">匣的数量</span>
                <span class="about-val">{{ s.pods.length }} 个</span>
              </div>
            </div>
          </section>
        </main>
      </div>
    </template>

    <Transition name="toast">
      <div v-if="toast" class="toast">{{ toast }}</div>
    </Transition>
  </div>
</template>

<style scoped>
.settings-root {
  position: fixed;
  inset: 0;
  display: flex;
  flex-direction: column;
  background: var(--surface);
  color: var(--ink);
}

/* 自绘标题栏：整条可拖拽，左侧标题，右侧窗口控制 */
.titlebar {
  flex-shrink: 0;
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-left: 14px;
  background: #ffffff;
  user-select: none;
}
.titlebar-title {
  font-size: 12px;
  color: var(--ink-2);
  letter-spacing: 0.02em;
}
.titlebar-controls {
  display: flex;
  height: 100%;
}
.tb-btn {
  width: 42px;
  height: 100%;
  border: 0;
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--ink-2);
  cursor: pointer;
  font-family: inherit;
  transition: background 120ms ease, color 120ms ease;
}
.tb-btn:hover {
  background: var(--surface-3);
  color: var(--ink);
}
.tb-btn.close:hover {
  background: var(--danger);
  color: #fff;
}

/* ---------- OOBE ---------- */
.oobe {
  flex: 1;
  min-height: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  background: var(--surface);
  overflow: auto;
}
.oobe-card {
  width: 380px;
  max-width: calc(100% - 48px);
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 16px;
  text-align: center;
  background: #ffffff;
  border-radius: 18px;
  box-shadow: var(--shadow-panel), 0 0 0 1px var(--line);
  padding: 34px 30px;
}
.oobe-brand {
  color: var(--accent);
}
.oobe-title {
  margin: 0;
  font-size: 21px;
  font-weight: 680;
  letter-spacing: -0.015em;
}
.oobe-step-title {
  margin: 0;
  font-size: 17px;
  font-weight: 650;
}
.oobe-text {
  margin: 0;
  font-size: 13px;
  line-height: 1.75;
  color: var(--ink-2);
}
.oobe-text.dim {
  color: var(--ink-3);
  font-size: 12.5px;
}
.oobe-form {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 14px;
  text-align: left;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  font-size: 12.5px;
  font-weight: 550;
  color: var(--ink-2);
}
.oobe-actions {
  display: flex;
  gap: 10px;
  justify-content: center;
  margin-top: 6px;
}

/* ---------- 主体布局 ---------- */
.settings-body {
  flex: 1;
  display: flex;
  min-height: 0;
}
.nav {
  width: 168px;
  flex-shrink: 0;
  background: #ffffff;
  display: flex;
  flex-direction: column;
  padding: 18px 10px 14px;
  border-right: 1px solid var(--line);
}
.nav-brand {
  display: flex;
  align-items: center;
  gap: 9px;
  font-size: 15px;
  font-weight: 650;
  letter-spacing: -0.01em;
  padding: 0 10px 16px;
}
.brand-icon {
  color: var(--accent);
}
.nav-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.nav-item {
  position: relative;
  display: flex;
  align-items: center;
  border: 0;
  background: transparent;
  text-align: left;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 13px;
  color: var(--ink-2);
  cursor: pointer;
  font-family: inherit;
  transition: background 120ms ease, color 120ms ease;
}
.nav-item:hover {
  background: #f2f4f7;
  color: var(--ink);
}
.nav-item.active {
  background: #eef3f8;
  color: var(--ink);
  font-weight: 600;
}
.nav-foot {
  margin-top: auto;
  padding: 0 10px;
  font-size: 11px;
  color: var(--ink-3);
}

.content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 26px 30px 34px;
}
.page-title {
  font-size: 19px;
  font-weight: 650;
  letter-spacing: -0.015em;
  margin: 0 0 6px;
}
.page-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.page-head .page-title {
  margin-bottom: 0;
}
.page-desc {
  font-size: 12.5px;
  color: var(--ink-3);
  line-height: 1.65;
  margin: 8px 0 16px;
}
.sep {
  height: 1px;
  background: var(--line);
}

/* ---------- 控件 ---------- */
.btn {
  border: 1px solid var(--line-strong);
  background: #ffffff;
  color: var(--ink);
  border-radius: 8px;
  padding: 6px 13px;
  font-size: 12.5px;
  font-weight: 550;
  cursor: pointer;
  font-family: inherit;
  transition: background 120ms ease, transform 100ms ease;
}
.btn:active {
  transform: scale(0.98);
}
.btn:hover {
  background: var(--surface-2);
}
.btn.primary {
  background: var(--accent);
  border-color: var(--accent);
  color: var(--on-accent);
}
.btn.primary:hover {
  background: var(--accent-hover);
}
.btn.ghost {
  border-color: transparent;
  color: var(--ink-2);
}
.btn.ghost:hover {
  border-color: var(--line-strong);
}
.btn:disabled {
  opacity: 0.45;
  cursor: default;
  pointer-events: none;
}
.input {
  border: 1px solid var(--line-strong);
  border-radius: 8px;
  padding: 6px 10px;
  font-size: 12.5px;
  background: #ffffff;
  color: var(--ink);
  outline: none;
  font-family: inherit;
}
.input:focus {
  border-color: var(--accent);
}
.input.mono {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  color: var(--ink-2);
}
select.input {
  cursor: pointer;
}
.slider {
  width: 170px;
  accent-color: var(--accent);
}
.folder-line {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}
.error {
  font-size: 12px;
  color: var(--danger);
  margin: 10px 0 0;
}
.reset-line {
  margin-top: 18px;
}

/* ---------- 匣卡片 ---------- */
.pod-list {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.pod-card {
  border: 1px solid var(--line);
  border-radius: 12px;
  background: #ffffff;
  padding: 14px 16px;
}
.pod-card.off {
  opacity: 0.6;
}
.pod-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 12px;
}
.pod-name-input {
  border: 0;
  background: transparent;
  font-size: 14.5px;
  font-weight: 650;
  color: var(--ink);
  outline: none;
  font-family: inherit;
  padding: 2px 4px;
  border-radius: 6px;
}
.pod-name-input:focus {
  background: var(--surface-2);
}
.pod-edge-tag {
  font-size: 11px;
  font-weight: 600;
  color: var(--on-accent);
  background: var(--accent);
  border-radius: 999px;
  padding: 2px 8px;
}
.pod-head-ops {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 10px;
}
.op-btn {
  border: 0;
  background: transparent;
  color: var(--ink-2);
  font-size: 12px;
  padding: 5px 8px;
  border-radius: 7px;
  cursor: pointer;
  font-family: inherit;
}
.op-btn:hover {
  background: var(--surface-3);
  color: var(--ink);
}
.op-btn.danger:hover {
  background: color-mix(in oklab, var(--danger) 14%, transparent);
  color: var(--danger);
}
.pod-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  gap: 14px 20px;
}
.pg-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}
.pg-label {
  font-size: 12px;
  color: var(--ink-2);
  font-weight: 550;
}

/* ---------- 关于 ---------- */
.about-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 22px 0 8px;
}
.about-brand {
  color: var(--accent);
}
.about-name {
  font-size: 18px;
  font-weight: 680;
  letter-spacing: -0.015em;
}
.about-ver {
  font-size: 12px;
  color: var(--ink-3);
}
.about-text {
  font-size: 13px;
  line-height: 1.8;
  color: var(--ink-2);
  max-width: 430px;
  margin: 6px auto 20px;
  text-align: center;
}
.about-meta {
  max-width: 430px;
  margin: 0 auto;
  border: 1px solid var(--line);
  border-radius: 12px;
  background: #ffffff;
  overflow: hidden;
}
.about-row {
  display: flex;
  align-items: baseline;
  gap: 16px;
  padding: 10px 14px;
  font-size: 12.5px;
}
.about-row + .about-row {
  border-top: 1px solid var(--line);
}
.about-key {
  flex-shrink: 0;
  color: var(--ink-2);
  font-weight: 550;
  width: 64px;
}
.about-val {
  color: var(--ink);
  overflow-wrap: anywhere;
}

.toast {
  position: absolute;
  bottom: 18px;
  left: 50%;
  transform: translateX(-50%);
  background: var(--ink);
  color: var(--surface);
  font-size: 12px;
  padding: 8px 16px;
  border-radius: 999px;
  box-shadow: var(--shadow-pop);
}
.toast-enter-active,
.toast-leave-active {
  transition: opacity 180ms ease, transform 220ms cubic-bezier(0.3, 1, 0.4, 1);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}
</style>
