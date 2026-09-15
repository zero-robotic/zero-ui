# zero-ui 框架与开发计划

> 基于产品目标整理：阶段一做跨平台桌面应用 toolkit；过渡期做 shell 组件；阶段二聚焦 **FreeBSD** 桌面。通过可替换后端与稳定公开 API，**尽量保持应用层不变、降低向 shell/会话迁移的成本**（不承诺零改动）。
>
> 当前阶段的整改顺序、任务清单与退出条件见 [current-stage-tasks.md](current-stage-tasks.md)。

## 一、产品定位

- **短期**：Rust 自绘 UI 框架，能在 Linux / macOS / Windows 上写桌面应用，并尽早在 FreeBSD 上可编译、随后可运行
- **中期**：同一套控件能做面板、启动器、通知等 shell 级应用（先在现有会话里验证）
- **长期**：在 **FreeBSD** 上提供会话 +（可选）合成器，形成可用桌面

**差异化定位**：应用与桌面 shell 共用同一 toolkit；平台能力全走抽象；**FreeBSD 为桌面阶段的第一公民目标平台**。

### BSD 家族范围（明确收窄）

| 系统 | 优先级 | 承诺 |
|------|--------|------|
| **FreeBSD** | P0 | 阶段一 CI 至少 `check`（力争 `build`）；阶段二会话/桌面的唯一正式目标 |
| OpenBSD | P2 | 无里程碑承诺；图形/输入栈差异大，仅作调研与社区移植欢迎 |
| NetBSD | P3 | 同上，优先级低于 OpenBSD |

正文若写「BSD」，除非另有说明，均指 **FreeBSD**。不承诺「一套代码覆盖全部 BSD」。

### 术语表

| 术语 | 含义 |
|------|------|
| **toolkit** | 本仓库交付的应用 UI 框架（控件、布局、主题、与 platform/render 的编排），用于编写普通桌面应用 |
| **shell** | 桌面外壳组件：面板、启动器、托盘/通知等；仍是「应用」，但宿主多为 layer-shell 等，而非普通窗口 |
| **session / FreeBSD session** | 用户登录之后、由会话管理拉起的一组进程与环境（面板、设置守护、自启动等）；阶段二目标，不等于整个操作系统 |
| **BSD**（本文） | 除非另有说明，均指 **FreeBSD**；不表示 OpenBSD/NetBSD 已纳入同一验收范围 |
| **compositor** | 显示服务器侧合成器（如 Wayland compositor），负责把各客户端表面合成到屏幕；可自研（`zero-comp`）或复用现有实现 |
| **Host** | `zui-platform` 抽象：可接收输入与生命周期事件的宿主；普通 `Window` 是 Host 的一种，面板宿主也是 |
| **Surface**（platform） | 非窗口或通用呈现表面的 platform 侧抽象（experimental）；**不要**与下项混淆 |
| **`wgpu::Surface`** | `zui-render` 内部的 GPU 交换链对象，由 raw window handle 创建；应用与 widgets 不直接接触 |
| **dip** | device-independent pixel，逻辑像素；布局与控件尺寸使用 dip，再按 scale factor 映射到物理像素 |
| **backend** | 实现 platform 公开 API（经 SPI）的平台适配 crate，如 `zui-backend-winit`；由组装根编译期注入 |
| **SPI** | Service Provider Interface：仅供 backend 使用的钩子/类型；应用与 `zero-ui` 不得依赖 |
| **capability** | 可选平台能力（剪贴板、托盘、LayerShell 等），按需实现，不同于 `core` 必选面 |
| **组装根** | example 或二进制的 `main` / `zero-ui-app` 入口：选择 backend、创建 `Renderer`、启动 `Application` |
| **MSRV** | Minimum Supported Rust Version，仓库保证可编译的最低 Rust 版本；从 M0 起写明并在 CI 验证 |

---

## 二、总体架构

### 依赖方向（Cargo / 逻辑依赖）

箭头表示「依赖于」（A → B 表示 A 的 `Cargo.toml` 依赖 B）。**勿与运行时调用栈混淆。**

