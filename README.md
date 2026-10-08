# reader-mini

用 Rust + Slint 写的极简桌面阅读器，面向 Windows / Linux。

## 功能

**主窗口（常规窗口）**
- 书架页：查看所有图书，一键「阅读」或查看「目录」
- 点「阅读」会把**当前书籍 + 上次读到的那一章 / 那一行**一起交给阅读窗口；
  窗口已经开着就原地刷新到该位置，不会另开一个
- 设置页：配置 `baseUrl`、阅读窗口背景色 / 字体色 / 字号、全局快捷键；
  每个输入项都有边框，聚焦时边框变色
- 快捷键不是让用户敲文本：点一下输入框进入录制，直接按下组合键（如 `Ctrl+Alt+K`）即写入

**系统托盘**
- 启动后托盘常驻一个图标，图标由代码画（`src/tray.rs`），不额外带图片资源
- 关掉主窗口**不会**退出程序：窗口只是藏起来，从托盘菜单「打开书架」再叫回来
- 托盘菜单：打开书架 / 继续阅读（和全局快捷键同一个开关）/ 退出
- 点「退出」会先收掉阅读窗口（进度照常写回服务端）再退出
- 桌面环境没有托盘宿主（Linux 上少见，如缺 StatusNotifierItem）时建不出图标，
  程序会退回老行为：关掉主窗口即退出

**阅读窗口（浮动窗口）**
- 无边框、置顶、系统任务栏无图标，类似 ditto 的呼出方式
- 全局快捷键呼出 / 关闭，快捷键在「设置」页里按键录制（默认 `Ctrl+Alt+R`，保存即生效）
- 失去焦点自动关闭；`Esc` 也可关闭
- 窗口顶部有拖动条，按住即可任意拖动
- 可自由缩放：按住窗口的**边缘或四角**拖动即可拉伸（鼠标会变成对应的箭头），
  尺寸记进配置，下次打开还是这个大小
- **不分页**：整章正文一次铺满，**没有滚动条**
- **按窗口大小分页**：一行几个字由窗口宽度定、一屏几行由窗口高度定，
  打开一章就按当前窗口算出本章一共几页，页脚显示「第 x/y 页 · n%」
- 行高和整章行数都让 Slint 实测（自然行高 × 行高倍数，不是按字号估），
  所以每屏最后一行都是完整的一行，不会出现"半行字"
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

## 构建

需要 Rust（stable，1.85+）和 C 编译器，以及 Slint 用到的 `fontconfig` / `freetype` /
`xkbcommon` / `libx11`。开发镜像已经带了这些，本地装的话：

```bash
# Debian / Ubuntu
sudo apt-get install -y build-essential pkg-config cmake \
  libfontconfig1-dev libfreetype6-dev libxkbcommon-dev libxkbcommon-x11-dev \
  libx11-dev libxcb1-dev libgl1-mesa-dev libegl1-mesa-dev

cargo build --release
```

Windows 下用 MSVC 工具链直接 `cargo build --release` 即可。

### 依赖说明

- `slint` 1.18，开了 `raw-window-handle-06`（拖动 / 失焦检测要拿原生句柄）
  和 `system-tray`（托盘图标，Linux 走 ksni、Windows 走 `Shell_NotifyIconW`、
  macOS 走 `NSStatusItem`）
- `reqwest` 0.13 走 rustls，且**不开** `query` / `charset` feature：
  查询串自己百分号编码（见 `api::encode_query`），正文按 UTF-8 解码。
  rustls 的 CryptoProvider 用纯 Rust 的 `ring`，由 `net::client()` 装好，
  避免多引入一层 C 依赖。

## 运行与验证

```bash
cargo test          # 单测 + e2e（含折行映射、按行滚动、接口往返）
cargo run           # 启动应用
```

## 代码结构

```
ui/app.slint        主窗口
ui/reader_win.slint 阅读窗口
ui/tray.slint       系统托盘图标 + 菜单
src/api.rs          接口客户端 + 查询串编码
src/config.rs       配置读写（含全局快捷键）
src/progress.rs     阅读进度本地记录
src/hotkey.rs       全局快捷键解析与运行时换键
src/net.rs          HTTP 客户端装配（rustls provider）
src/platform.rs     平台层：拖动、失焦检测
src/tray.rs         系统托盘装配 + 图标绘制
src/reader_view.rs  阅读窗口逻辑（折行映射 + 按窗口分页 + 翻页 / 翻章）
src/main.rs         应用装配与主循环
```
