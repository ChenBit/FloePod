<script setup lang="ts">
/**
 * 设置窗口：常规 / 外观 / 交互 / 快捷键 / 场景 / 关于。
 * 所有修改即时保存（save -> Rust 持久化并广播 settings_changed）。
 */
import { computed, onMounted, ref } from "vue";
import { ipc } from "@/lib/ipc";
import { useSettingsStore } from "@/stores/settings";
import { useStagingStore } from "@/stores/staging";
import SegmentedControl from "@/components/SegmentedControl.vue";
import ToggleSwitch from "@/components/ToggleSwitch.vue";
import SettingsRow from "@/components/SettingsRow.vue";
import HotkeyRecorder from "@/components/HotkeyRecorder.vue";
import BrandMark from "@/components/BrandMark.vue";

const settingsStore = useSettingsStore();
const staging = useStagingStore();

const page = ref<"general" | "look" | "behavior" | "hotkeys" | "scenes" | "about">("general");
const toast = ref("");
const hotkeyError = ref("");
const renaming = ref<number | null>(null);
const renameValue = ref("");
const newScene = ref("");

const s = computed(() => settingsStore.settings);
const firstRun = computed(() => s.value && !s.value.firstRunDone);

const PAGES = [
  { id: "general", label: "常规" },
  { id: "look", label: "外观" },
  { id: "behavior", label: "交互" },
  { id: "hotkeys", label: "快捷键" },
  { id: "scenes", label: "场景" },
  { id: "about", label: "关于" },
] as const;

function showToast(msg: string) {
  toast.value = msg;
  window.setTimeout(() => (toast.value = ""), 2400);
}

async function save(patch: Parameters<typeof settingsStore.save>[0]) {
  try {
    await settingsStore.save(patch);
  } catch (err) {
    console.error(err);
    showToast("保存失败，请重试");
  }
}

/* ---------- 暂存文件夹 ---------- */
async function chooseFolder() {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const dir = await open({ directory: true, multiple: false, title: "选择暂存文件夹" });
  if (typeof dir === "string") await save({ stagingFolder: dir });
}