```text
  examples/* / 最终二进制（组装根 composition root）
       │
       │  选择并注入具体 backend + 创建 Renderer
       ▼
  ┌────────────┐     ┌──────────────┐     ┌─────────────┐
  │ zero-ui-app│────▶│   zero-ui    │────▶│  zui-core  │
  └─────┬──────┘     └──────┬───────┘     └─────────────┘
        │                   │
        │                   │ 仅依赖绘制抽象（如 DisplayList /
        │                   │ Paint 接口），不直接握 wgpu Device
        │                   ▼
        │            ┌──────────────┐
        ├───────────▶│ zui-render  │────▶ zui-core
        │            └──────────────┘
        │
        ├───────────▶│ zui-platform │────▶ zui-core
        │            │  （公开 API）  │
        │            └───────▲───────┘
        │                    │ implements（backend 实现公开 API；
        │                    │ 可依赖 spi，见下文）
        ▼                    │
  zero-backend-* ────────────┘
  (winit / headless / layershell / …)
```

**组装根职责（通常在 `zero-ui-app` 或 example 的 `main`）：**

1. 按 **Cargo feature** 编译进所选 `zero-backend-*`  
2. 构造实现了 `zui-platform` 公开 API 的 backend 实例  
3. 构造 `zui-render::Renderer`（拥有 Device）  
4. 将二者交给 `zero-ui-app::Application`，再挂载 `zero-ui` 控件树  

应用业务与 `zero-ui` **不**自己 `new` 某个 OS backend，也 **不**依赖 backend crate。

### 逻辑分层（谁调用谁）

```text
  应用业务 / zero-shell
        │ 使用控件与 Application API
        ▼
  zero-ui-app  ──编排──▶  zero-ui.paint()  →  zui-render.render_frame()
        │                      ▲
        │ 事件/窗口/capability │
        ▼                      │
  zui-platform 公开 API ◀── backend 实现
```

阶段二新增（不改 widgets 稳定 API；目录见第八节 planned）：

- `zero-shell` → 面板/启动器等应用侧原语（M3）  
- `zero-session` → 登录后拉起与守护（M4）  
- `zero-services-*` → 音频/网络/电源等最小权限服务（M4）  
- `zero-comp` → compositor（可选、后置）  

### UI 控件通信与状态流

控件之间不直接互相持有或调用。采用「事件/命令输入、状态更新、重新渲染」的单向数据流，降低控件耦合，并保持页面在桌面应用与 shell 场景之间的可复用性：

```text
控件交互
   │
   ▼
局部事件 / Action / Command
   │
   ▼
父组件或 Application 更新 PageState / AppState
   │
   ▼
控件根据新状态重新布局与绘制
```

通信方式按范围划分：

| 范围 | 推荐方式 | 说明 |
|------|----------|------|
| 控件局部交互 | 父子回调 / 局部事件 | 子控件只报告事件，由父组件协调其他子控件 |
| 页面内部通信 | 共享 `PageState` + 单向数据流 | 一个控件修改状态，相关控件根据状态更新 |
| 跨页面或应用级行为 | `Action` / `Command` | 例如打开设置、保存配置、切换主题、启动应用 |
| 平台与后台事件 | 受控事件总线或 channel | 统一投递到 UI 线程，不让控件直接访问 backend |
| 高频或派生 UI 更新 | 可观察状态 / signal | 作为受控的响应式机制，避免隐式依赖与循环触发 |

控件 API 可以提供事件注册或事件产生能力，但不得直接依赖另一个具体控件。例如，按钮产生 `AppCommand::OpenSettings`，由页面或应用处理，而不是直接调用某个设置面板的方法。事件总线仅用于跨模块或平台级事件，不作为普通控件通信的全局调用中心。

该约束适用于 `zero-ui`、`zero-ui-app` 和 example 业务逻辑；具体事件类型、状态更新 API 与响应式机制可在 M1 先以 `experimental` 形式实现，待焦点、命中测试和基础控件交互稳定后再固化。

### 公开 API vs Backend SPI

「平台能力只有一条应用可见入口」，**不等于**整个仓库只有一个 trait。

| 类别 | 所在位置 | 谁可以依赖 | 例子 |
|------|----------|------------|------|
| **Platform 公开 API** | `zui-platform` 根模块 / `api` | 应用、`zero-ui`、`zero-ui-app` | `AppLoop`、`Window`、`InputEvent`、`Clipboard`、`Paths`、`LayerShell`（capability） |
| **Render 公开 API** | `zui-render` | `zero-ui-app`；`zero-ui` 仅限绘制抽象 | `Renderer`、`attach_surface`、`DisplayList` / `Paint` |
| **Backend SPI** | `zui-platform::spi`（或日后 `zui-platform-spi`） | **仅** `zui-backend-*` | 将 raw handle、平台事件泵进公开 API 的钩子；应用禁止依赖 |
| **Backend 实现** | `zero-backend-*` | 仅组装根通过 feature 引用 | `WinitBackend`、`HeadlessBackend`、`LayerShellBackend` |

