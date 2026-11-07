# Tabs 页签容器

TabsView 在控件层组合页签栏和当前页面。页签栏复用 SegmentedControl 的布局、绘制和选择交互；通过 TabChanged 提交选择请求，由应用修改 selected 后重建。核心、布局、渲染模块无需认识 Tabs。

```rust,ignore
let mut tabs = TabsView::themed(
    &theme,
    vec![TabOption::new(TabId(1), "概览"), TabOption::new(TabId(2), "趋势")],
    families,
    Some(model.selected),
    ActionId(42),
);
tabs.layout.width = Dimension::Percent(1.0);
let view = tabs.build(|id| model.page(id))?;
// map_action 读取 TabChanged，message 更新 model.selected。
```

## 构建与身份

- TabId 在同一容器内唯一且保持业务身份，不以位置作为 ID。
- build 在调用内容工厂前校验重复 ID 和不存在的 selected，返回 TabsError；无选择时不调用工厂，合法选择时恰好调用一次。空列表允许无选择。
- 选中页可以处于 disabled 状态，仍显示内容；disabled 仅阻止页签交互，不禁用页面内的控件。不会自动改选第一项。
- 根节点为 Column；页签栏使用固定文本 key，页面包装容器使用数字 TabId key，二者不会冲突。同一选择的重建、页签重排保留页面身份；切换选择卸载旧页面，即使两页内部使用相同 key 也不会误复用。
- 仅挂载选中页，不缓存未选中页的控件树。持久业务数据放在应用模型；编辑状态可配合 TextInputDraft 保存。再次打开旧页会创建新节点。

## 布局、外观与输入

layout 配置整个容器，header_layout 配置页签栏尺寸，panel_layout 配置页面包装容器。根与页面包装容器的布局类型固定为 Column；页签栏固定为 Leaf。默认页签栏 shrink 为 0；固定高度容器的剩余空间分配可由应用设置 panel_layout.grow。内容滚动由页面显式组合 Scroll。

header_style 使用 SegmentedStyle，text_style 配置文字；标签栏共用一个外边框，相邻标签只绘制一条分隔线，间隙固定为零（忽略 header_style.gap）；默认 `compact=true`，页签按文字宽度和内边距紧凑靠左排列，不填满剩余空间；`compact=false` 恢复等宽布局。空间不足时按比例收缩并允许标签换行，正常/选中/禁用和焦点视觉复用分段控件。

页签栏占一个 Tab 焦点位置。方向键、Home/End 选择并跳过禁用项，Space 请求活动项，指针沿用捕获与取消规则。模型确认选择后，栏内焦点保持；Tab 按树顺序进入当前页。外部切换若卸载了焦点所在页面，核心清理焦点，不自动跳到新页面。

## 验证与范围

窗口侧集成测试使用真实 Taffy/Parley 和 WidgetHost，验证动作模型回写、禁用跳过、页面工厂/构建/销毁次数、稳定节点、进入内容区、切换清理焦点及无效选择。另回归 SegmentedControl 模型回写。本阶段没有新增绘制原语，未重跑 GPU 像素测试。

当前支持横向紧凑/等宽页签、关闭、拖动重排和单页挂载。关闭与拖动命中使用实际标签边界。页签溢出滚动、竖向/RTL、Ctrl+Tab、保留非活动控件树和页签级无障碍语义尚未实现。

## 可运行示例

```sh
# CPU 原生窗口：概览、可编辑巡检备注、禁用归档页签
cargo run -p zui-window-check --locked --offline -- cpu --tabs
# GPU 原生窗口（需可用 GPU 和桌面环境）
cargo run -p zui-window-check --features gpu --locked --offline -- gpu --tabs
# 有限时长运行，在中途自动切换到巡检页
cargo run -p zui-window-check --locked --offline -- cpu --tabs --smoke-ms 2000
# 静态 CPU 预览
cargo run -p zui-render-check --locked --offline -- cpu artifacts/tabs-cpu.png tabs
```

窗口示例在应用层拥有 selected、备注字符串和 TextInputDraft，通过 TabChanged/TextInputEvent 更新模型；控件不引用示例业务类型。--tabs 与 --widgets 互斥。巡检页面卸载后释放草稿的活动绑定，重新进入时恢复绑定；示例集成测试实际经过 WidgetHost 和文本布局验证这一过程。撤销/选区保存机制沿用 TextInputDraft，详见 [文本输入](text-input.md)。

静态预览位于 artifacts/tabs-cpu.png，已目视检查文字、选中/禁用状态和页面布局。窗口示例已编译、完成模型集成测试及严格 Clippy；原生窗口手动交互、实际 IME 和 GPU 运行尚未在本阶段验收。

新增增强交互及能力边界见 [控件增强](advanced-controls.md)。
