import { defineStore } from "pinia";
import { ipc } from "@/lib/ipc";
import { Events, listen } from "@/lib/events";
import type { ExportMode, StagedItem } from "@/types";

/** 暂存数据：条目 + 选中态（按当前匣过滤） */
export const useStagingStore = defineStore("staging", {
  state: () => ({
    items: [] as StagedItem[],
    activePodId: 0,
    selectedIds: new Set<number>(),
    lastError: "" as string,
  }),

  getters: {
    activeItems(state): StagedItem[] {
      return state.items
        .filter((it) => it.podId === state.activePodId)
        .sort((a, b) => b.createdAt - a.createdAt);
    },
    selectedItems(state): StagedItem[] {
      return state.items.filter((it) => state.selectedIds.has(it.id));
    },
  },

  actions: {
    setActivePod(id: number) {
      this.activePodId = id;
      this.selectedIds.clear();
    },

    async refresh(podId?: number) {
      const pid = podId ?? this.activePodId;
      if (!pid) return;
      const items = await ipc.listPodItems(pid);
      this.items = items;
    },

    toggleSelected(id: number, additive: boolean) {
      if (!additive) this.selectedIds.clear();
      if (this.selectedIds.has(id)) this.selectedIds.delete(id);
      else this.selectedIds.add(id);
    },

    selectAll() {
      this.selectedIds = new Set(this.activeItems.map((i) => i.id));
    },

    clearSelection() {
      this.selectedIds.clear();
    },

    async removeItems(ids: number[], deleteFiles: boolean) {
      await ipc.removeItems(ids, deleteFiles);
      ids.forEach((id) => this.selectedIds.delete(id));
      await this.refresh();
    },

    async clearActivePod(deleteFiles: boolean) {
      const ids = this.activeItems.map((i) => i.id);
      if (ids.length) await ipc.removeItems(ids, deleteFiles);
      this.selectedIds.clear();
      await this.refresh();
    },

    /** 返回冲突文件名列表；为空表示已完成 */
    async exportItems(ids: number[], destDir: string, mode: ExportMode): Promise<string[]> {
      return ipc.exportItems(ids, destDir, mode, "ask");
    },

    async listenChanges(podId: number) {
      listen<{ podId: number }>(Events.ItemsChanged, (p) => {
        if (!p.podId || p.podId === podId) void this.refresh(podId);
      });
    },
  },
});