说明：

- `Renderer` **不属于** `zui-platform`，属于 `zui-render`。  
- 旧文中的 `WindowBackend` 若保留，应落在 **SPI / backend**，不暴露给应用。  
- `Clipboard` 等是 **platform 公开 capability trait**；其 OS 实现在 backend 内，经组装注入。

### `zui-platform` 与 `zui-render` 协作（所有权）

| 职责 | 归属 | 说明 |
|------|------|------|
| 窗口 / Host / Surface 创建与销毁 | platform 公开 API + backend 实现 | 事件循环、resize、scale、关闭 |
| Raw window / display handle | backend → 经 SPI/借出接口给 render | 供建交换链 |
| `wgpu::Instance` / `Adapter` / `Device` / `Queue` | **`zui-render`** | 进程内可共享 |
| `wgpu::Surface` 与 reconfigure | **`zui-render`** | platform 发 `Resized` → app 调 `render.resize` |
| 每帧 present | **`zui-render`** | UI 只提交绘制列表 |
| 输入、IME、剪贴板、路径、托盘等 | **`zui-platform` 公开 API** | render 不感知 |

**推荐调用顺序（每窗）：**

1. backend 创建 Host/Window，进入事件循环  
2. 首次 `Redraw` 前：`render.attach_surface(window_handle, size, scale)`  
3. `Resized` / `ScaleFactorChanged`：`render.resize(...)`  
4. 每帧：`ui.paint()` → `render.render_frame()`  
5. 窗口关闭：先 `render.detach_surface()`，再销毁 window  

### Crate 划分（monorepo）

| Crate | 职责 | 阶段 |
|-------|------|------|
| `zui-core` | 颜色、几何、错误、ID、dip/物理像素、时间 | 1 |
| `zui-render` | GPU/软件绘制、字形、图片；拥有 device 与 per-window surface | 1 |
| `zui-platform` | 公开 API（core + capability）+ `spi`（仅 backend） | 1 |
| `zui-backend-winit` | 普通应用窗：Linux/macOS/Windows/FreeBSD | 1 |
| `zui-backend-headless` | 测试 / CI | 1 |
| `zero-backend-layershell` | Wayland layer-shell / X11 dock 类面板宿主 | M3 |
| `zero-backend-hotkey` | 全局快捷键适配 | M3 |
| `zero-backend-tray` | 托盘 / StatusNotifier 等适配 | M3 |
| `zero-ui` | 控件、布局、主题、焦点、无障碍树 | 1 |
| `zero-ui-app` | `Application`、编排 platform + render + ui | 1 |
| `zero-shell` | 面板/启动器等 shell 原语 | M3 |
| `zero-backend-freebsd` | FreeBSD 会话向增强 | M4 planned |
| `zero-session` / `zero-services-*` / `zero-comp` | 桌面系统件 | M4 planned |

**关于 winit：** 仅作为 **普通应用窗** 的第一后端。面板、全局快捷键、托盘等走独立 backend，经 platform **公开** capability 暴露。

**后端切换承诺：** 默认仅 **编译期**（Cargo feature + 组装根代码）切换 backend。不承诺运行时 `dlopen` 插件、稳定插件 ABI 或动态库分发；若将来需要，单独立项。

---

## 三、平台契约：核心稳定 + capability 演进

不要求所有接口「签名一次设计好」。高复杂度面用 **成熟度分级**。

### 成熟度

| 级别 | 含义 | 破坏性变更 |
|------|------|------------|
| `core` | 应用窗闭环必需，评审后尽量稳定 | 需 RFC + 主版本或明确迁移期 |
| `capability` | 按需实现；**编译期** feature 或类型级可选 | 可加方法；删改需版本说明 |
| `experimental` | 允许快速迭代 | 可随时改；不承诺跨小版本兼容 |

### 阶段一必需 vs 未来扩展

