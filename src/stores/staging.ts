import { defineStore } from "pinia";
import { ipc } from "@/lib/ipc";
import { Events, listen } from "@/lib/events";
import type { ExportMode, Scene, StagedItem } from "@/types";

/** 暂存数据：场景 + 条目 + 选中态（按当前场景过滤） */
export const useStagingStore = defineStore("staging", {
  state: () => ({
    items: [] as StagedItem[],
    scenes: [] as Scene[],
    activeSceneId: 0,
    selectedIds: new Set<number>(),
    lastError: "" as string,
  }),

  getters: {
    activeItems(state): StagedItem[] {
      return state.items
        .filter((it) => it.sceneId === state.activeSceneId)
        .sort((a, b) => b.createdAt - a.createdAt);
    },
    selectedItems(state): StagedItem[] {
      return state.items.filter((it) => state.selectedIds.has(it.id));
    },
  },

  actions: {
    setActiveScene(id: number) {
      this.activeSceneId = id;
      this.selectedIds.clear();
    },

    async refresh() {
      const [items, scenes] = await Promise.all([ipc.listItems(), ipc.listScenes()]);
      this.items = items;
      this.scenes = scenes;
      if (!this.scenes.some((s) => s.id === this.activeSceneId)) {
        this.activeSceneId = scenes[0]?.id ?? 0;
      }
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

    async clearActiveScene(deleteFiles: boolean) {
      const ids = this.activeItems.map((i) => i.id);
      if (ids.length) await ipc.removeItems(ids, deleteFiles);
      this.selectedIds.clear();
      await this.refresh();
    },

    /** 返回冲突文件名列表；为空表示已完成 */
    async exportItems(ids: number[], destDir: string, mode: ExportMode): Promise<string[]> {
      return ipc.exportItems(ids, destDir, mode, "ask");
    },

    async listenChanges() {
      listen<void>(Events.ItemsChanged, () => {
        void this.refresh();
      });
    },
  },
});
