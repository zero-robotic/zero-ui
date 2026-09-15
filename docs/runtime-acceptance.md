# 最小运行与窗口状态验收表

> 适用入口：`examples/empty_window`、`examples/text_input`。自动化测试证明事件和状态机契约；本表验证真实窗口系统、显示缩放、IME 与 GPU surface 行为。

## 1. 执行前记录

每个平台每次验收先复制并填写一行。结果必须关联 commit；没有执行的项目保持 `未测`，不能用“可以编译”代替“可以运行”。

| 记录 ID | 日期 | commit | OS / 版本 | 桌面或窗口系统 | GPU / 驱动 | 显示器与缩放 | 执行人 |
|---|---|---|---|---|---|---|---|
| 示例：macos-arm64-01 | YYYY-MM-DD | `abcdef0` | macOS 15.x | Quartz | Apple M 系列 | 内屏 2x | 姓名 |
| 待填写 |  |  |  |  |  |  |  |

构建和自动化基线：

```bash
cargo test -p empty-window -p text-input
cargo run -p empty-window
cargo run -p text-input
```

运行前记录终端中的 warning/error；验收结束后使用正常关闭按钮退出，不以强制杀进程作为通过证据。

## 2. `empty_window`：窗口与 surface

启动命令：

```bash
cargo run -p empty-window
```

| ID | 场景 | 操作 | 通过标准 |
|---|---|---|---|
| EW-01 | 建窗与清屏 | 启动后保持 10 秒 | 只出现一个标题正确的窗口；客户区为连续一致的主题背景；无闪烁、旧内容和终端错误 |
| EW-02 | 普通 resize | 分别拖动四边和四角，覆盖变大、变小、宽窄窗口 | 客户区每帧完整清屏；没有未刷新的条带、拉伸帧、崩溃或 surface error |
| EW-03 | 连续 resize | 持续快速拖动窗口边缘至少 10 秒，结束后停留 3 秒 | 事件期间可短暂降帧，但停止后最终尺寸必须正确；CPU 不持续空转，窗口仍可交互 |
| EW-04 | scale change | 在 1x 与 2x 显示器间来回移动窗口至少 3 次；单显示器可切换系统缩放后重启复测 | 客户区始终覆盖当前物理 surface；无只占部分窗口、裁剪、模糊拉伸或崩溃 |
| EW-05 | 最小化/恢复 | 最小化并恢复 5 次，每次停留约 2 秒 | 恢复后首帧完整清屏；无黑帧滞留、无界重绘和 surface lost 循环 |
| EW-06 | 遮挡/恢复 | 用不透明窗口完全遮挡 5 秒，再移开；重复 3 次 | 再次可见时内容立即正确；终端无重复 acquire/present 错误，空闲 CPU 回落 |
| EW-07 | close | 使用标题栏关闭按钮 | 窗口关闭且进程正常退出；无 panic、GPU validation error 或后台残留进程 |

## 3. `text_input`：焦点、键盘与 IME

启动命令：

```bash
cargo run -p text-input
```

开始前启用系统 CJK 输入法。测试中观察的是输入框内的 composition，不要求框架自绘系统候选窗口。

| ID | 场景 | 操作 | 通过标准 |
|---|---|---|---|
| TI-01 | 指针焦点 | 依次点击第一、第二输入框并键入不同拉丁字符 | 只有当前点击的输入框接收文字并显示 caret；焦点背景随之切换 |
| TI-02 | 基础键盘 | 输入 ASCII 和中文标点，使用左右、Home、End、Backspace，并测试 Shift 选择 | 插入、移动、选择和删除作用于正确 UTF-8 边界；无重复字符 |
| TI-03 | IME preedit | 切换拼音等组合式输入法，输入 `nihao`，先不要选词 | preedit 在输入框内可见，但尚未写入正式文本；候选区靠近当前输入位置 |
| TI-04 | IME commit | 从 TI-03 选择“你好”并提交 | preedit 消失且“你好”只提交一次；caret 位于提交文本之后 |
| TI-05 | IME cancel | 开始新的 composition，再按系统输入法的取消键（通常为 Escape） | preedit 消失，取消内容不进入正式文本，原文本保持不变 |
| TI-06 | composition 中切换焦点 | 开始 preedit 后点击另一输入框 | 不崩溃、不把组合文本提交到错误输入框；记录该平台是 commit 还是 cancel，行为须稳定 |
| TI-07 | resize/scale 下输入 | composition 期间 resize；随后在 1x/2x 显示器间移动并继续输入 | 文本、caret 和 preedit 保持清晰且位置正确；候选区不长期停留在旧坐标 |
| TI-08 | close | composition 期间关闭窗口 | 进程正常退出，无 panic 或残留候选窗口 |

## 4. 1x / 2x 与窗口状态矩阵

每个目标桌面 OS 至少填写一组；具有双缩放显示器的环境应在同一记录中完成 scale transition。

结果填写 `通过`、`失败（issue 链接）`、`受限（说明）` 或 `未测`。

| 记录 ID | 1x 建窗/清屏 | 1x 连续 resize | 2x 建窗/清屏 | 2x 连续 resize | 1x↔2x | 最小化/恢复 | 遮挡/恢复 | close |
|---|---|---|---|---|---|---|---|---|
| Linux | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 |
| macOS | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 |
| Windows | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 |

| 记录 ID | 焦点/键盘 | preedit | commit | cancel | composition 切焦点 | resize 中 IME | 1x↔2x IME | close |
|---|---|---|---|---|---|---|---|---|
| Linux | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 |
| macOS | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 |
| Windows | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 | 未测 |

## 5. 失败记录要求

失败项至少附上：记录 ID、测试 ID、commit、操作系统、缩放组合、复现步骤、终端日志，以及能说明问题的截图或录屏。涉及 resize/scale 时还要记录窗口从哪个显示器移动到哪个显示器；涉及 IME 时记录输入法名称和取消方式。

以下情况不能标记为通过：

- 只有 `cargo check` 或 headless 测试结果，没有真实窗口运行；
- 通过强制终止绕过 close 或卡死；
- 字体、caret 或客户区由旧的低分辨率帧拉伸；
- 最小化或遮挡后持续请求帧、持续报 surface error；
- preedit 被提前写入、commit 重复，或 cancel 后仍留下文本。