| 能力 | 阶段归属 | 落点 | 说明 |
|------|----------|------|------|
| App loop / Window / 基础 Input / Paths / Output·scale | **阶段一必需** | platform `core` | M0 起 |
| 多窗 | 阶段一（M2） | platform `core` | 由 `AppContext` 受控创建、查询和销毁 |
| Clipboard / Dialog | 阶段一（M2） | platform `capability` | |
| IME | 阶段一（M1.5） | platform `experimental`→`capability` | |
| 控件内主题 token（色/字号/间距） | **阶段一必需** | **`zero-ui`**，非 OS 服务 | 不进 platform 契约 |
| 系统主题/外观偏好订阅 | 未来扩展 | platform `capability`（可选） | 有明确需求再加 |
| Notify / Tray / GlobalHotkey / LayerShell | M3 | platform `capability` | 独立 backend |
| Accessibility 平台桥 | 阶段一可浅做 | `experimental` | 控件树节点阶段一就要有 |
| 电源 / 网络 / 音量等 | **M4** | `zero-services-*` + IPC | **不**进阶段一 platform 契约 |
| Session / DM / Compositor | **M4** | `zero-session` / `zero-comp` | 同上 |

### 能力清单与分级（platform 公开 API）

| 能力 | 初定级别 | 说明 |
|------|----------|------|
| App loop / 退出 / 帧请求 | `core` | |
| Window 级 Host（建窗、resize、scale） | `core` | |
| 基础 Input（指针、键盘、滚轮） | `core` | |
| Paths（配置/数据/缓存/运行时） | `core` | |
| Output 列表与 scale | `core`（色域可后补） | |
| `Surface` / 非窗口 Host 抽象 | `experimental` → 视 M3 再升 | |
| Clipboard / DragDrop | `capability` | |
| Dialog（文件选择等） | `capability` | |
| IME | `experimental`（M1.5）→ 成熟后 `capability` | |
| Notify / Tray | `capability`（M3） | |
| GlobalHotkey | `capability`（M3） | |
| LayerShell / PanelHost | `capability`（M3） | |
| Accessibility 语义树提交 | `experimental` | |
| 后台任务投递到 UI 线程 | `experimental` | |
| 系统电源/网络/音量 | — | **不在此表**；见 M4 services |

**硬规则：**

- 应用、`zero-ui`、example **业务逻辑**禁止直接依赖 `winit` / 各 OS crate / 协议 crate / `zui-platform::spi`  
- **`#[cfg(target_os)]` / 平台 feature：** 不得出现在 `zero-ui`、应用业务与 example 主逻辑中；允许出现在 `zero-backend-*`、workspace/`Cargo.toml` 接线、组装根的薄胶水、`build.rs`、以及为链接/资源选择所必需的最小条件编译  
- 布局与绘制只用 **逻辑像素（dip）**  
- 新增公开能力：先改 `zui-platform`（标好级别），再写 backend  

---

## 四、技术选型（阶段一默认）

| 层 | 建议 | 说明 |
|----|------|------|
| 语言 | Rust 2021+，edition 统一 | workspace |
| 普通窗口 | `winit` 作第一后端 | 不覆盖 shell 专用协议 |
| Shell 宿主 | 独立 layershell / X11 dock 适配 | M3；经 platform capability |
| 渲染 | `wgpu` + 自绘 | device 在 `zui-render`；可加 soft 后备 |
| 文本 | `cosmic-text` 或等价 | 复杂文本/CJK 必做 |
| 布局 | 自研简化 flex/stack | 勿过早上完整 CSS |
| 异步 | UI 线程同步；重活用通道回 UI | 勿把整个 UI 绑死某 runtime |
| 测试 | headless + 布局/事件单测；黄金图后补 | M0a 起 headless |
| FreeBSD CI | M0b：**至少 `cargo check`**；有条件再 `build`；运行开窗不作为 M0b 必达 | 「能编译」与「可用」分开 |
| Backend 切换 | 编译期 feature | 无运行时插件 ABI 承诺 |

**阶段一明确不做：** WebView 核心、原生控件核心、自研合成器、backend 热加载。

---

## 五、从阶段一到 FreeBSD 桌面的过渡原则

### 分层契约

```text
┌─────────────────────────────────────┐
│  App / Widgets / 控件内 Theme token │  ← 阶段一交付
├─────────────────────────────────────┤
│  Platform 公开 API（core+capability）│  ← 阶段一要有；实现可薄
│  窗口 · 输入 · 剪贴板 ·（M3）托盘等  │
├─────────────────────────────────────┤
│  Backend 实现 + SPI                 │  ← 编译期注入
│  阶段一: winit + headless            │
│  M3: + layershell + hotkey + tray    │
│  M4: + freebsd 等（planned）         │
├─────────────────────────────────────┤
│  Session / Services（M4，最小权限）   │  ← 非「特权 shell」
└─────────────────────────────────────┘
```

