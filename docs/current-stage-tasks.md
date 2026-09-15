# zero-ui 当前阶段整改任务

> 状态基线：2026-09-15，P1 整改基于 `83bf2e3` 开始
> 关联规划：[roadmap.md](roadmap.md)

## 1. 阶段结论

当前代码已经具备 winit/headless 后端、基础控件、组件入口、GPU 渲染、增量场景更新和 platform core v0。P1 已补齐受控多窗口、Paths、Output/scale、类型化 capability、IME、无障碍树入口与 UI 线程任务；剩余主缺口是 roadmap 中 M0a/M0b 的跨平台 CI、示例和手工验收闭环。surface 所有权边界的整改状态记录在第 4 节。

当前阶段不以增加控件数量或继续优化 GPU 热路径为主，整改顺序固定为：

1. 建立工程与跨平台验收基线；
2. 收敛 platform/render/SPI 与 surface 生命周期；
3. 补齐 `zui-platform` core v0 和 capability 扩展点。

## 2. 本阶段范围与约束

### 本阶段必须完成

- M0a/M0b 的 CI、MSRV、FreeBSD check、文档和示例闭环；
- surface 从创建到销毁的唯一所有权和状态机；
- backend SPI 依赖边界整改；
- `zui-platform` 阶段一 core API 的最小完整面；
- winit 与 headless 对同一平台和渲染契约的验证。

### 本阶段暂停

- 新增复杂 GPU 缓存、批处理或局部重绘优化；
- 扩充非必要 M2 控件；
- CPU renderer、software-GPU renderer 的占位架构扩张；
- M3 的 layer-shell、托盘、全局快捷键；
- M4 的 session、services、compositor。

允许修复阻塞验收的正确性问题，但修复必须补充对应的回归测试或验收项。

## 3. P0：建立工程与跨平台验收基线

### 目标

把“本机能够运行”变成可重复、可追踪的跨平台验收结果，先完成 roadmap 的 M0a/M0b 门槛。

### 任务

- [ ] 新增 Linux、macOS、Windows CI：执行 build、workspace tests 和 headless tests；
- [ ] 使用 workspace 声明的 MSRV `1.85` 增加独立 check/build job；
- [ ] 增加 FreeBSD `cargo check`，并明确 build/run 是否支持；
- [ ] 创建 `CHANGELOG.md`，记录当前 `0.1.x` 已有用户可见能力和破坏性变更策略；
- [x] 创建 `docs/architecture.md`，固化依赖方向、组装根和 platform/render 所有权；
- [x] 创建 `docs/platform-api.md`，区分公开 API、capability 与 backend SPI；
- [ ] 创建 `docs/platform-status.md`，记录各平台 check/build/run、HiDPI 和已知限制；
- [ ] 创建 `docs/msrv.md` 或在 README 中明确 MSRV 策略；
- [x] 增加 `examples/empty_window`，只验证建窗、清屏、resize、scale 和关闭；
- [x] 增加 `examples/text_input`，作为键盘、焦点和后续 IME 的稳定验收入口；
- [x] 建立 [最小运行与窗口状态验收表](runtime-acceptance.md)，覆盖 1x/2x、连续 resize、最小化/恢复、遮挡/恢复；
- [ ] 依据真实证据更新 roadmap 勾选状态，不以“已有相似代码”代替验收。

### 验收标准

- [ ] `cargo fmt --all -- --check` 通过；
- [ ] `cargo test --workspace` 通过；
- [ ] Linux、macOS、Windows CI 全绿；
- [ ] MSRV job 全绿；
- [ ] FreeBSD `cargo check` 全绿；
- [ ] headless 至少验证一次完整的 backend → app → widget tree → scene → frame 链路；
- [ ] 至少两个桌面系统完成 `empty_window` 的 resize/scale/close 冒烟测试；
- [ ] 所有平台结果和限制写入 `docs/platform-status.md`。

## 4. P0：收敛 surface 所有权和 SPI 边界

### 整改前问题

roadmap 规定 backend SPI 仅供 `zui-backend-*` 使用，但当前 `zui-render` 和 `zui-render-runtime` 都直接引用 `zui-platform::spi::RawWindowHandleProvider`。同时 surface 生命周期分别由 winit backend、`zui-app::RunnerState` 和 renderer 的内部状态管理，造成 resize、scale、outdated、lost、deferred 等状态需要跨三层协同，容易产生重复配置、漏重绘和陈旧帧。

### 目标边界

```text
backend
  ├─ 拥有原生窗口和事件循环
  ├─ 实现 platform core/API
  └─ 提供受控的原生 surface target 借用
             │
             ▼
zui-app（唯一编排者）
  ├─ 消费 platform 事件
  ├─ 驱动 UI layout/paint
  └─ 调用 render surface/frame API
             │
             ▼
zui-render
  ├─ 拥有 Instance/Adapter/Device/Queue
  ├─ 拥有 wgpu::Surface 和配置
  └─ 独立处理 attach/resize/present/recover
```

### 任务

- [x] 在 `docs/architecture.md` 固化原生 window/display handle 的借用决策；
- [x] 使用 `raw-window-handle` 标准借用 trait，并通过持有 handle source 保证生命周期；
- [x] 移除 `zui-render`、`zui-render-runtime` 对 `zui-platform::spi` 的直接引用；
- [x] 将 `zui-render-runtime` 限定为 renderer 选择和 app-facing adapter 层；
- [x] 定义并文档化 surface 状态机：
  `Detached → Attached → Resized → Presented`，以及 `Deferred`、`Outdated`、`Lost` 的恢复路径；
