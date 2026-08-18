/** Tauri 事件名常量与监听封装 */

export const Events = {
  ItemsChanged: "floepod://items-changed",
  SettingsChanged: "floepod://settings-changed",
  PodsChanged: "floepod://pods-changed",
  PanelMode: "floepod://panel-mode",
  PanelShown: "floepod://panel-shown",
  PanelPinned: "floepod://panel-pinned",
  PanelHideRequest: "floepod://panel-hide-request",
  CollectClipboard: "floepod://collect-clipboard",
  OpenPanel: "floepod://open-panel",
} as const;

export async function listen<T>(event: string, handler: (payload: T) => void): Promise<() => void> {
  if (!("__TAURI_INTERNALS__" in window)) return () => undefined;
  const { listen: rawListen } = await import("@tauri-apps/api/event");
  const unlisten = await rawListen<T>(event, (e) => handler(e.payload));
  return unlisten;
}
