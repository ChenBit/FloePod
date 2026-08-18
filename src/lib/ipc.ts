import type {
  Bootstrap,
  ConflictStrategy,
  DropAction,
  ExportMode,
  Hotkeys,
  ModifierState,
  MonitorInfo,
  PanelMode,
  Pod,
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
  theme: "system",
  firstRunDone: false,
  autostart: false,
  hotkeys: { toggleBar: "Alt+Shift+F", collectClipboard: "Alt+Shift+S", openPanel: "Alt+Shift+P" },
  pods: [
    {
      id: 1,
      name: "我的匣",
      edge: "left",
      monitor: "",
      offset: 0.5,
      stagingFolder: "D:\\浮匣暂存（浏览器预览）",
      opacity: 0.85,
      material: "acrylic",
      panelWidth: 380,
      hoverDelayMs: 120,
      dropAction: "ask",
      enabled: true,
    },
  ],
  version: "0.5.1-mock",
  dataDir: "浏览器预览",
};
const mockMonitors: MonitorInfo[] = [
  { name: "\\\\.\\DISPLAY1", label: "主显示器", primary: true },
];

if (!inTauri) {
  mockItems = [
    {
      id: 1, podId: 1, kind: "file", stagingPath: "D:\\staging\\blueprint.webp",
      originalPath: "C:\\src\\blueprint.webp", name: "蓝图参考.webp", ext: "webp", size: 245760, createdAt: Date.now() - 6e5,
    },
    {
      id: 2, podId: 1, kind: "file", stagingPath: "D:\\staging\\需求说明.pdf",
      originalPath: null, name: "需求说明.pdf", ext: "pdf", size: 512000, createdAt: Date.now() - 18e5,
    },
    {
      id: 3, podId: 1, kind: "text", stagingPath: "D:\\staging\\会议要点.txt",
      originalPath: null, name: "会议要点.txt", ext: "txt", size: 2048, createdAt: Date.now() - 4e6,
    },
    {
      id: 4, podId: 1, kind: "folder", stagingPath: "D:\\staging\\素材包",
      originalPath: "C:\\src\\素材包", name: "素材包", ext: null, size: 0, createdAt: Date.now() - 2e6,
    },
    {
      id: 5, podId: 1, kind: "shortcut", stagingPath: "D:\\staging\\原型.fig - 快捷方式.lnk",
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
      return ret({ settings: mockSettings, monitors: mockMonitors, version: "0.5.1-mock" });
    case "get_pod":
      return ret(mockSettings.pods.find((p) => p.id === Number(args?.podId)) ?? null);
    case "get_monitors":
      return ret(mockMonitors);
    case "get_hotkey_defaults":
      return ret(mockSettings.hotkeys);
    case "list_pod_items":
      return ret(mockItems.filter((i) => i.podId === Number(args?.podId)));
    case "create_pod": {
      const pod: Pod = {
        id: mockSettings.pods.reduce((m, p) => Math.max(m, p.id), 0) + 1,
        name: "新匣",
        edge: "left",
        monitor: "",
        offset: 0.5,
        stagingFolder: "",
        opacity: 0.85,
        material: "acrylic",
        panelWidth: 380,
        hoverDelayMs: 120,
        dropAction: "ask",
        enabled: true,
        ...(args?.config as object),
      };
      mockSettings = { ...mockSettings, pods: [...mockSettings.pods, pod] };
      return ret(pod);
    }
    case "update_pod": {
      const id = Number(args?.podId);
      const patch = (args?.patch as object) ?? {};
      mockSettings = {
        ...mockSettings,
        pods: mockSettings.pods.map((p) => (p.id === id ? { ...p, ...patch } : p)),
      };
      return ret(mockSettings.pods.find((p) => p.id === id) ?? null);
    }
    case "delete_pod": {
      const id = Number(args?.podId);
      mockSettings = { ...mockSettings, pods: mockSettings.pods.filter((p) => p.id !== id) };
      return ret(undefined);
    }
    case "save_settings":
      mockSettings = { ...mockSettings, ...((args?.patch as object) ?? {}) };
      return ret(mockSettings);
    case "stage_text": {
      const content = String(args?.content ?? "");
      const requestedTitle = String(args?.title ?? "").trim().replace(/\.txt$/i, "");
      const baseName = requestedTitle || `文字 ${mockItems.length + 1}`;
      const item: import("@/types").StagedItem = {
        id: Date.now(), podId: Number(args?.podId) || 1, kind: "text",
        stagingPath: `D:\\staging\\${baseName}.txt`,
        originalPath: null, name: `${baseName}.txt`, ext: "txt",
        size: content.length, createdAt: Date.now(),
      };
      seed().push(item);
      return ret(item);
    }
    default:
      // 窗口类命令在浏览器里静默成功即可
      return ret(undefined);
  }
}

export const ipc = {
  inTauri,

  getBootstrap: (): Promise<Bootstrap> => invoke("get_bootstrap"),
  getPod: (podId: number): Promise<Pod | null> => invoke("get_pod", { podId }),
  getMonitors: (): Promise<MonitorInfo[]> => invoke("get_monitors"),

  // ---- 匣 CRUD ----
  createPod: (config: Partial<Pod>): Promise<Pod> =>
    invoke("create_pod", { config }),
  updatePod: (podId: number, patch: Partial<Pod>): Promise<Pod | null> =>
    invoke("update_pod", { podId, patch }),
  deletePod: (podId: number, recycleFiles: boolean): Promise<void> =>
    invoke("delete_pod", { podId, recycleFiles }),

  // ---- 拖入 ----
  getModifierState: (): Promise<ModifierState> => invoke("get_modifier_state"),
  /** 拖入待询问：暂存路径并弹出该匣面板 ask 模式 */
  holdPendingDrop: (podId: number, paths: string[]): Promise<void> =>
    invoke("hold_pending_drop", { podId, paths }),
  /** 确认动作后执行暂存 */
  stagePaths: (podId: number, paths: string[], action: DropAction): Promise<StagedItem[]> =>
    invoke("stage_paths", { podId, paths, action }),
  stageText: (podId: number, content: string, title?: string): Promise<StagedItem> =>
    invoke("stage_text", { podId, content, title: title ?? null }),

  // ---- 列表 ----
  listPodItems: (podId: number): Promise<StagedItem[]> =>
    invoke("list_pod_items", { podId }),
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

  // ---- 设置 ----
  saveSettings: (patch: Partial<Settings>): Promise<Settings> =>
    invoke("save_settings", { patch }),
  getHotkeyDefaults: (): Promise<Hotkeys> => invoke("get_hotkey_defaults"),

  // ---- 窗口（按匣） ----
  showPanel: (podId: number): Promise<void> => invoke("show_panel", { podId }),
  togglePanel: (podId: number): Promise<void> => invoke("toggle_panel", { podId }),
  hidePanel: (podId: number): Promise<void> => invoke("hide_panel", { podId }),
  setPanelMode: (podId: number, mode: PanelMode): Promise<void> =>
    invoke("set_panel_mode", { podId, mode }),
  reportPresence: (podId: number, window: string, inside: boolean): Promise<void> =>
    invoke("report_presence", { podId, window, inside }),
  setPanelPinned: (podId: number, pinned: boolean): Promise<void> =>
    invoke("set_panel_pinned", { podId, pinned }),
  setDraggingOut: (podId: number, dragging: boolean): Promise<void> =>
    invoke("set_dragging_out", { podId, dragging }),
  setPodAccept: (podId: number, accepting: boolean): Promise<void> =>
    invoke("set_pod_accept", { podId, accepting }),
  toggleAllBars: (): Promise<void> => invoke("toggle_all_bars"),
  openSettings: (): Promise<void> => invoke("open_settings"),
  logFrontend: (msg: string): Promise<void> => invoke("log_frontend", { msg }),
  appLog: (msg: string): Promise<void> => invoke("app_log", { msg }),
  quitApp: (): Promise<void> => invoke("quit_app"),
  setPanelSize: (podId: number, width: number, height: number): Promise<void> =>
    invoke("set_panel_size", { podId, width, height }),

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
