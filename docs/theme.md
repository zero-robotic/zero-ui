# 主题变量与更新

`zui-theme` 是 `no_std`、无堆分配的通用值模块，仅依赖 `zui-types`。它不认识控件、布局引擎、字体注册器、核心或渲染器；控件状态到主题值的映射属于控件层。

当前提供：

- `ColorRole`：页面背景、面板、抬升面板、正文、次要文字、主色、主色上的文字、边界及成功/警告/危险颜色。
- `TypeRole`：Caption、Body、Heading、Display，对应字号和相对行高。
- `Space`：None、Xs、Sm、Md、Lg、Xl，对应逻辑间距。
- `Theme::dark()`、`light()` 和 `dashboard()`：三套配色，共享默认字号与间距。

主题是应用拥有的普通值，无全局主题管理器。构建 View 时读取当前主题，将语义变量转为普通属性，再通过现有 `Tree::reconcile` 更新。主题改变不会自动修改已经构建的 View；应用必须重建描述并提交协调。

```rust
use zui_api::view::View;
use zui_theme::{ColorRole, Space, Theme, TypeRole};
use zui_widgets::{Container, TextView};

// families 来自调用方显式注册的字体族。
let theme = Theme::dashboard();
let text = TextView::themed(&theme, "设备状态", families, TypeRole::Body);
let view = Container::new(View::new(text))
    .background(theme.color(ColorRole::Surface))
    .style(|s| s.padding = zui_api::layout::Edges::all(theme.space(Space::Md)))
    .build();
```

`TextView::themed` 默认使用正文颜色，调用方可进一步覆盖其 color、align 等属性。字体族仍由应用提供；主题不会扫描字体、读取文件或改变文字服务。字号与行高必须为正且有限，间距必须非负且有限。主题变量验证不替代文字服务的资源预算，例如超出服务最大字号仍会返回排版错误。

局部覆盖通过克隆主题并调用 `set_color`、`set_typography`、`set_space` 完成，再用该副本构建指定子树；原主题不变。当前没有隐式的树级继承、样式选择器或优先级规则。

主题本身不发出“全树重排”指令。协调按最终使用的属性决定失效：

| 实际变化 | 当前行为 |
| --- | --- |
| 背景或文字颜色 | 仅绘制失效，保留已排版字形和节点身份 |
| 字号、行高、字体族 | 文本布局失效，重新测量和准备 |
| padding、gap 等布局值 | 布局失效 |
| 没有被本次 View 使用的变量 | 不改变控件属性，不产生额外失效 |

预览与测试由顶层验证工程装配：

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/theme-light.png theme-light
cargo run -p zui-render-check --locked -- cpu artifacts/theme-dashboard.png theme-dashboard
```

测试验证配色切换后像素改变、字形和节点身份保留、无额外排版调用；字号和间距修改触发重新布局。CPU/GPU 执行同一验证路径。

当前仅交付基础变量与显式解析。圆角、边框宽度、动效时长、控件悬停/按下/禁用状态映射、系统主题跟随、主题过渡动画和可访问性对比度验收仍待实现。Dashboard 配色不意味着已实现辉光、滤镜或完整大屏组件。