- [x] 明确每个状态转换的唯一负责人，禁止 backend、app、render 重复维护同一状态；
- [x] 统一 resize 与 scale 事件中的逻辑尺寸、物理尺寸和 device scale 计算来源；
- [x] 为 attach、resize、零尺寸、deferred retry、outdated/lost recovery、detach 建立契约测试；
- [x] winit 和 headless 运行同一组可共享的生命周期测试。

### 验收标准

- [x] `zui-platform::spi` 仅被 `zui-backend-*` 引用；
- [x] `zui-render` 不依赖具体 backend 类型；
- [x] 每个 surface 状态只有一个权威数据源；
- [x] deferred frame 重试不重建已接受的 scene；
- [x] resize/scale 后第一帧必定完整呈现，且文字和图像保持原生物理分辨率；
- [x] lost/outdated 后由 renderer 使用已持有 target 恢复，并强制完整帧；
- [ ] 最小化、遮挡和恢复不会进入无界重绘循环；
- [x] 生命周期规则写入 `docs/architecture.md`，测试覆盖关键转换。

## 5. P1：补齐 `zui-platform` core v0 与 capability 扩展点

### 完成结果

`AppLoop<H>` 的单 Host 借用已替换为对象安全的 `AppLoop + AppContext`。backend 在上下文中管理 Host 集合，所有窗口调度都显式携带 `WindowId`；headless 与 winit 实现同一生命周期。公开 API 按 core、capability、experimental 分层，完整契约见 [platform-api.md](platform-api.md)。

### 任务

- [x] 冻结并文档化 `core v0`：AppLoop、退出/调度、Host/Window、基础输入；
- [x] 引入受控的应用上下文或窗口管理接口，支持运行期创建、查询和销毁多个窗口；
- [x] 将窗口 resize 与 scale change 建模为明确事件，避免依赖调用方推断；
- [x] 增加 `Paths`：config、data、cache、runtime 的平台无关接口；
- [x] 增加 `Output` 快照和变更事件，至少包含标识、逻辑区域和 scale；
- [x] 定义 capability 的查询/注入方式，不要求所有 backend 实现所有能力；
- [x] 增加 Clipboard、Dialog、DragDrop 的 capability trait 草案；
- [x] 增加 experimental IME 契约：enabled、preedit、commit、cancel、cursor area；
- [x] 增加 experimental accessibility 语义树提交入口；
- [x] 增加后台任务安全投递到 UI 线程的最小接口；
- [x] 为 core/capability 标注成熟度，并记录破坏性变更策略；
- [x] 先在 headless 实现契约测试，再接入 winit。

### 验收标准

- [x] example 业务代码不依赖 winit、OS crate 或 `zui-platform::spi`；
- [x] backend 切换只涉及 Cargo feature 和组装根；
- [x] headless 能创建、驱动并销毁至少两个 Host；
- [x] Output/scale 变化能够到达 app，并触发正确的 layout 与 surface resize；
- [x] Paths 在支持平台返回符合约定的目录，失败使用结构化错误；
- [x] TextInput 能通过公开 IME API 观察 preedit、commit 和 cancel；
- [x] 未实现 capability 能被可靠检测，不以 panic 或静默失败代替；
- [x] `docs/platform-api.md` 与实现保持一致。

### 自动化证据

- `zui-backend-headless`：双 Host 生命周期、resize/scale/Output、Paths、unsupported capability、UI task 和无界重绘保护；
- `zui-app`：scale → layout/surface resize、语义树 capability 提交、surface deferred/recover；
- `zui-ui`：`TextInput` 的 preedit、commit、cancel 状态机；
- `zui-backend-winit`：原生 target 生命周期、平台路径和结构化路径失败；winit 主代码映射多窗口、monitor、IME 与文件拖放事件。

## 6. 执行顺序与合并门槛

### 第一批：建立基线

先完成 P0 工程文件、`empty_window`、CI 和平台状态表。该批不修改核心架构，目标是建立后续重构的安全网。

### 第二批：surface/SPI 重构

ADR 先行，再修改依赖边界和 surface 生命周期。每次合并必须同时包含 headless 契约测试；涉及 winit 行为时同步更新手工验收表。

### 第三批：platform core v0

按 Window/Host → Paths/Output → capability/IME 的顺序推进。每新增一项公开 API，先补文档和 headless 测试，再实现 winit。

### 每个 PR 的统一门槛

- [x] 不引入新的跨层反向依赖；
- [x] 新公开类型标明 core、capability 或 experimental；
- [x] 正确性修改有自动化测试或明确的手工验收步骤；
- [x] `cargo fmt --all -- --check` 通过；
- [x] `cargo test --workspace` 通过；
- [x] 更新相关架构、平台状态或 changelog 文档。

## 7. 当前阶段退出条件

只有同时满足以下条件，才进入新的 M2 控件扩展或渲染性能阶段：

- [ ] M0a/M0b 的可测量完成标准已有 CI 和文档证据；
- [ ] FreeBSD `cargo check` 持续通过；
- [ ] surface 状态机和所有权边界稳定，SPI 无跨层泄漏；
- [x] winit/headless 生命周期契约测试通过；
- [x] platform core v0 覆盖 Window、Input、Paths、Output/scale；
- [ ] HiDPI、连续 resize、最小化/恢复不再存在已知阻断问题；
- [ ] roadmap、platform-status 与 CHANGELOG 已同步更新。
