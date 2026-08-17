<script setup lang="ts">
/** 设置行：左侧标签 + 说明，右侧控件插槽 */
defineProps<{ label: string; hint?: string }>();
</script>

<template>
  <div class="row">
    <div class="row-text">
      <div class="row-label">{{ label }}</div>
      <div v-if="hint" class="row-hint">{{ hint }}</div>
    </div>
    <div class="row-control">
      <slot />
    </div>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 24px;
  padding: 14px 2px;
}
.row-text {
  min-width: 0;
}
.row-label {
  font-size: 13.5px;
  font-weight: 550;
  color: var(--ink);
}
.row-hint {
  font-size: 12px;
  color: var(--ink-3);
  margin-top: 3px;
  line-height: 1.55;
}
.row-control {
  flex-shrink: 0;
}
</style>
