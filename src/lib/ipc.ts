import type {
  Bootstrap,
  ConflictStrategy,
  DropAction,
  ExportMode,
  Hotkeys,
  ModifierState,
  PanelMode,
  Scene,
  Settings,
  StagedItem,
  ThumbnailPayload,
} from "@/types";

/**
 * Tauri IPC 封装。
 * 在浏览器中（vite dev）自动切换到 mock 实现，便于无 Tauri 环境的 UI 开发。
 */

const inTauri = "__TAURI_INTERNALS__" in window;

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

export const ipc = {
  inTauri,

  getBootstrap: (): Promise<Bootstrap> => invoke("get_bootstrap"),

  // ---- 拖入 ----
  getModifierState: (): Promise<ModifierState> => invoke("get_modifier_state"),
  /** 拖入待询问：暂存路径并弹出面板 ask 模式 */
  holdPendingDrop: (paths: string[]): Promise<void> =>
    invoke("hold_pending_drop", { paths }),
  /** 确认动作后执行暂存 */
  stagePaths: (paths: string[], action: DropAction): Promise<StagedItem[]> =>
    invoke("stage_paths", { paths, action }),
  stageText: (content: string): Promise<StagedItem> => invoke("stage_text", { content }),

  // ---- 列表 ----
  listItems: (): Promise<StagedItem[]> => invoke("list_items"),
  removeItems: (ids: number[], deleteFiles: boolean): Promise<void> =>
    invoke("remove_items", { ids, deleteFiles }),

  // ---- 导出 ----
  exportItems: (
    ids: number[],
    destDir: string,
    mode: ExportMode,
    onConflict: ConflictStrategy,
  ): Promise<string[]> => invoke("export_items", { ids, destDir, mode, onConflict }),

  // ---- 缩略图 ----
  readThumbnail: (path: string): Promise<ThumbnailPayload | null> =>
    invoke("read_thumbnail", { path }),

  // ---- 场景 ----
  listScenes: (): Promise<Scene[]> => invoke("list_scenes"),
  createScene: (name: string): Promise<Scene> => invoke("create_scene", { name }),
  renameScene: (id: number, name: string): Promise<void> => invoke("rename_scene", { id, name }),
  deleteScene: (id: number): Promise<void> => invoke("delete_scene", { id }),
  setActiveScene: (id: number): Promise<void> => invoke("set_active_scene", { id }),

  // ---- 设置 ----
  saveSettings: (patch: Partial<Settings>): Promise<Settings> =>
    invoke("save_settings", { patch }),
  getHotkeyDefaults: (): Promise<Hotkeys> => invoke("get_hotkey_defaults"),

  // ---- 窗口 ----
  showPanel: (cursorY?: number): Promise<void> => invoke("show_panel", { cursorY }),
  togglePanel: (): Promise<void> => invoke("toggle_panel"),
  hidePanel: (): Promise<void> => invoke("hide_panel"),
  setPanelMode: (mode: PanelMode): Promise<void> => invoke("set_panel_mode", { mode }),
  reportPresence: (window: string, inside: boolean): Promise<void> =>
    invoke("report_presence", { window, inside }),
  setBarHover: (hovering: boolean): Promise<void> => invoke("set_bar_hover", { hovering }),
  openSettings: (): Promise<void> => invoke("open_settings"),
  quitApp: (): Promise<void> => invoke("quit_app"),
  setPanelSize: (width: number, height: number): Promise<void> =>
    invoke("set_panel_size", { width, height }),

  // ---- 拖出（tauri-plugin-drag 底层命令） ----
  startDragOut: async (
    paths: string[],
    iconDataUrl: string,
    mode: "copy" | "move",
    onResult: (dropped: boolean) => void,
  ): Promise<void> => {
    const { Channel, invoke: rawInvoke } = await import("@tauri-apps/api/core");
    const channel = new Channel<{ result: { type?: string } | string }>();
    channel.onmessage = (msg) => {
      const r = msg?.result;
      const kind = typeof r === "string" ? r : (r?.type ?? "");
      onResult(kind === "Dropped" || kind === "dropped");
    };
    await rawInvoke("plugin:drag|start_drag", {
      item: paths,
      image: iconDataUrl,
      options: { mode },
      onEvent: channel,
    });
  },

  /** 剪切拖出完成后的源删除（OLE 移动契约：目标已接收，源负责删除） */
  finalizeDragCut: (paths: string[]): Promise<void> =>
    invoke("finalize_drag_cut", { paths }),
};

export type Ipc = typeof ipc;
