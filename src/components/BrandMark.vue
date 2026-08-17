<script setup lang="ts">
/**
 * 品牌图形：浮冰上的匣子。
 * 一枚圆角匣（匣），上方悬浮一块小冰板（浮）——几何表达，不使用渐变。
 */
withDefaults(defineProps<{ size?: number }>(), { size: 24 });
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 32 32"
    fill="none"
    aria-hidden="true"
  >
    <!-- 悬浮的冰板 -->
    <rect x="17" y="4" width="11" height="5.5" rx="2" fill="currentColor" opacity="0.95" />
    <!-- 匣体 -->
    <rect
      x="4.5"
      y="12"
      width="23"
      height="15.5"
      rx="4"
      stroke="currentColor"
      stroke-width="2.4"
    />
    <!-- 匣口 -->
    <path d="M11 18.5h10" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" opacity="0.55" />
  </svg>
</template>