### 阶段一就要为桌面埋的桩

| 能力 | 阶段一怎么做 | 阶段二为什么需要 |
|------|-------------|------------------|
| **后端可插拔** | 公开 API + SPI + 多 backend（编译期） | 不能绑死 winit |
| **逻辑像素 + 缩放** | 统一 dip | HiDPI / 多显示器 |
| **输入链路可扩展** | 统一 `InputEvent`；IME experimental | 会话级输入 |
| **无障碍树** | 控件侧语义节点 | 桌面无障碍 |
| **主题 token** | `zero-ui` 内 token | 与系统主题服务解耦 |
| **异步边界** | UI 线程 vs 后台 | 接 IPC 服务 |
| **headless** | 测试后端 | CI / 无显示环境 |

### 阶段一主动避开的坑

1. 把 macOS/Windows 原生控件当核心  
2. 业务直接依赖 `winit` / `arboard` / 通知 crate  
3. 用 WebView 当唯一 UI 架构中心  
4. 把 Linux 专用假设（systemd、D-Bus 形状、硬编码路径）写进核心  
5. 把「窗口」当成唯一宿主（`Host`/`Surface` experimental 演进）  
6. 假设 winit 提供 layer-shell / 全局快捷键 / 托盘  
7. 把 shell 做成默认高权限「特权进程」  
8. 把电源/网络等系统服务塞进阶段一 platform 契约  

### 过渡节奏

1. **阶段 1A**：跨平台 App toolkit；FreeBSD 先 `check`/`build`、再运行  
2. **阶段 1B（M3）**：独立协议 backend + 在现有会话跑**受限**面板/启动器  
3. **阶段 2（M4）**：最小权限 services + session（接现有 compositor）；`zero-comp` 可选后置  

### 迁移成本可检验标准（尽量保持应用层不变）

- 同一个 `Button` / 布局代码，在 Win App 和 FreeBSD 面板里都能用  
- 换/加 backend：**只改 Cargo feature 与组装根**，不改应用业务与 `zero-ui`（不承诺运行时热插拔，也不承诺绝对零改动）  
- Shell 是 **受限客户端**（仅申请所需 capability/协议）+ **最小权限服务** + **明确 IPC**；不是另一套 UI 框架，也不是默认 root/轮询全系统的特权进程  
- 阶段二新增系统 crate，而不是 `widgets-v2`  

---

## 六、开发计划（里程碑）

原则：**「跨平台能编译」与「跨平台可用」分开验收**；IME、shell 协议等从主路径拆出。时间盒为单人全职粗估，兼职按 2× 计。

**版本与变更记录（从 M0 起，不等到 API「稳定」或 M2）：**

- 维护根目录 `CHANGELOG.md`，用户可见变更进入主线时更新  
- 对外 crate 使用 **0.x 语义化版本**（`0.y` 允许破坏性变更，须在 changelog 标明）  
- 文档与 CI 固定 **MSRV**；提升 MSRV 必须记入 changelog  

### M0a — 单平台地基（约 2–3 周）

**目标**：在主开发机（建议 Linux 或 macOS）上空窗 + wgpu 清屏 + 事件循环。

- [ ] monorepo workspace；依赖方向与第二节一致  
- [ ] `zui-core` + `zui-platform` **core** 公开 API + 最小 `spi`  
- [ ] 文档化 platform↔render 所有权与组装根注入方式  
- [ ] 建立根目录 `CHANGELOG.md`（Keep a Changelog 或等价）；crate 使用 **0.x** 语义化版本  
- [ ] 写明 **MSRV**（如在 `README` / `Cargo.toml` `rust-version`），CI 用 MSRV 工具链跑 `check`/`build`  
- [ ] `zui-backend-winit`：单平台建窗、resize、scale、键盘/鼠标  
- [ ] `zui-render`：attach surface、清屏一帧  
- [ ] `zui-backend-headless`：无窗跑一帧  
- [ ] 示例：`examples/empty_window`（组装根注入 backend）  

**完成标准（可测量）：**

- [ ] 主开发机运行 `empty_window`，手动确认窗口可缩放、关闭  
- [ ] `cargo test -p zui-backend-headless`（或等价）通过，至少 1 个「提交一帧」测试  
- [ ] CI：主开发机对应 OS 上 build + 上述测试绿，且 **MSRV job 绿**  
- [ ] `CHANGELOG.md` 已存在且含 M0a 条目；MSRV 版本号有文档可查  
- [ ] 第二节依赖图与所有权表可被 PR 引用；`zero-ui` 无 `target_os` cfg  

