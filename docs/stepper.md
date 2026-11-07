# Stepper 数值步进器

StepperView 在控件层组合减号按钮、数值 Text 和加号按钮。底层 Button 不感知 Stepper；内部包装仅把 ButtonActivated 转成 StepperChanged。数值计算复用 NumericRange，与 Slider 使用同一套步长和端点规则。

```rust
use zui_api::action::ActionId;
use zui_widgets::{NumericRange, StepperView};

let range = NumericRange::new(0.0, 10.0, 3.0)?;
let mut stepper = StepperView::themed(&theme, families, 3.0, range, ActionId(19));
stepper.display = Some("3 台设备".into());
let view = stepper.build()?;
```

`StepperChanged { value }` 是请求，应用确认后更新 value 并重建。拒绝请求不会改变数值或累加下一步。value 必须有限且在范围内，build 在创建子节点前拒绝非法值；允许非步长网格上的值，向上/向下到对应方向的相邻网格。端点始终可选，例如 0..10、step=3 时 9→10、10→9。

display 是可选的业务格式化字符串，不参与计算，也不作为可编辑文本解析；未设置时用 value 的标准字符串表示。单位、本地化及小数位由应用决定。按钮与数值文字样式、三部分布局、根 key 均可配置。根固定采用 Row，子节点使用 decrement/value/increment 稳定 key。

减号和加号分别是普通按钮焦点，支持鼠标、Space 和 Enter。到达端点时对应按钮禁用；disabled 禁用两个按钮，数值继续显示。确认数值或范围变化时取消尚未释放的手势，即便新旧数值算出的下一步相同；动作绑定和禁用变化沿用 Button 的取消规则。更新保留子节点身份；焦点按钮变为禁用时由 core 清理焦点。

当前不是可编辑 SpinBox，也未提供单焦点方向键、长按连发、滚轮调值或控件级无障碍语义。

## 验证

三项真实 Tree/Taffy/Parley 集成测试覆盖受控鼠标和键盘请求、非网格数值、短末区间、端点及整体禁用、外部改值/范围/绑定取消、格式化更新保留身份，以及 NaN/无穷/越界拒绝。Button 四项回归通过。widgets/render-check 严格 Clippy、格式、23 包依赖检查和 12 包 Cortex-M 裸机库构建通过。

复用既有 Button/Text 绘制，本阶段未新增 CPU/GPU 像素或原生设备专项验收。

## 应用示例

`window-check --widgets` 中的 Stepper 与 Slider、ProgressBar 共用 `Demo.level`（0..100、步长 5）。Stepper 使用 ActionId(6) 映射为 SetLevel，应用确认后重建三者；百分比字符串由应用格式化。没有将业务模型或消息类型引入 widgets。

```sh
cargo run -p zui-window-check --features gpu --locked --offline -- cpu --widgets --smoke-ms 1600
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --widgets --smoke-ms 1600
```

2026-10-28 当前 macOS 开发机，两条命令均退出码 0，均报告 events=12/redraws=3，逻辑窗口 800×480、scale=2，并完成自动主题切换。该记录验证窗口与呈现路径，不代表原生手动交互或截图验收。

扩展的 Host 集成测试验证 Slider 改值后 Stepper 请求回写到同一模型、零值时 Tab 跳过禁用减号、按钮激活重建后焦点保持、后续 TextInput 输入不受影响，以及主题切换后步进按钮身份保留。window-check 全目标全特性严格 Clippy、格式及依赖检查通过。
