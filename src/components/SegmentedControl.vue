<script setup lang="ts">
/** 分段选择（iOS 风格）：当前段平滑滑动 */
import { computed } from "vue";

const props = defineProps<{
  options: { value: string; label: string }[];
  modelValue: string;
}>();
const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

const index = computed(() =>
  Math.max(
    0,
    props.options.findIndex((o) => o.value === props.modelValue),
  ),
);
const pct = computed(() => (100 / props.options.length) * index.value);
</script>

<template>
  <div class="seg" role="radiogroup">
    <div class="seg-thumb" :style="{ width: `calc(${100 / options.length}% - 4px)`, transform: `translateX(${pct}%)` }" />
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      role="radio"
      :aria-checked="o.value === modelValue"
      class="seg-item"
      @pointerdown="emit('update:modelValue', o.value)"
    >
      {{ o.label }}
    </button>
  </div>
</template>

<style scoped>
.seg {
  position: relative;
  display: flex;
  padding: 2px;
  background: var(--surface-2);
  border-radius: 9px;
  width: fit-content;
}
.seg-thumb {
  position: absolute;
  top: 2px;
  left: 2px;
  height: calc(100% - 4px);
  background: var(--surface);
  border-radius: 7px;
  box-shadow: 0 1px 4px oklch(0.2 0.02 230 / 0.18);
  transition: transform 260ms cubic-bezier(0.25, 1, 0.4, 1);
}
.seg-item {
  position: relative;
  z-index: 1;
  border: 0;
  background: transparent;
  padding: 4px 12px;
  font-size: 12.5px;
  color: var(--ink-2);
  cursor: pointer;
  border-radius: 7px;
  transition: color 160ms ease;
  font-family: inherit;
}
.seg-item[aria-checked="true"] {
  color: var(--ink);
  font-weight: 550;
}
</style>
