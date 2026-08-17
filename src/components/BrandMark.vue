<script setup lang="ts">
/** 品牌图形：官方 FP Logo（去白底），按高度等比缩放。 */
import logoUrl from "@/assets/fp-logo.png";
withDefaults(defineProps<{ size?: number }>(), { size: 24 });
</script>

<template>
  <img
    :src="logoUrl"
    :style="{ height: size + 'px' }"
    class="brand-logo"
    alt="浮匣"
    draggable="false"
  />
</template>

<style scoped>
.brand-logo {
  width: auto;
  display: block;
  user-select: none;
}
</style>