---

### M0b — 跨平台能编译 / 基础能跑（约 2–4 周，接 M0a）

**目标**：三桌面 OS 与 FreeBSD 进入 CI；运行能力可深浅不一。

- [ ] CI 矩阵：Linux / macOS / Windows 的 `build` + headless 测试  
- [ ] FreeBSD CI：**必达 `cargo check`**；runner 允许时再加 `build`（`build` 非必达）  
- [ ] 尽量使 `empty_window` 在 Win / macOS / Linux **运行**（允许列已知限制）  
- [ ] FreeBSD：**开窗运行不作为本里程碑必达**；在 `platform-status.md` 标明 check / build / run 各自状态  

**完成标准（可测量）：**

- [ ] Linux/macOS/Windows：`build` + headless 测试 CI 全绿  
- [ ] FreeBSD：`cargo check` CI 绿；若已有 `build` 则记录，但无 `build` 不阻塞 M0b  
- [ ] `docs/platform-status.md` 列出每平台：`check` / `build` / `empty_window 运行` / 已知限制  
- [ ] 至少 **2** 个桌面 OS 上 `empty_window` 冒烟通过（第三 OS 与 FreeBSD run 可标「进行中」）  

---

### M1 — 最小 UI 闭环（无 IME 验收）（约 4–8 周）

**目标**：键盘/鼠标可交互的小型示例；**不把 CJK IME 列入本里程碑必达**。

- [ ] 布局：`Stack` / `Row` / `Column` / `Padding` / 固定与伸缩  
- [ ] 控件：`Text`、`Button`、`TextInput`（拉丁键盘与基本编辑即可）  
- [ ] 焦点、命中测试、基础主题 token（`zero-ui` 内）  
- [ ] 示例：`counter` 与简单设置页（主题色/字号）  
- [ ] 布局或命中相关单测 ≥ 10 个  

**完成标准（可测量）：**

- [ ] `examples/counter` 在 M0b 已冒烟通过的平台上可运行  
- [ ] 指定操作路径可复现：点击按钮计数 +1；Tab 焦点切换；TextInput 拉丁字符增删  
- [ ] example 业务逻辑与 `zero-ui` 中无平台 `cfg`  
- [ ] 已知限制列表入库（例如：无 IME、无触控、单窗口）  

---

### M1.5 — IME（至少一平台 CJK）（约 3–6 周，可与 M2 部分并行）

**目标**：IME 走 `experimental` capability；打通一条 CJK 上屏路径。

- [ ] `zui-platform` IME 事件草案（preedit / commit / 候选区职责划分）  
- [ ] 一平台完整实现（建议 Linux 或 macOS）  
- [ ] `examples/text_input`：中文输入、提交、取消 preedit  
- [ ] 其它平台：能编译；行为写入 platform-status  

**完成标准（可测量）：**

- [ ] 目标平台：拼音（或系统 IME）输入「你好」并 commit 到 TextInput，可复现  
- [ ] preedit 显示与取消路径有手工验收步骤  
- [ ] API 仍标 `experimental`；changelog 记录差异  
- [ ] 明确非目标：候选窗口自绘美化、双拼/五笔专项等可列 Won’t have  

---

### M2 — 应用级可用（约 6–12 周）

**目标**：达到阶段一「能开发并打包小型桌面软件」的能力指标。

- [ ] 控件：`Scroll`、`List`、`Checkbox`、`Slider`、`Menu`、`Dialog`（可分批）  
- [ ] Clipboard、文件对话框、多窗口（capability 实现）  
- [ ] 字体与图标资源协议；HiDPI 场景清单与回归  
- [ ] 无障碍：控件树节点完善；平台桥可仍为 experimental/浅实现  
- [ ] 打包文档：至少 2 个桌面 OS 的构建与分发步骤  
- [ ] **继续**维护 `CHANGELOG` 与 0.x 版本；在文档中标出当前 `core` 稳定面与已知破坏性变更策略（1.0 另议，不在本里程碑强求）  

**完成标准（可测量）：**

- [ ] 「最小 App」教程：≥2 OS 上从 clone 到运行 ≤ 30 分钟（抽样）  
- [ ] 多窗 + 剪贴板往返各至少 1 条自动化或脚本化验收  
- [ ] HiDPI：列明测试组合（例如 1x/2x），并在 platform-status 勾选  
- [ ] 启动时间与空闲帧耗时：主开发机记录基线（观测项，不设硬 SLA）  
- [ ] 无未记录的 `todo!`/临时全局状态支撑 example 主路径  
- [ ] `CHANGELOG` 覆盖本里程碑用户可见变更；MSRV 仍有 CI  

