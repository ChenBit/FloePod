<script setup lang="ts">
/**
 * 设置窗口：OOBE 首启引导 / 常规 / 匣管理 / 快捷键 / 关于。
 * 所有修改即时保存（save -> Rust 持久化并广播 settings_changed）。
 * 排版原则：单一强调色、层级靠字号与留白、分区标题建立可扫视结构、
 * 进入/切换动画统一使用出程缓动（--ease-out）。
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

const DROP_ACTIONS: { value: string; label: string }[] = [
  { value: "ask", label: "询问" },
  { value: "copy", label: "复制" },
  { value: "move", label: "移动" },
  { value: "shortcut", label: "快捷方式" },
];

const MATERIALS: { value: Material; label: string }[] = [
  { value: "acrylic", label: "亚克力" },
  { value: "plain", label: "纯半透明" },
];

const THEMES: { value: ThemeMode; label: string }[] = [
  { value: "system", label: "跟随系统" },
  { value: "light", label: "浅色" },
  { value: "dark", label: "深色" },
];

/** 侧栏导航图标（单线条，随 currentColor） */
const NAV_ICONS: Record<string, string> = {
  general:
    '<path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M2 14h4M10 8h4M18 16h4"/>',
  pods: '<rect x="3.5" y="3.5" width="17" height="17" rx="3"/><path d="M12 8v8M8 12h8"/>',
  hotkeys:
    '<rect x="3" y="6.5" width="18" height="11" rx="2"/><path d="M7.5 12h.01M12 12h.01M16.5 12h.01M10 15h4"/>',
  about: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5M12 8h.01"/>',
};

