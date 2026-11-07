# 按钮基础

`zui-widgets::ButtonView` 构造持久 Button，内部复用 Text 的测量、排版和字形绘制。默认、悬停、按下、禁用及焦点样式由控件层的 `ButtonStyle` 决定；核心不引用按钮类型。

```rust
use zui_api::{action::ActionId, layout::{Dimension, Edges, LayoutStyle}, view::View};
use zui_widgets::ButtonView;

// theme 与 families 由应用提供；字体仍需显式注册。
let button = View::new(ButtonView::themed(&theme, "保存", families, ActionId(1)))
    .keyed("save")
    .styled(LayoutStyle {
        min_height: Dimension::Length(44.0),
        padding: Edges { left: 16.0, right: 16.0, top: 8.0, bottom: 8.0 },
        ..Default::default()
    });
```

尺寸与 padding 使用通用布局属性，没有隐藏的控件布局算法。主题构造的文字默认水平居中；按钮在最终内容区域中垂直居中文字，并裁剪越界字形。当前支持文字换行和裁剪，尚无省略号、图标或任意子控件插槽。

## 交互语义

- 主鼠标按钮按下请求焦点和鼠标捕获，在按钮命中区域内松开激活；拖出后松开取消，拖回后松开可激活。
- Tab 可聚焦启用的按钮。Space 首次按下进入按下状态，松开激活；Enter 首次按下立即激活。键盘 repeat 不产生重复动作。
- Control/Alt/Super 修饰的 Space/Enter 不执行默认激活。Escape 取消尚未完成的按下手势，并释放鼠标捕获。
- 失焦或输入取消清除手势状态。按下期间禁用或更换 ActionId 会取消旧手势，不能将旧手势用来激活新绑定。
- 禁用按钮不获取焦点/捕获，也不输出激活动作；仍参与命中并处理主按钮输入，避免点击穿透到下层控件。

`Widget::enabled` 是通用契约。核心据此禁止焦点/捕获请求，并在属性协调后清理已禁用节点的所有权；控件自身负责禁用绘制及事件语义。禁用不改变按钮测量结果。

## 样式与消息

`ButtonStyle` 提供各状态的背景/前景颜色和焦点描边颜色，`from_theme` 从语义主题映射默认值。焦点描边为当前实现固定的 2 个逻辑单位；状态切换只重绘，不重新排版。主题颜色变化保持状态，标签或文字样式变化才重新布局。

成功激活输出 `WidgetAction`，ID 为应用提供的 ActionId，载荷为公开的 `ButtonActivated`。应用在 `map_action` 中检查 ID 与 `payload::<ButtonActivated>()`，映射成自有 Message，在 `message` 中修改业务状态。示例的“切换主题”按钮已使用这条路径。

```sh
cargo run -p zui-window-check --no-default-features --features cpu --locked -- cpu --widgets
cargo run -p zui-render-check --locked -- cpu artifacts/button-cpu.png button
```

窗口示例可点击按钮，或 Tab 聚焦后使用 Space/Enter；不再用全局 Space 快捷键代替按钮动作。定时冒烟模式仍通过模型定时事件切换主题，不能代替真实按钮点击验证。

## 验证和剩余范围

测试覆盖鼠标点击/拖出/拖回、键盘重复与取消、禁用期间的焦点/捕获清理、按下期间动作绑定变化，以及 CPU/GPU 上五种状态在 1×/2× 下的真实像素。预览使用真实按钮树，并通过事件路由设置焦点和按下状态。

本轮原生桌面自动化连接启动失败，实际鼠标点击和键盘激活尚未由桌面自动化验证。触摸、多指手势、无障碍角色/动作、焦点可见性策略、圆角、图标按钮和可访问性对比度验收仍待实现；这是一批可用的基础行为，不代表完整按钮验收或全部传统控件已完成。
