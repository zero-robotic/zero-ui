# Stack 叠层布局

`zui_widgets::stack(children)` 将直接子节点放入同一个内容区域，适用于背景上的文字、角标、装饰和组合面板。它输出普通 View；核心沿用既有绘制和命中遍历，不增加 Stack 或具体大屏控件分支。

## 尺寸与对齐

容器 padding 从边框盒中扣除，所有直接子节点共享剩余区域。自动尺寸按重叠层的最大尺寸贡献计算，而不是累加；子节点的 margin 和容器 padding 参与尺寸计算。百分比依据包含块解析，不能把不确定的百分比直接当作屏幕比例。

| 样式 | Stack 中的含义 |
| --- | --- |
| 父级 `align` | 子层默认垂直对齐 |
| 父级 `justify_items` | 子层默认水平对齐 |
| 子级 `align_self` | 覆盖垂直对齐；None 继承父级 |
| 子级 `justify_self` | 覆盖水平对齐；None 继承父级 |
| 子级 width/height/min/max | 子层自身尺寸约束 |

两个方向默认 Stretch，自动尺寸的子层填充可用区域；显式尺寸仍受自身约束。Start/Center/End 分别用于起始、居中和末端对齐。子层内部继续执行其自身的 Row、Column、Grid 或 Stack 布局。上述 self 对齐也适用于 Grid；Flex 使用 align_self 覆盖交叉轴对齐，justify_self 不控制 Flex 主轴。

```rust
use zui_api::layout::{Align, Dimension, Edges};
use zui_widgets::{Container, stack};
use zui_types::Color;

let background = Container::empty()
    .background(Color::rgb(12, 32, 50))
    .build();
let badge = Container::empty()
    .keyed("badge")
    .background(Color::rgb(20, 100, 100))
    .style(|s| {
        s.width = Dimension::Length(48.0);
        s.height = Dimension::Length(24.0);
        s.margin = Edges::all(12.0);
        s.align_self = Some(Align::Start);
        s.justify_self = Some(Align::End);
    })
    .build();
let panel = stack([background, badge])
    .style(|s| {
        s.width = Dimension::Length(320.0);
        s.height = Dimension::Length(180.0);
    })
    .build();
```

Stack 不接受非零 gap、wrap 或显式 rows/columns；这些配置返回 InvalidStyle，避免悄悄忽略调用意图。布局适配器内部将直接子层放在同一网格单元，并使用零最小值的弹性轨道：固定容器内超大的子层可以溢出，但不会扩大共享轨道并推移其他层的对齐位置。该策略不向公共 API 暴露 Taffy 类型；每轴仍占用一个轨道预算。

## 绘制与输入

子节点列表从后景到前景排列，后面的子层后绘制，命中按相反顺序查找。动态重排应提供稳定 Key，以保留控件局部状态。绘制顺序不等于键盘焦点隔离，叠加内容也不会自动捕获或阻止下层输入。

容器表面本身不命中，后代仍正常参与命中；透明颜色不自动让一个可命中的控件穿透。默认允许溢出，`clipped(true)` 复用边框盒裁剪，同时限制绘制和命中。需要阻止下层交互的遮罩必须提供明确的命中与事件行为。

当前实现不包含 z-index、绝对定位、忽略指针策略、脱离祖先裁剪的 Portal、模态焦点圈定或弹层关闭策略。这些是后续覆盖层和交互能力，不能仅以 Stack 代替。

## 验证与示例

布局测试覆盖共享区域、padding/margin、独立对齐、自然尺寸、百分比、嵌套 Row、不合法配置和轨道预算。顶层集成测试验证稳定 Key 重排、事件目标、溢出命中以及 CPU/GPU 的 1×/2× 像素顺序。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/stack-cpu.png stack
cargo test -p zui-render-check --test stack --all-features --locked
# 需要真实 GPU。
cargo test -p zui-render-check --test stack --all-features --locked -- --ignored
```