---

### M3 — 过渡期 Shell（约 8–14 周，可与 M2 尾部重叠）

**目标**：同一 toolkit 做出 **受限** shell；**不依赖 winit 提供面板能力**。可先在 Linux Wayland 验证，再落 FreeBSD。

- [ ] `zui-platform`：`LayerShell` / `GlobalHotkey` / `Tray`（或 Notify）capability  
- [ ] `zero-backend-layershell`（及必要时 X11 strut/dock 适配）  
- [ ] `zero-backend-hotkey`、`zero-backend-tray`  
- [ ] `zero-shell` + `examples/panel_shell`：边缘面板 + 简单启动器  
- [ ] 以普通用户权限运行；所需能力仅通过协议/capability 申请；文档写明权限边界  
- [ ] 在**现有** compositor/会话中运行（不自研 compositor）  
- [ ] FreeBSD：选定目标会话（X11 或 Wayland）并写入 platform-status  

**完成标准（可测量）：**

- [ ] 面板在指定参考环境（写明 compositor/WM 与版本）固定于屏幕边缘  
- [ ] 全局快捷键 ≥1 个可调起启动器；托盘或通知至少一条路径可用  
- [ ] FreeBSD 上面板 + 启动器冒烟通过；连续运行 **8 小时** 无崩溃  
- [ ] 安全/权限：默认无额外特权；已知能力与 IPC 列表入库  
- [ ] widgets API 相对 M2 无强制破坏性变更  

---

### M4 — FreeBSD 桌面阶段二（数个季度）

**目标**：从「FreeBSD 上的 App/Shell」到「FreeBSD 会话」。

**不是绝对串行**，按依赖约束推进；无依赖部分可并行：

```text
  M3 完成（受限 shell + 现有 compositor）
           │
           ├──────────────────────────┐
           ▼                          ▼
  zero-services-*（可先做只读）    继续打磨 shell / DM 调研
           │                          │
           └──────────┬───────────────┘
                      ▼
              zero-session
           （依赖：能拉起 shell；
             服务可先 stub/可选）
                      │
                      ▼
           锁定 / 显示管理集成
                      │
                      ▼
           zero-comp（可选、后置；
           不阻塞「接现有 compositor 的会话」）
```

| 工作流 | 依赖 | 可并行对象 |
|--------|------|------------|
| `zero-services-*` | 本地 IPC 约定 | 与 shell 打磨、DM 调研并行 |
| `zero-session` | 可启动 M3 shell；服务可先可选 | 部分 UI/设置页 |
| DM / 锁屏集成 | session 原型 | 与更多 services 并行 |
| `zero-comp` | 明确「现有 compositor 不足」后 | **默认后置**，不挡会话 MVP |

**完成标准（可测量）：**

- [ ] FreeBSD 上固定步骤可复现：登录 → 自有会话组件拉起 → 面板/启动器/设置可用 → 能启动至少一个第三方应用与一个自有 example  
- [ ] 服务：至少 2 类（如电源只读 + 音量）经 **明确 IPC** 验收；服务进程权限最小化并有文档  
- [ ] 会话连续运行 **24 小时** 无崩溃（允许列已知泄漏/告警）  
- [ ] OpenBSD/NetBSD：不作为本里程碑验收项  
- [ ] 合成器：若未做，文档明确「依赖某某 compositor」及版本  

---

## 七、每个里程碑的工程纪律

1. **API 审查**：新公开能力先入 `zui-platform`（标成熟度）；SPI 变更不影响应用  
2. **变更记录**：自 M0 起维护 `CHANGELOG.md` 与语义化版本（前期 0.x）；破坏性变更必须记入 changelog  
3. **MSRV**：自 M0 起文档化并在 CI 验证；提升 MSRV 须写入 changelog  
4. **双后端验证**：涉及绘制的里程碑，窗 backend + `headless` 同时绿  
5. **FreeBSD 不掉队**：每里程碑更新 `docs/platform-status.md`  
6. **示例驱动**：每个能力至少一个 `examples/*`；组装在 example/`zero-ui-app`，不在 widgets  
7. **禁止范围**：M3 结束前不写自研 compositor；不做完整 DE 主题商店；不重写布局为浏览器级 CSS；不做 backend 热加载  
8. **验收**：完成标准以本文件勾选为准  

