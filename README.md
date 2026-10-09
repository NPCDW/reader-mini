# reader-mini

用 tauri Rust + vue 写的极简桌面阅读器，面向 Windows / Linux。

## 功能

**主窗口（常规窗口）**
- 书架页：查看所有图书，一键「阅读」或查看「目录」
- 点「阅读」会把**当前书籍 + 上次读到的那一章 / 那一行**一起交给阅读窗口；
  窗口已经开着就原地刷新到该位置，不会另开一个
- 设置页：配置 `baseUrl`、阅读窗口背景色 / 字体色 / 字号 / 行高倍数、全局快捷键、
  阅读窗口失焦是否自动关闭；每个输入项都有边框，聚焦时边框变色。
  保存后立刻作用到已开着的阅读窗口，不用重开窗口。
  默认底色 `#181818`、字色 `#bdbdbd`、字号 20、行高倍数 1.5、失焦关闭关
- 快捷键不是让用户敲文本：点一下输入框进入录制，直接按下组合键（如 `Ctrl+Alt+K`）即写入
- 关掉阅读窗口前会先把进度写回服务端，本地也记下读到第几行

**系统托盘**
- 启动后托盘常驻一个图标，图标由代码画（`src-tauri/src/tray.rs`），不额外带图片资源
- 关掉主窗口**不会**退出程序：窗口只是藏起来，从托盘菜单「打开书架」再叫回来
- 托盘菜单：打开书架 / 继续阅读·收起（和全局快捷键同一个开关）/ 退出
- 点「退出」会先收掉阅读窗口（进度照常写回服务端）再退出
- 桌面环境没有托盘宿主（Linux 上少见，如缺 StatusNotifierItem）时建不出图标，
  程序会退回老行为：关掉主窗口即退出
- Linux 上如果装的是 ayatana 版实现，启动时可能看到一行
  `libayatana-appindicator-WARNING **: ... is deprecated. Please use
  libayatana-appindicator-glib`。这是上游库自己打的：托盘由 Tauri 的
  `tray-icon` 走 `dlopen` 建出来，新一点的发行版把
  `libayatana-appindicator3.so.1` 指向了 `-glib` 实现，而那一版里保留了一个
  兼容用的弃用告警。功能不受影响，去掉它需要换成 KSNI（D-Bus）托盘后端，
  会牵动 `tray-icon` 与 `tauri` 的 feature，暂未采纳

**阅读窗口（浮动窗口）**
- 无边框、置顶、系统任务栏无图标，类似 ditto 的呼出方式
- 全局快捷键呼出 / 关闭，是同一个开关：窗口没开就按出来，开着就再按一下收起来
  （默认 `Alt+PgDn`，快捷键在「设置」页里按键录制，保存即生效）。
  收起之后不会自己弹回来：全局快捷键的那一下只会在「真的开着的窗口」上当成收起，
  窗口管理器在隐藏窗口后补发的失焦也不会被当成读者的关闭动作
- `Esc` / 右上角 ✕ / 同一个全局快捷键收起；**失去焦点自动关闭是配置项，默认关**
  （设置页里勾上才生效 —— 开着它点一下别处窗口就没了，太容易误伤）
- 窗口顶部有拖动条，按住即可任意拖动
- 可自由缩放：按住窗口的**边缘或四角**拖动即可拉伸（鼠标会变成对应的箭头），
  尺寸记进配置，下次打开还是这个大小
- **不分页**：整章正文一次铺满，**没有滚动条**
- **按窗口大小分页**：一行几个字由窗口宽度定、一屏几行由窗口高度定，
  打开一章就按当前窗口算出本章一共几页，页脚显示「第 x/y 页 · n%」
- 行位置让 webview 逐段实测（自然行高 × 行高倍数，不是按字号估）；
  一屏只装「整行底边还在可视区里」的那些行 —— 放不下的整行留给下一页，
  屏幕下沿不会切出半行字。每屏首行都贴着可视区上沿，
  下一页的第一行就是上一页末行的下一行，**页与页首尾相接、不重复、不漏行**；
  正文是「第 1 页 → 第 2 页 → …」，没有「上一页看一眼下一屏」这回事
- `PgUp` / `PgDn` 一屏一屏翻，相邻两屏首尾相接：不重复上一屏的行，也不跳字
- 本章最后一页再按 `PgDn` 会自动翻到下一章；第一页再按 `PgUp` 回到上一章末尾

## 接口

详细请查看 [README-api.md](./README-api.md)

| 接口 | 方法 | 说明 |
| --- | --- | --- |
| `/getBookshelf` | GET | 获取所有图书 |
| `/getChapterList?url=<bookUrl>` | GET | 获取一本图书的所有章节 |
| `/getBookContent?url=<bookUrl>&index=<n>` | GET | 获取正文 |
| `/saveBookProgress` | POST | 保存阅读进度 |

阅读进度除了同步到服务端，也会在本地记录每本书读到哪一行，下次直接续读；
从书架点「阅读」时按这个记录定位，改字号 / 重开阅读页也不会跳回开头。
进度以 `bookUrl` 为键 —— 服务端可能按阅读时间重排书架，用下标会串书。

## 目录结构

```
src/                  Vue 前端
  App.vue             主窗口：书架页 / 目录页 / 设置页
  ReaderApp.vue       阅读窗口：无边框浮动窗口，按窗口分页
  paging.js           分页算法（纯函数，有单测）
  keys.js             按键录制：KeyboardEvent -> "Ctrl+Alt+R"
  bridge.js           Tauri 命令与事件的前端封装
src-tauri/            Rust 后端
  src/lib.rs          窗口编排：书架主窗口 + 阅读窗口
  src/api.rs          四个接口 + 信封拆解
  src/config.rs       配置读写
  src/progress.rs     本地阅读进度
  src/hotkey.rs       快捷键写法 <-> Tauri 加速键
  src/tray.rs         系统托盘
```

两个窗口各是一个 Vite 入口：`index.html`（主窗口）与 `reader.html`（阅读窗口）。

## 开发

```bash
pnpm i
pnpm tauri dev          # 起开发环境（vite + tauri）
pnpm tauri build        # 出安装包
```

门禁：

```bash
pnpm test                              # 前端分页单测
pnpm build                             # 前端构建
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

配置与进度落在系统配置目录：Linux 为 `~/.config/reader-mini/`，
Windows 为 `%APPDATA%\readermini\reader-mini\config\`。