async function openStagingFolder() {
  if (!s.value?.stagingFolder) return;
  const { revealItemInDir } = await import("@tauri-apps/plugin-opener");
  /* 打开文件夹本身 */
  const { openPath } = await import("@tauri-apps/plugin-opener");
  await openPath(s.value.stagingFolder);
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

/* ---------- 场景 ---------- */
async function addScene() {
  const name = newScene.value.trim();
  if (!name) return;
  const scene = await ipc.createScene(name);
  newScene.value = "";
  await staging.refresh();
  await ipc.setActiveScene(scene.id);
}

async function startRename(id: number, name: string) {
  renaming.value = id;
  renameValue.value = name;
}

async function commitRename(id: number) {
  const name = renameValue.value.trim();
  renaming.value = null;
  if (name) {
    await ipc.renameScene(id, name);
    await staging.refresh();
  }
}

async function removeScene(id: number) {
  if (staging.scenes.length <= 1) return;
  await ipc.deleteScene(id);
  await staging.refresh();
}

async function activateScene(id: number) {
  await ipc.setActiveScene(id);
  await staging.refresh();
}

function sceneCount(id: number): number {
  return staging.items.filter((i) => i.sceneId === id).length;
}

onMounted(async () => {
  await settingsStore.load();
  await staging.refresh();
  staging.setActiveScene(settingsStore.settings?.activeSceneId ?? 0);
  settingsStore.listenChanges();
  staging.listenChanges();
  if (firstRun.value) page.value = "general";
});
</script>

<template>
  <div class="settings-root" v-if="s">
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
          <span v-if="p.id === 'general' && firstRun" class="nav-dot" />
        </button>
      </nav>
      <div class="nav-foot">FloePod · {{ s.version }}</div>
    </aside>

    <main class="content">
      <!-- 首启引导 -->
      <div v-if="firstRun && page === 'general'" class="first-run">
        <div class="fr-title">欢迎使用浮匣</div>
        <div class="fr-text">先选一个暂存文件夹，之后把任何文件拖到屏幕边缘的浮匣就能暂存。</div>
      </div>

      <section v-show="page === 'general'">
        <h2 class="page-title">常规</h2>
        <SettingsRow label="暂存文件夹" hint="拖入的文件会复制 / 移动到这里集中保管">
          <div class="folder-picker">
            <div class="folder-path" :title="s.stagingFolder ?? ''">
              {{ s.stagingFolder ?? "未选择" }}
            </div>
            <button type="button" class="btn" @pointerdown="chooseFolder">选择…</button>
            <button
              v-if="s.stagingFolder"
              type="button"
              class="btn ghost"
              @pointerdown="openStagingFolder"
            >
              打开
            </button>
          </div>
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="拖入时" hint="按住 Ctrl / Shift / Alt 拖入可临时跳过此设置">
          <SegmentedControl
            :options="[
              { value: 'ask', label: '每次询问' },
              { value: 'copy', label: '复制' },
              { value: 'move', label: '移动' },
              { value: 'shortcut', label: '快捷方式' },
            ]"
            :model-value="s.dropAction"
            @update:model-value="(v) => save({ dropAction: v as never })"
          />
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="开机自启" hint="以托盘常驻方式随 Windows 启动">
          <ToggleSwitch
            :model-value="s.autostart"
            @update:model-value="(v) => save({ autostart: v })"
          />
        </SettingsRow>
      </section>

      <section v-show="page === 'look'">
        <h2 class="page-title">外观</h2>
        <SettingsRow label="形态" hint="浮动条贴满屏幕边缘；浮动书签是边缘的一枚小胶囊">
          <SegmentedControl
            :options="[
              { value: 'strip', label: '浮动条' },
              { value: 'bookmark', label: '浮动书签' },
            ]"
            :model-value="s.barForm"
            @update:model-value="(v) => save({ barForm: v as never })"
          />
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="屏幕边缘">
          <SegmentedControl
            :options="[
              { value: 'left', label: '左侧' },
              { value: 'right', label: '右侧' },
            ]"
            :model-value="s.edge"
            @update:model-value="(v) => save({ edge: v as never })"
          />
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="不透明度">
          <input
            type="range"
            class="slider"
            min="0.55"
            max="1"
            step="0.05"
            :value="s.opacity"
            @input="(e) => save({ opacity: Number((e.target as HTMLInputElement).value) })"
          />
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="材质" hint="亚克力为系统级背景模糊；纯半透明不模糊、开销最低">
          <SegmentedControl
            :options="[
              { value: 'acrylic', label: '亚克力' },
              { value: 'plain', label: '纯半透明' },
            ]"
            :model-value="s.material"
            @update:model-value="(v) => save({ material: v as never })"
          />
        </SettingsRow>
        <div class="sep" />
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
      </section>

      <section v-show="page === 'behavior'">
        <h2 class="page-title">交互</h2>
        <SettingsRow label="悬停展开延迟" hint="鼠标在浮匣上停留多久后弹出面板">
          <div class="range-line">
            <input
              type="range"
              class="slider"
              min="0"
              max="400"
              step="20"
              :value="s.hoverDelayMs"
              @input="(e) => save({ hoverDelayMs: Number((e.target as HTMLInputElement).value) })"
            />
            <span class="range-val">{{ s.hoverDelayMs }} ms</span>
          </div>
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="面板宽度">
          <div class="range-line">
            <input
              type="range"
              class="slider"
              min="320"
              max="480"
              step="10"
              :value="s.panelWidth"
              @input="(e) => save({ panelWidth: Number((e.target as HTMLInputElement).value) })"
            />
            <span class="range-val">{{ s.panelWidth }} px</span>
          </div>
        </SettingsRow>
      </section>

      <section v-show="page === 'hotkeys'">
        <h2 class="page-title">快捷键</h2>
        <SettingsRow label="显示 / 隐藏浮匣">
          <HotkeyRecorder
            :model-value="s.hotkeys.toggleBar"
            @update:model-value="(v) => saveHotkey('toggleBar', v)"
          />
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="收集剪贴板文字" hint="把当前剪贴板里的文字存为一则暂存">
          <HotkeyRecorder
            :model-value="s.hotkeys.collectClipboard"
            @update:model-value="(v) => saveHotkey('collectClipboard', v)"
          />
        </SettingsRow>
        <div class="sep" />
        <SettingsRow label="打开面板">
          <HotkeyRecorder
            :model-value="s.hotkeys.openPanel"
            @update:model-value="(v) => saveHotkey('openPanel', v)"
          />
        </SettingsRow>
        <p v-if="hotkeyError" class="error">{{ hotkeyError }}</p>
        <div class="reset-line">
          <button type="button" class="btn ghost" @pointerdown="resetHotkeys">恢复默认快捷键</button>
        </div>
      </section>

      <section v-show="page === 'scenes'">
        <h2 class="page-title">场景</h2>
        <p class="page-desc">场景是同一暂存文件夹下的分组，比如「工作素材」「个人文件」。切换场景后，拖入与面板只显示该场景的内容。</p>
        <div class="scene-list">
          <div
            v-for="sc in staging.scenes"
            :key="sc.id"
            class="scene-card"
            :class="{ active: sc.id === staging.activeSceneId }"
          >
            <button type="button" class="scene-main" @pointerdown="activateScene(sc.id)">
              <span class="scene-check" :class="{ on: sc.id === staging.activeSceneId }" />
              <span class="scene-name">{{ sc.name }}</span>
              <span class="scene-count">{{ sceneCount(sc.id) }} 项</span>
            </button>
            <div class="scene-ops">
              <button
                v-if="renaming !== sc.id"
                type="button"
                class="op-btn"
                title="重命名"
                @pointerdown="startRename(sc.id, sc.name)"
              >改名</button>
              <template v-else>
                <input
                  v-model="renameValue"
                  class="rename-input"
                  maxlength="12"
                  @keydown.enter.prevent="commitRename(sc.id)"
                />
                <button type="button" class="op-btn" @pointerdown="commitRename(sc.id)">保存</button>
              </template>
              <button
                v-if="staging.scenes.length > 1"
                type="button"
                class="op-btn danger"
                title="删除场景（其中的暂存文件一并移出）"
                @pointerdown="removeScene(sc.id)"
              >删除</button>
            </div>
          </div>
        </div>
        <div class="scene-new">
          <input v-model="newScene" class="new-input" placeholder="新场景名称" maxlength="12" @keydown.enter.prevent="addScene" />
          <button type="button" class="btn" :disabled="!newScene.trim()" @pointerdown="addScene">新建场景</button>
        </div>
      </section>

      <section v-show="page === 'about'">
        <h2 class="page-title">关于</h2>
        <div class="about-hero">
          <BrandMark :size="44" class="about-brand" />
          <div class="about-name">浮匣 FloePod</div>
          <div class="about-ver">版本 {{ s.version }}</div>
        </div>
        <p class="about-text">
          本地优先的屏幕边缘暂存工具：拖进来集中保管，拖出去继续使用。
          不联网、不收集数据，所有内容只存在你自己的电脑上。
        </p>
        <SettingsRow label="数据位置" hint="数据库与配置文件所在目录（便携版跟随程序目录）">
          <span class="data-path">{{ s.dataDir }}</span>
        </SettingsRow>
      </section>
    </main>

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
  background: var(--surface);
  color: var(--ink);
}
.nav {
  width: 168px;
  flex-shrink: 0;
  background: var(--surface-2);
  display: flex;
  flex-direction: column;
  padding: 18px 10px 14px;
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
  background: var(--surface-3);
  color: var(--ink);
}
.nav-item.active {
  background: var(--surface);
  color: var(--ink);
  font-weight: 600;
  box-shadow: 0 1px 4px oklch(0.2 0.02 230 / 0.1);
}
.nav-dot {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: var(--accent);
  margin-left: auto;
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
.page-desc {
  font-size: 12.5px;
  color: var(--ink-3);
  line-height: 1.65;
  margin: 0 0 16px;
}
.sep {
  height: 1px;
  background: var(--line);
}

.first-run {
  margin-bottom: 18px;
  padding: 14px 16px;
  border-radius: 12px;
  background: var(--accent-soft);
}
.fr-title {
  font-size: 14px;
  font-weight: 650;
  color: var(--accent);
  margin-bottom: 3px;
}
.fr-text {
  font-size: 12.5px;
  color: var(--ink-2);
}

.folder-picker {
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 340px;
}
.folder-path {
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--ink-2);
  border: 1px solid var(--line);
  border-radius: 8px;
  padding: 6px 10px;
  background: var(--surface-2);
}
.btn {
  border: 1px solid var(--line-strong);
  background: var(--surface);
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
}

