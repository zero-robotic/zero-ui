# Icon 演进路线

本文说明 `Icon` 和 `IconButton` 的当前实现、扩展方式，以及后续从内置矢量图标演进到通用路径渲染的计划。

## 当前实现

当前图标位于 `zui-ui` 的 widgets 层：

```text
IconName
   │
   ▼
paint_icon()
   │
   ▼
PaintContext::draw_line()
   │
   ▼
zui-render 的抗锯齿线条 Shader
```

`IconName` 提供有限的内置图标，例如：

- `Check`
- `Close`
- `Menu`
- `Plus` / `Minus`
- `Play` / `Pause`
- `ArrowLeft` / `ArrowRight`

添加一个简单图标的当前方式是在 `IconName` 中增加枚举值，并在 `paint_icon()` 中增加绘制逻辑。图标使用归一化坐标 `0.0..1.0` 描述，因此可以适配不同尺寸的 `Icon` 和 `IconButton`。

这种方式适合早期阶段：API 简单、没有外部资源依赖，并且可以复用现有的线条抗锯齿管线。

## 当前阶段的扩展规则

新增内置图标时应遵循以下规则：

1. 使用 `IconName` 的语义名称，不使用平台或字体相关名称。
2. 使用归一化坐标，不直接写死像素尺寸。
3. 通过 `IconStyle` 或 `IconButtonStyle` 控制颜色、尺寸和线宽。
4. 图标绘制只负责几何，不在图标内部处理点击、焦点或业务状态。
5. 需要状态的图标由父控件决定，例如 `Switch` 决定使用开或关状态。

## 短期演进：提取 `IconData`

当图标数量继续增加时，应将 `paint_icon()` 中的 `match` 拆分为「图标数据」和「通用绘制器」：

```rust
pub struct IconData {
    pub view_box: Size,
    pub paths: &'static [PathCommand],
}

pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    Close,
}
```

之后 `IconName` 只负责返回对应的 `IconData`：

```text
IconName ──▶ IconData ──▶ PathRenderer ──▶ PaintContext
```

这一阶段的目标是：

- 消除大型 `match` 绘制函数。
- 统一坐标变换、缩放和线宽处理。
- 让 `Icon`、`IconButton` 和其他控件共享同一套图标数据。
- 为曲线和填充路径预留扩展位置。

## 中期演进：通用矢量路径

仅支持直线不足以表达完整图标集合。下一阶段应增加：

```rust
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    QuadTo { control: Point, to: Point },
    CurveTo {
        control1: Point,
        control2: Point,
        to: Point,
    },
    Close,
}
```

路径数据应保持平台无关，放在 `zui-core` 或独立的矢量数据模块；路径的 GPU 栅格化放在 `zui-render`。`zui-ui` 只负责选择图标和提供样式，不应直接依赖 wgpu。

推荐的职责划分：

| 模块 | 职责 |
|---|---|
| `zui-core` | 点、路径命令、ViewBox 等基础矢量数据 |
| `zui-ui` | `Icon`、`IconButton`、图标语义和主题 |
| `zui-render` | 路径变换、填充、描边、抗锯齿和缓存 |
| 应用层 | 业务自定义图标和状态映射 |

## 长期演进：外部图标和 SVG

在路径模型稳定后，可以增加外部图标来源：

```rust
pub enum IconSource {
    Builtin(IconName),
    Path(IconData),
    Svg(Arc<SvgDocument>),
}
```

外部 SVG 不应直接在每次绘制时解析。推荐流程是：

```text
SVG 文件
  │ 解析一次
  ▼
IconData / PathCache
  │ 缩放、颜色、线宽作为缓存键
  ▼
GPU 绘制
```

需要注意：

- SVG 解析属于资源加载，不应发生在 paint 热路径。
- 缓存键至少应包含图标资源、尺寸、scale factor、颜色和描边参数。
- 对固定内置图标可以使用静态数据，避免运行时解析。
- 外部资源加载失败时应提供明确的占位图标或错误结果。

## IconButton 的演进

`IconButton` 应始终保持为普通控件，不把图标数据和交互逻辑耦合在一起：

```text
IconButton
 ├── IconSource
 ├── ButtonState（normal / hover / pressed / disabled）
 ├── Theme
 └── Action::Clicked
```

后续可以增加：

- `disabled` 状态。
- pressed / focused 样式。
- 无障碍名称和语义角色。
- tooltip。
- 键盘激活和焦点环。

这些能力应扩展 `IconButton` 的状态和主题，不应修改 `IconData` 的格式。

## 推荐迁移顺序

1. 当前阶段：继续使用 `IconName + paint_icon()`，快速补齐常用图标。
2. 图标数量达到维护困难时：提取 `IconData` 和 `PathCommand`。
3. 需要复杂图标时：增加曲线、填充和描边路径。
4. 路径渲染稳定后：增加路径缓存和资源级缓存。
5. 最后再支持 SVG 和应用自定义图标。

迁移过程中应保持 `Icon::new(...)` 和 `IconButton::new(...)` 的高层 API 稳定，只替换图标数据来源和底层渲染实现。
