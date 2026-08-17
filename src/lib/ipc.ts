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

/** 浏览器开发模式的轻量 mock（无 Tauri 时保证 UI 可预览） */
let mockItems: import("@/types").StagedItem[] = [];
let mockSettings: import("@/types").Settings = {
  stagingFolder: "D:\\浮匣暂存（浏览器预览）",
  dropAction: "ask",
  barForm: "strip",
  edge: "left",
  opacity: 0.85,
  material: "acrylic",
  hoverDelayMs: 120,
  panelWidth: 380,
  theme: "system",
  autostart: false,
  activeSceneId: 1,
  firstRunDone: true,
  hotkeys: { toggleBar: "Alt+Shift+F", collectClipboard: "Alt+Shift+S", openPanel: "Alt+Shift+P" },
  version: "0.2.0-mock",
  dataDir: "浏览器预览",
};
let mockScenes: import("@/types").Scene[] = [
  { id: 1, name: "默认", sort: 0, createdAt: 1 },
  { id: 2, name: "工作素材", sort: 1, createdAt: 2 },
];
if (!inTauri) {
  mockItems = [
    {
      id: 1, sceneId: 1, kind: "file", stagingPath: "D:\\staging\\blueprint.webp",
      originalPath: "C:\\src\\blueprint.webp", name: "蓝图参考.webp", ext: "webp", size: 245760, createdAt: Date.now() - 6e5,
    },
    {
      id: 2, sceneId: 1, kind: "file", stagingPath: "D:\\staging\\需求说明.pdf",
      originalPath: null, name: "需求说明.pdf", ext: "pdf", size: 512000, createdAt: Date.now() - 18e5,
    },
    {
      id: 3, sceneId: 1, kind: "text", stagingPath: "D:\\staging\\会议要点.txt",
      originalPath: null, name: "会议要点.txt", ext: "txt", size: 2048, createdAt: Date.now() - 4e6,
    },
    {
      id: 4, sceneId: 2, kind: "folder", stagingPath: "D:\\staging\\素材包",
      originalPath: "C:\\src\\素材包", name: "素材包", ext: null, size: 0, createdAt: Date.now() - 2e6,
    },
    {
      id: 5, sceneId: 1, kind: "shortcut", stagingPath: "D:\\staging\\原型.fig - 快捷方式.lnk",
      originalPath: "C:\\src\\原型.fig", name: "原型.fig - 快捷方式.lnk", ext: "lnk", size: 0, createdAt: Date.now() - 3e5,
    },
  ];
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return mockInvoke<T>(cmd, args);
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const seed = () => (mockItems = [...mockItems]);
  const ret = (v: unknown) => v as T;
  switch (cmd) {
    case "get_bootstrap":
      return ret({
        settings: mockSettings,
        scenes: mockScenes,
        items: mockItems,
        panelMode: "list",
        pendingDrop: null,
        version: "0.2.0-mock",
      });
    case "list_items":
      return ret(mockItems);
    case "list_scenes":
      return ret(mockScenes);
    case "save_settings":
      mockSettings = { ...mockSettings, ...((args?.patch as object) ?? {}) };
      return ret(mockSettings);
    case "stage_text": {
      const content = String(args?.content ?? "");
      const item: import("@/types").StagedItem = {
        id: Date.now(), sceneId: mockSettings.activeSceneId, kind: "text",
        stagingPath: `D:\\staging\\文字 ${mockItems.length + 1}.txt`,
        originalPath: null, name: `文字 ${mockItems.length + 1}.txt`, ext: "txt",
        size: content.length, createdAt: Date.now(),
      };
      seed().push(item);
      return ret(item);
    }
    case "create_scene":
      return ret({ id: Date.now(), name: String(args?.name), sort: mockScenes.length, createdAt: Date.now() });
    case "get_hotkey_defaults":
      return ret(mockSettings.hotkeys);
    default:
      // 窗口类命令在浏览器里静默成功即可
      return ret(undefined);
  }
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