function edgeLabel(edge: string): string {
  return edge === "left" ? "左" : edge === "right" ? "右" : edge === "top" ? "上" : "下";
}

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
  void settingsStore.listenChanges();
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
              <SegmentedControl :options="THEMES" v-model="oobe.theme" />
            </label>
            <label class="field">
              <span>不透明度</span>
              <input type="range" class="slider" min="0.55" max="1" step="0.05" v-model.number="oobe.opacity" />
            </label>
            <label class="field">
              <span>材质</span>
              <SegmentedControl :options="MATERIALS" v-model="oobe.material" />
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

    <!-- 设置主体 -->
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
              <svg
                class="nav-ico"
                width="15"
                height="15"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.7"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <g v-html="NAV_ICONS[p.id]" />
              </svg>
              {{ p.label }}
            </button>
          </nav>
          <div class="nav-foot">FloePod · {{ s.version }}</div>
        </aside>

        <main class="content">
          <Transition name="page" mode="out-in">
            <section :key="page">
              <!-- 常规 -->
              <template v-if="page === 'general'">
                <h2 class="page-title">常规</h2>
                <p class="page-desc">浮匣的整体外观与行为。</p>
                <div class="settings-card">
                  <SettingsRow label="主题" hint="跟随系统会随 Windows 深浅色自动切换">
                    <SegmentedControl :options="THEMES" :model-value="s.theme" @update:model-value="(v) => save({ theme: v as never })" />
                  </SettingsRow>
                  <div class="sep" />
                  <SettingsRow label="开机自启" hint="以托盘常驻方式随 Windows 启动">
                    <ToggleSwitch :model-value="s.autostart" @update:model-value="(v) => save({ autostart: v })" />
                  </SettingsRow>
                  <div class="sep" />
                  <SettingsRow label="退出浮匣" hint="关闭所有匣并退出程序（托盘仍可退出）">
                    <button type="button" class="btn" @pointerdown="ipc.quitApp()">退出</button>
                  </SettingsRow>
                </div>
              </template>

              <!-- 匣管理 -->
              <template v-else-if="page === 'pods'">
                <div class="page-head">
                  <div>
                    <h2 class="page-title">匣</h2>
                    <p class="page-desc">每个匣是贴在屏幕边缘的独立暂存点，可分别设置位置、显示器和保存文件夹。</p>
                  </div>
                  <button type="button" class="btn" @pointerdown="addPod">+ 新建匣</button>
                </div>

                <TransitionGroup name="pod" tag="div" class="pod-list">
                  <div v-for="pod in s.pods" :key="pod.id" class="pod-card" :class="{ off: !pod.enabled }">
                    <div class="pod-head">
                      <input
                        :value="pod.name"
                        class="pod-name-input"
                        maxlength="12"
                        @change="(e) => savePod(pod.id, { name: (e.target as HTMLInputElement).value })"
                      />
                      <span class="pod-edge-tag">{{ edgeLabel(pod.edge) }}</span>
                      <div class="pod-head-ops">
                        <ToggleSwitch
                          :model-value="pod.enabled"
                          @update:model-value="(v) => savePod(pod.id, { enabled: v })"
                        />
                        <button type="button" class="op-btn danger" title="删除此匣" @pointerdown="removePod(pod)">
                          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                            <path d="M5 7h14M9 7V5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2m3 0-1 13a1.5 1.5 0 0 1-1.5 1.4h-7A1.5 1.5 0 0 1 6.5 20L5.5 7" />
                          </svg>
                        </button>
                      </div>
                    </div>

                    <div class="pod-groups">
                      <div class="pod-group">
                        <div class="group-title">位置</div>
                        <div class="frow">
                          <span class="flabel">屏幕边缘</span>
                          <div class="fctrl">
                            <SegmentedControl :options="EDGES" :model-value="pod.edge" @update:model-value="(v) => savePod(pod.id, { edge: v as Edge })" />
                          </div>
                        </div>
                        <div class="frow">
                          <span class="flabel">显示器</span>
                          <div class="fctrl">
                            <select :value="pod.monitor" class="input sel" @change="(e) => savePod(pod.id, { monitor: (e.target as HTMLSelectElement).value })">
                              <option value="">主显示器</option>
                              <option v-for="m in monitors" :key="m.name" :value="m.name">{{ m.label }}</option>
                            </select>
                          </div>
                        </div>
                        <div class="frow">
                          <span class="flabel">沿边缘位置</span>
                          <div class="fctrl">
                            <input type="range" class="slider" min="0" max="1" step="0.01" :value="pod.offset" @input="(e) => savePod(pod.id, { offset: Number((e.target as HTMLInputElement).value) })" />
                            <span class="fval">{{ Math.round(pod.offset * 100) }}%</span>
                          </div>
                        </div>
                      </div>

                      <div class="pod-group">
                        <div class="group-title">外观</div>
                        <div class="frow">
                          <span class="flabel">不透明度</span>
                          <div class="fctrl">
                            <input type="range" class="slider" min="0.55" max="1" step="0.05" :value="pod.opacity" @input="(e) => savePod(pod.id, { opacity: Number((e.target as HTMLInputElement).value) })" />
                            <span class="fval">{{ Math.round(pod.opacity * 100) }}%</span>
                          </div>
                        </div>
                        <div class="frow">
                          <span class="flabel">材质</span>
                          <div class="fctrl">
                            <SegmentedControl :options="MATERIALS" :model-value="pod.material" @update:model-value="(v) => savePod(pod.id, { material: v as Material })" />
                          </div>
                        </div>
                      </div>

                      <div class="pod-group">
                        <div class="group-title">面板</div>
                        <div class="frow">
                          <span class="flabel">面板宽度</span>
                          <div class="fctrl">
                            <input type="range" class="slider" min="300" max="520" step="10" :value="pod.panelWidth" @input="(e) => savePod(pod.id, { panelWidth: Number((e.target as HTMLInputElement).value) })" />
                            <span class="fval">{{ pod.panelWidth }}px</span>
                          </div>
                        </div>
                        <div class="frow">
                          <span class="flabel">悬停展开延迟</span>
                          <div class="fctrl">
                            <input type="range" class="slider" min="0" max="400" step="20" :value="pod.hoverDelayMs" @input="(e) => savePod(pod.id, { hoverDelayMs: Number((e.target as HTMLInputElement).value) })" />
                            <span class="fval">{{ pod.hoverDelayMs }}ms</span>
                          </div>
                        </div>
                      </div>

                      <div class="pod-group">
                        <div class="group-title">拖入</div>
                        <div class="frow">
                          <span class="flabel">落地动作</span>
                          <div class="fctrl">
                            <SegmentedControl :options="DROP_ACTIONS" :model-value="pod.dropAction" @update:model-value="(v) => savePod(pod.id, { dropAction: v as never })" />
                          </div>
                        </div>
                        <div class="frow folder-row">
                          <span class="flabel">暂存文件夹</span>
                          <div class="fctrl folder-line">
                            <input :value="pod.stagingFolder" class="input mono" readonly :title="pod.stagingFolder" placeholder="未选择" />
                            <button type="button" class="btn" @pointerdown="async () => { const f = await pickFolder(); if (f) await savePod(pod.id, { stagingFolder: f }); }">选择…</button>
                            <button v-if="pod.stagingFolder" type="button" class="btn ghost" @pointerdown="openPodFolder(pod)">打开</button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </div>
                </TransitionGroup>
              </template>

              <!-- 快捷键 -->
              <template v-else-if="page === 'hotkeys'">
                <h2 class="page-title">快捷键</h2>
                <p class="page-desc">全局快捷键，点击后按下新组合即可修改。</p>
                <div class="settings-card">
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
                </div>
                <p v-if="hotkeyError" class="error">{{ hotkeyError }}</p>
                <div class="reset-line">
                  <button type="button" class="btn ghost" @pointerdown="resetHotkeys">恢复默认快捷键</button>
                </div>
              </template>

              <!-- 关于 -->
              <template v-else>
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
              </template>
            </section>
          </Transition>
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

