<script setup lang="ts">
/** 图片缩略图：走 Rust 命令读取字节（仅限暂存文件夹内的图片），非图片回落为图形 */
import { onMounted, ref, watch } from "vue";
import { ipc } from "@/lib/ipc";
import type { ItemKind } from "@/types";
import FileGlyph from "./FileGlyph.vue";

const props = defineProps<{
  kind: ItemKind;
  path: string;
  ext: string | null;
  name: string;
}>();

const url = ref<string | null>(null);
const failed = ref(false);

const IMAGE_EXTS = ["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico"];

async function load() {
  url.value = null;
  failed.value = false;
  if (!IMAGE_EXTS.includes((props.ext ?? "").toLowerCase())) return;
  try {
    const payload = await ipc.readThumbnail(props.path);
    if (payload) {
      const blob = new Blob([new Uint8Array(payload.bytes)], { type: payload.mime });
      url.value = URL.createObjectURL(blob);
    } else {
      failed.value = true;
    }
  } catch {
    failed.value = true;
  }
}

onMounted(load);
watch(() => props.path, load);
</script>

<template>
  <div class="thumb-box">
    <img v-if="url" :src="url" :alt="name" class="thumb-img" draggable="false" />
    <FileGlyph v-else :kind="kind" :ext="ext" :size="22" class="thumb-glyph" />
  </div>
</template>

<style scoped>
.thumb-box {
  width: 44px;
  height: 44px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  background: var(--surface-2);
  overflow: hidden;
}
.dark .thumb-box {
  background: var(--surface-3);
}
.thumb-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
.thumb-glyph {
  color: var(--ink-3);
}
</style>
