<script setup lang="ts">
/** 面板头部场景切换：下拉列出场景 + 新建 */
import { onBeforeUnmount, onMounted, ref } from "vue";
import { ipc } from "@/lib/ipc";
import type { Scene } from "@/types";

const props = defineProps<{ scenes: Scene[]; activeId: number }>();
const emit = defineEmits<{ (e: "changed"): void }>();

const open = ref(false);
const creating = ref(false);
const newName = ref("");
const root = ref<HTMLElement | null>(null);

function toggle() {
  open.value = !open.value;
  creating.value = false;
}

async function pick(s: Scene) {
  open.value = false;
  if (s.id !== props.activeId) {
    await ipc.setActiveScene(s.id);
    emit("changed");
  }
}

async function create() {
  const name = newName.value.trim();
  if (!name) return;
  const s = await ipc.createScene(name);
  newName.value = "";
  creating.value = false;
  open.value = false;
  await ipc.setActiveScene(s.id);
  emit("changed");
}

function onDocDown(e: MouseEvent) {
  if (open.value && root.value && !root.value.contains(e.target as Node)) {
    open.value = false;
    creating.value = false;
  }
}

onMounted(() => document.addEventListener("pointerdown", onDocDown, true));
onBeforeUnmount(() => document.removeEventListener("pointerdown", onDocDown, true));
</script>

<template>
  <div ref="root" class="scene-switcher">
    <button type="button" class="scene-btn" @pointerdown.stop="toggle">
      <span class="scene-name">{{ scenes.find((s) => s.id === activeId)?.name ?? "场景" }}</span>
      <svg :class="{ flipped: open }" width="10" height="10" viewBox="0 0 10 10" fill="none">
        <path d="M2 4l3 3 3-3" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>

    <Transition name="pop">
      <div v-if="open" class="scene-menu">
        <button
          v-for="s in scenes"
          :key="s.id"
          type="button"
          class="scene-item"
          :class="{ active: s.id === activeId }"
          @pointerdown.stop="pick(s)"
        >
          <span class="scene-check" :class="{ on: s.id === activeId }" />
          {{ s.name }}
        </button>
        <div class="menu-sep" />
        <div v-if="creating" class="scene-create">
          <input
            v-model="newName"
            class="scene-input"
            placeholder="场景名称"
            maxlength="12"
            @keydown.enter.prevent="create"
            ref="inputEl"
          />
          <button type="button" class="mini-btn" @pointerdown.stop.prevent="create">新建</button>
        </div>
        <button v-else type="button" class="scene-item new" @pointerdown.stop="creating = true">
          <svg width="11" height="11" viewBox="0 0 11 11" fill="none">
            <path d="M5.5 2v7M2 5.5h7" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
          </svg>
          新建场景
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.scene-switcher {
  position: relative;
}
.scene-btn {
  display: flex;
  align-items: center;
  gap: 6px;
  border: 0;
  background: var(--surface-2);
  border-radius: 8px;
  padding: 5px 10px;
  font-size: 12.5px;
  font-weight: 550;
  color: var(--ink);
  cursor: pointer;
  font-family: inherit;
  transition: background 140ms ease;
}
.scene-btn:hover {
  background: var(--surface-3);
}
.scene-btn svg {
  color: var(--ink-3);
  transition: transform 180ms ease;
}
.scene-btn svg.flipped {
  transform: rotate(180deg);
}
.scene-menu {
  position: absolute;
  top: calc(100% + 8px);
  left: 0;
  min-width: 148px;
  background: var(--surface);
  border-radius: 12px;
  box-shadow: var(--shadow-pop), 0 0 0 1px var(--line);
  padding: 5px;
  z-index: 30;
}
.scene-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  border: 0;
  background: transparent;
  border-radius: 8px;
  padding: 7px 9px;
  font-size: 12.5px;
  color: var(--ink);
  cursor: pointer;
  text-align: left;
  font-family: inherit;
  transition: background 120ms ease;
}
.scene-item:hover {
  background: var(--surface-2);
}
.scene-item.active {
  font-weight: 600;
}
.scene-item.new {
  color: var(--ink-2);
}
.scene-check {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: transparent;
}
.scene-check.on {
  background: var(--accent);
}
.menu-sep {
  height: 1px;
  background: var(--line);
  margin: 5px 7px;
}
.scene-create {
  display: flex;
  gap: 6px;
  padding: 4px;
  align-items: center;
}
.scene-input {
  flex: 1;
  min-width: 0;
  border: 1px solid var(--line-strong);
  border-radius: 7px;
  padding: 5px 8px;
  font-size: 12.5px;
  background: var(--surface);
  color: var(--ink);
  outline: none;
  font-family: inherit;
}
.scene-input:focus {
  border-color: var(--accent);
}
.mini-btn {
  border: 0;
  background: var(--accent);
  color: var(--on-accent);
  border-radius: 7px;
  padding: 5px 9px;
  font-size: 12px;
  font-weight: 550;
  cursor: pointer;
  font-family: inherit;
}

.pop-enter-active,
.pop-leave-active {
  transition: opacity 150ms ease, transform 180ms cubic-bezier(0.3, 1, 0.4, 1);
}
.pop-enter-from,
.pop-leave-to {
  opacity: 0;
  transform: translateY(-4px) scale(0.98);
}
</style>