/* ---------- 自绘标题栏 ---------- */
.titlebar {
  flex-shrink: 0;
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-left: 14px;
  background: var(--surface-raised);
  border-bottom: 1px solid var(--line);
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
  transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
}
.tb-btn:hover {
  background: var(--surface-hover);
  color: var(--ink);
}
.tb-btn.close:hover {
  background: var(--danger);
  color: var(--on-danger);
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
  background: var(--surface-raised);
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
  width: 172px;
  flex-shrink: 0;
  background: var(--surface-raised);
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
  padding: 0 10px 18px;
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
  gap: 10px;
  border: 0;
  background: transparent;
  width: 100%;
  text-align: left;
  padding: 8px 12px;
  border-radius: 8px;
  font-size: 13px;
  color: var(--ink-2);
  cursor: pointer;
  font-family: inherit;
  transition: background 160ms var(--ease-out), color 160ms var(--ease-out);
}
.nav-ico {
  flex-shrink: 0;
  opacity: 0.85;
}
.nav-item:hover {
  background: var(--surface-hover);
  color: var(--ink);
}
.nav-item.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}
.nav-item.active .nav-ico {
  opacity: 1;
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
  margin: 0 0 4px;
}
.page-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 20px;
}
.page-desc {
  font-size: 12.5px;
  color: var(--ink-3);
  line-height: 1.65;
  margin: 0 0 18px;
}
.page-head .page-desc {
  margin-bottom: 18px;
}
.sep {
  height: 1px;
  background: var(--line);
  margin: 0 16px;
}
.settings-card {
  border: 1px solid var(--line);
  border-radius: 12px;
  background: var(--surface-raised);
  overflow: hidden;
}
.settings-card :deep(.row) {
  padding: 14px 16px;
}

/* ---------- 页面切换 ---------- */
.page-enter-active,
.page-leave-active {
  transition: opacity 170ms ease, transform 240ms var(--ease-out);
}
.page-enter-from {
  opacity: 0;
  transform: translateY(8px);
}
.page-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

/* ---------- 控件 ---------- */
.btn {
  border: 1px solid var(--line-strong);
  background: var(--surface-raised);
  color: var(--ink);
  border-radius: 8px;
  padding: 6px 13px;
  font-size: 12.5px;
  font-weight: 550;
  cursor: pointer;
  font-family: inherit;
  transition: background 150ms var(--ease-out), border-color 150ms var(--ease-out),
    transform 100ms var(--ease-out);
}
.btn:active {
  transform: scale(0.97);
}
.btn:hover {
  background: var(--surface-hover);
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
  background: var(--surface-raised);
  color: var(--ink);
  outline: none;
  font-family: inherit;
}
.input:focus {
  border-color: var(--accent);
  box-shadow: 0 0 0 3px var(--accent-soft);
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
.input.sel {
  min-width: 140px;
}
.slider {
  width: 150px;
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
  background: var(--surface-raised);
  padding: 16px 18px;
  box-shadow: 0 8px 24px -22px rgb(0 0 0 / 0.55);
}
.pod-card.off {
  opacity: 0.6;
}
.pod-head {
  display: flex;
  align-items: center;
  gap: 10px;
  padding-bottom: 14px;
  border-bottom: 1px solid var(--line);
  margin-bottom: 12px;
}
.pod-name-input {
  border: 0;
  background: transparent;
  font-size: 15px;
  font-weight: 650;
  color: var(--ink);
  outline: none;
  font-family: inherit;
  padding: 2px 4px;
  border-radius: 6px;
}
.pod-name-input:focus {
  background: var(--surface-hover);
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
  color: var(--ink-3);
  width: 28px;
  height: 28px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  font-family: inherit;
  transition: background 150ms var(--ease-out), color 150ms var(--ease-out);
}
.op-btn:hover {
  background: var(--surface-hover);
  color: var(--ink);
}
.op-btn.danger:hover {
  background: color-mix(in oklab, var(--danger) 14%, transparent);
  color: var(--danger);
}

/* 分组字段：标签左对齐，控件右对齐，分区标题建立扫视结构 */
.pod-groups {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.group-title {
  font-size: 11px;
  font-weight: 650;
  letter-spacing: 0.05em;
  color: var(--ink-3);
  margin: 10px 0 2px;
}
.pod-group:first-child .group-title {
  margin-top: 0;
}
.frow {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 20px;
  padding: 5px 0;
}
.flabel {
  font-size: 13px;
  color: var(--ink-2);
  white-space: nowrap;
}
.fctrl {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.fval {
  min-width: 40px;
  text-align: right;
  font-size: 12px;
  color: var(--ink-3);
  font-variant-numeric: tabular-nums;
}
.folder-row .folder-line {
  max-width: 460px;
}

/* ---------- 匣列表过渡 ---------- */
.pod-enter-active {
  transition: opacity 220ms ease, transform 280ms var(--ease-out);
}
.pod-enter-from {
  opacity: 0;
  transform: translateY(10px);
}
.pod-leave-active {
  transition: opacity 140ms ease;
}
.pod-leave-to {
  opacity: 0;
}
.pod-move {
  transition: transform 280ms var(--ease-out);
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
  background: var(--surface-raised);
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
  transition: opacity 180ms ease, transform 240ms var(--ease-out);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}
</style>
