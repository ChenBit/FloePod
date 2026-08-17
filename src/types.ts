/** 与 Rust 侧 serde 对应的共享类型（camelCase 序列化） */

export type ItemKind = "file" | "folder" | "text" | "shortcut";
export type DropAction = "ask" | "copy" | "move" | "shortcut";
export type BarForm = "strip" | "bookmark";
export type Edge = "left" | "right";
export type Material = "acrylic" | "plain";
export type ThemeMode = "system" | "light" | "dark";
export type ExportMode = "copy" | "move";
export type ConflictStrategy = "ask" | "overwrite" | "skip" | "rename";

export interface StagedItem {
  id: number;
  sceneId: number;
  kind: ItemKind;
  /** 暂存文件夹内的绝对路径 */
  stagingPath: string;
  /** 原文件路径（copy/shortcut 时保留；text 为 null） */
  originalPath: string | null;
  name: string;
  ext: string | null;
  /** 字节数；folder 为 0 */
  size: number;
  createdAt: number;
}

export interface Scene {
  id: number;
  name: string;
  sort: number;
  createdAt: number;
}

export interface Hotkeys {
  toggleBar: string;
  collectClipboard: string;
  openPanel: string;
}

export interface Settings {
  stagingFolder: string | null;
  /** 拖入动作：ask = 每次询问 */
  dropAction: DropAction;
  barForm: BarForm;
  edge: Edge;
  /** 0.55 - 1，条体不透明度 */
  opacity: number;
  material: Material;
  /** 悬停展开延迟 ms */
  hoverDelayMs: number;
  /** 面板宽度 300 - 480 */
  panelWidth: number;
  theme: ThemeMode;
  autostart: boolean;
  activeSceneId: number;
  firstRunDone: boolean;
  hotkeys: Hotkeys;
  /** 只读信息：应用版本与数据目录 */
  version: string;
  dataDir: string;
}

export interface ModifierState {
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
}

export interface PendingDrop {
  /** 待处理的原始路径 */
  paths: string[];
}

export interface ExportConflict {
  /** 与目标位置冲突的名字 */
  names: string[];
}

export interface ThumbnailPayload {
  mime: string;
  /** 图像字节 */
  bytes: number[];
}

export interface Bootstrap {
  settings: Settings;
  scenes: Scene[];
  items: StagedItem[];
  panelMode: PanelMode;
  pendingDrop: PendingDrop | null;
  version: string;
}

export type PanelMode = "list" | "ask" | "conflict";
