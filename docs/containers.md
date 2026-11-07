# 标准布局容器

`zui-widgets` 提供 `Container` 和 `row`、`column`、`grid`、`stack` 声明式构造入口。构造器输出普通 `View`，持久节点、子节点管理、Key 协调和布局仍由既有公共契约完成；控件库不依赖核心或 Taffy。

| 入口 | 子节点 | 布局行为 |
| --- | --- | --- |
| `Container::new(child)` | 一个 | 单列盒子；支持内边距、尺寸与对齐 |
| `Container::empty()` | 无 | 可用于背景面板或固定尺寸占位 |
| `row(children)` | 多个 | 横向 Flex，可通过样式开启换行 |
| `column(children)` | 多个 | 纵向 Flex |
| `grid(columns, children)` | 多个 | 指定列轨道，按既有 Grid 自动放置规则排列 |
| `stack(children)` | 多个 | 共享内容区域，按子节点顺序叠放；见 [Stack](stack.md) |

所有构造器共享 `style`、`background`、`keyed`、`clipped` 和 `build` 方法，也可以用 `Into<View>` 转换。`style` 接收修改闭包，保留未修改的默认值；构造器决定布局 kind，闭包对 kind 的修改会被忽略。`build` 后得到普通 View，调用方应保持其布局类型与容器意图一致。

```rust
use zui_api::layout::{Dimension, Edges, Track};
use zui_types::{Color, Size};
use zui_widgets::{Container, grid, row};

let sidebar = Container::empty()
    .keyed("sidebar")
    .background(Color::rgb(25, 50, 80))
    .style(|s| {
        s.width = Dimension::Length(144.0);
        s.shrink = 0.0;
    })
    .build();
let cards = grid([Track::Fraction(1.0); 2], [])
    .style(|s| {
        s.grow = 1.0;
        s.min_width = Dimension::Length(0.0);
        s.gap = Size::new(12.0, 12.0);
    })
    .build();
let page = row([sidebar, cards])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Percent(1.0);
        s.padding = Edges::all(16.0);
        s.gap = Size::new(16.0, 0.0);
    })
    .build();
```

尺寸使用 border-box；以下主轴/交叉轴说明适用于 Flex 容器。`align` 控制交叉轴，`justify` 控制主轴。对于单子节点 Container，交叉轴为水平方向、主轴为垂直方向。默认 align 为 Stretch；要让有固定宽度的子节点居中，应显式设为 Center。

背景覆盖整个边框盒。默认不裁剪，`clipped(true)` 同时约束自身/后代绘制和命中。容器表面本身不参与命中，空白区域不会吞掉输入；后代仍按核心的逆绘制顺序命中。当前容器不是可点击面板，也未实现滚动、圆角或边框样式。

背景变化仅产生绘制失效。尺寸、间距、方向或子节点顺序改变需要重新布局；Row 与 Column 共享表面控件实现，切换方向可保留同一父节点下具有稳定 Key 的子节点身份。标准容器不会为子节点自动生成 Key；动态列表应由应用提供稳定 Key。

现有布局预览已使用这些公开入口：

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/layout-cpu.png layout
```

验证覆盖单子节点对齐、内边距、背景、溢出裁剪与命中、1×/2× 像素、Key 重排和仅背景变化跳过布局；嵌套 Flex/Grid 预览另验证轨道与间隙。Stack 已验证独立对齐、叠放和重排，详见 [叠层布局](stack.md)。[ScrollView 纵向基础](scroll.md) 已接入。Grid 显式放置/跨轨道、虚拟化及完整滚动交互仍按计划实现。
