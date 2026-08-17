<script setup lang="ts">
/**
 * 分段选择（iOS 风格）：当前段平滑滑动。
 * 缩略图按活动按钮的实际位置与宽度像素级对齐（translateX 百分比相对自身宽度，
 * 按钮文案长短不一，无法用等分百分比对齐）。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

const props = defineProps<{
  options: { value: string; label: string }[];
  modelValue: string;
}>();
const emit = defineEmits<{ (e: "update:modelValue", v: string): void }>();

const segRef = ref<HTMLElement | null>(null);
const thumbStyle = ref<{ width: string; transform: string }>({
  width: "0px",
  transform: "translateX(0px)",
});

const index = computed(() =>
  Math.max(0, props.options.findIndex((o) => o.value === props.modelValue)),
);

let ro: ResizeObserver | null = null;

function positionThumb() {
  const seg = segRef.value;
  if (!seg) return;
  const btn = seg.querySelector<HTMLElement>(".seg-item.active");
  if (!btn) return;
  thumbStyle.value = {
    width: `${btn.offsetWidth}px`,
    transform: `translateX(${btn.offsetLeft}px)`,
  };
}

onMounted(() => {
  positionThumb();
  ro = new ResizeObserver(() => positionThumb());
  if (segRef.value) ro.observe(segRef.value);
});
onBeforeUnmount(() => ro?.disconnect());

watch([index, () => props.options], async () => {
  await nextTick();
  positionThumb();
});
</script>

<template>
  <div ref="segRef" class="seg" role="radiogroup">
    <div class="seg-thumb" :style="thumbStyle" />
    <button
      v-for="o in options"
      :key="o.value"
      type="button"
      role="radio"
      :aria-checked="o.value === modelValue"
      class="seg-item"
      :class="{ active: o.value === modelValue }"
      @pointerdown="emit('update:modelValue', o.value)"
    >
      {{ o.label }}
    </button>
  </div>
</template>

<style scoped>
.seg {
  position: relative;
  display: inline-flex;
  padding: 2px;
  background: var(--surface-2);
  border-radius: 9px;
  width: fit-content;
}
.seg-thumb {
  position: absolute;
  top: 2px;
  left: 0;
  height: calc(100% - 4px);
  background: var(--surface);
  border-radius: 7px;
  box-shadow: 0 1px 4px oklch(0.2 0.02 230 / 0.18);
  transition: transform 260ms cubic-bezier(0.25, 1, 0.4, 1), width 260ms
    cubic-bezier(0.25, 1, 0.4, 1);
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
  white-space: nowrap;
}
.seg-item.active {
  color: var(--ink);
  font-weight: 550;
}
</style>
