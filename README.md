# 浮匣 FloePod

本地优先的 Windows 屏幕边缘文件暂存工具。把文件、图片、文字拖到屏幕边缘的浮匣里集中保管，需要时再拖出去或批量导出。不联网、无 AI、无遥测，所有数据只存在你自己的电脑上。

## 功能

- **浮动条 / 浮动书签** 两种常驻形态（设置中切换），半透明贴在屏幕左/右边缘
- **拖入即暂存**：文件、文件夹、图片，落地动作可选 复制 / 移动 / 创建快捷方式（每次询问或固定，修饰键 Ctrl/Shift/Alt 可临时直达）
- **文字暂存**：剪贴板热键收集、面板手动输入（自动存为 .txt，统一按文件管理）
- **暂存面板**：悬停或单击浮匣弹出（不抢焦点）；缩略图、多选、批量复制/移动导出、拖出（复制或剪切）、进回收站移除
- **场景**：同一暂存文件夹下的多套分组（工作素材 / 个人文件），面板与托盘快速切换
- **系统集成**：托盘菜单、全局快捷键、开机自启（可选）
- **便携优先**：exe 旁 `FloePodData/` 可写则数据跟随程序走，否则回退 `%APPDATA%\FloePod`

## 技术栈

- 前端：Vue 3 + TypeScript + Vite + Tailwind CSS v4 + Pinia
- 后端：Rust + Tauri 2（多窗口、原生拖放、托盘、全局快捷键）
- 存储：SQLite（rusqlite bundled，含 WAL）——暂存条目元数据 + 场景 + 设置
- 拖出：`tauri-plugin-drag`（Windows OLE 拖放，剪切模式遵循移动契约回删源文件）

## 开发

```bash
pnpm install          # 安装前端依赖
pnpm tauri dev        # 开发运行（Rust + Vite 联动）
cargo test            # Rust 单测（在 src-tauri/ 下）
pnpm build            # 前端类型检查 + 产物构建
pnpm tauri build      # 发布构建（NSIS 安装包 + 裸 exe）
node scripts/make-placeholder-icon.mjs   # 重新生成占位图标源图
pnpm tauri icon app-icon.png             # 由源图生成全套图标
```

### 结构

```
src/                    # Vue 前端（按窗口 label 分发视图，无需路由）
  windows/              # BarWindow / PanelWindow / SettingsWindow
  components/           # 文件图形、缩略图、场景切换、询问/冲突选择器…
  stores/               # pinia：settings / staging
  lib/                  # ipc 封装、事件常量、弹簧动画、格式化
src-tauri/src/
  lib.rs                # 应用装配（插件/托盘/首启引导）
  manager.rs            # 窗口编排：几何、面板不抢焦点显隐、在场看门狗
  commands.rs           # 全部 Tauri 命令（暂存/导出/场景/设置）
  db.rs settings.rs     # SQLite 与设置持久化
  watcher.rs            # 暂存文件夹监听对账（notify）
  win.rs lnk.rs         # Win32 辅助；.lnk 快捷方式
  tray.rs hotkeys.rs    # 托盘与全局快捷键
```

### 便携版打包

```bash
pnpm tauri build
node scripts/package-portable.mjs
# -> dist/FloePod-<版本>-win-x64-portable.zip
```

## 说明

- 需要 WebView2 运行时（Windows 10/11 一般自带）
- 图标当前为占位版本，与界面品牌图形同源；拿到正式图标后执行
  `pnpm tauri icon <图标.png> && pnpm tauri build` 即可替换