.slider {
  width: 170px;
  accent-color: var(--accent);
}
.range-line {
  display: flex;
  align-items: center;
  gap: 10px;
}
.range-val {
  font-size: 12px;
  color: var(--ink-2);
  min-width: 52px;
  text-align: right;
}

.error {
  font-size: 12px;
  color: var(--danger);
  margin: 10px 0 0;
}
.reset-line {
  margin-top: 18px;
}

.scene-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 16px;
}
.scene-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  border: 1px solid var(--line);
  border-radius: 10px;
  padding: 4px 6px 4px 4px;
}
.scene-card.active {
  border-color: color-mix(in oklab, var(--accent) 55%, transparent);
  background: var(--accent-soft);
}
.scene-main {
  flex: 1;
  display: flex;
  align-items: center;
  gap: 10px;
  border: 0;
  background: transparent;
  padding: 8px;
  border-radius: 8px;
  cursor: pointer;
  font-family: inherit;
  min-width: 0;
}
.scene-check {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: transparent;
  flex-shrink: 0;
}
.scene-check.on {
  background: var(--accent);
}
.scene-name {
  font-size: 13.5px;
  font-weight: 560;
  color: var(--ink);
}
.scene-count {
  font-size: 11.5px;
  color: var(--ink-3);
}
.scene-ops {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
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
.rename-input,
.new-input {
  border: 1px solid var(--line-strong);
  border-radius: 7px;
  padding: 5px 9px;
  font-size: 12.5px;
  background: var(--surface);
  color: var(--ink);
  outline: none;
  font-family: inherit;
  width: 130px;
}
.rename-input:focus,
.new-input:focus {
  border-color: var(--accent);
}
.scene-new {
  display: flex;
  gap: 8px;
}

.about-hero {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
  padding: 18px 0 6px;
}
.about-brand {
  color: var(--accent);
}
.about-name {
  font-size: 17px;
  font-weight: 650;
  letter-spacing: -0.015em;
}
.about-ver {
  font-size: 12px;
  color: var(--ink-3);
}
.about-text {
  font-size: 13px;
  line-height: 1.75;
  color: var(--ink-2);
  max-width: 420px;
  margin: 8px auto 22px;
  text-align: center;
}
.data-path {
  font-size: 12px;
  color: var(--ink-2);
  max-width: 260px;
  overflow-wrap: anywhere;
  text-align: right;
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