---

## 八、目录骨架

```text
zero-ui/
  Cargo.toml                 # workspace；feature 接线允许平台 cfg
  crates/
    zui-core/
    zui-platform/             # api + spi
    zui-render/
    zui-backend-winit/
    zui-backend-headless/
    zero-backend-layershell/ # M3
    zero-backend-hotkey/     # M3
    zero-backend-tray/       # M3
    zero-ui/
    zero-ui-app/
    zero-shell/              # M3
    # --- planned (M4) ---
    zero-backend-freebsd/    # planned
    zero-session/            # planned
    zero-services-power/     # planned（示例名）
    zero-services-audio/     # planned（示例名）
    zero-comp/               # planned；可选后置
  examples/
    empty_window/            # 组装根示例
    counter/
    text_input/
    panel_shell/             # M3
  docs/
    architecture.md          # 依赖方向 + platform↔render 所有权
    platform-api.md          # 公开 API vs SPI
    platform-status.md       # 每平台 check/build/run/限制
    msrv.md                  # MSRV 策略（可并入 README）
    roadmap.md               # 本文件
  CHANGELOG.md               # M0 起维护
```

---

## 九、人力与节奏（单人/小团队）

| 配置 | 建议节奏 |
|------|----------|
| 1 人兼职 | 严格串行 M0a→M0b→M1；IME/M2 控件集砍到最小 |
| 1 人全职 | ~2 个月到 M1；M1.5+M2 再 3–5 个月；M3 另计 |
| 2–3 人 | 一人 platform/render，一人 widgets/app，一人 FreeBSD/CI+shell 协议 |

阶段二按 M4 依赖图推进；**不要**和 M1 控件并行开太多前线。

---

## 十、近期 30 天执行清单

1. 建 workspace + CI（主 OS 测试；FreeBSD `check`；**MSRV job**）  
2. 建立 `CHANGELOG.md`、初始 crate 版本与 MSRV 声明  
3. 写下 `zui-platform` **core** 公开 API + 最小 `spi`（不冻结 IME/A11y）  
4. 写清依赖方向、组装根注入、platform↔render 所有权  
5. 打通主 OS 上 `empty_window`  
6. headless「一帧」测试  
7. 开始 `docs/platform-status.md`  
8. 维护本文件里程碑勾选  

---

## 十一、决策冻结（减少后期摇摆）

| 决策 | 选择 |
|------|------|
| UI 形态 | 自绘，不走原生控件/WebView 核心 |
| 依赖方向 | app → ui/platform/render；backend 仅组装根注入 |
| API 分层 | platform/render 公开 API vs platform SPI vs backend 实现 |
| 普通应用窗 | winit 第一后端 |
| Shell 专用能力 | 独立 backend；shell 为受限客户端 |
| 系统服务 | M4 最小权限进程 + IPC；不进阶段一契约 |
| GPU 生命周期 | Device 在 zui-render |
| 平台 API 演进 | core + capability + experimental |
| Backend 切换 | **仅编译期** feature；无插件 ABI 承诺 |
| cfg 规则 | widgets/业务禁止平台 cfg；backend/接线/组装允许 |
| 桌面路径 | toolkit → 受限 shell on existing session → FreeBSD session → compositor 可选 |
| BSD 策略 | **仅 FreeBSD 为阶段二正式目标**；M0b FreeBSD 必达 check |
| 布局 | 先简单 flex/stack |
| 版本与变更 | M0 起 CHANGELOG + 0.x 语义化版本 + MSRV；不把「开始记版本」推迟到 M2 |
| 迁移预期 | 尽量保持应用层不变、降低迁移成本；不承诺「零成本/无痛」 |
| 验收 | 编译与可用分阶段；完成标准可勾选 |

---

## 十二、可行性摘要（背景）

| 阶段 | 目标 | 可行性 |
|------|------|--------|
| 第一阶段 | Linux / macOS / Windows 桌面应用 | 高度可行 |
| 过渡期 | 同一 toolkit + 独立协议 backend 做受限 shell | 可行，取决于 Wayland/X11 适配投入 |
| 终极目标 | **FreeBSD** 桌面环境 | 可行，但是系统工程，不是控件库自然延伸 |

阶段二显示协议 API 谈 `Output` / `Seat` / `Surface`；IPC 自有一层且默认最小权限；配置与运行时路径可映射；渲染需能接受软渲染/备用后端。
