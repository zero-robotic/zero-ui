# Disclosure 可折叠面板

DisclosureView 由标题按钮与可选内容容器组成。标题复用 Button 的测量、绘制、指针捕获、Space/Enter 激活和取消机制；在 widgets 层转换为 DisclosureChanged，不向核心或渲染模块引入具体控件类型。

```rust,ignore
let mut panel = DisclosureView::themed(
    &theme, "高级设置", families, model.expanded, ActionId(10),
);
panel.key = Some("advanced-settings".into());
panel.layout.gap = Size::new(0.0, 12.0);
let view = panel.build(|| settings_view(&model.settings));
// map_action 读取 DisclosureChanged.expanded；message 更新模型并重建。
```

## 状态和生命周期

expanded 是应用确认的状态。标题激活只发出请求，不立即修改展开状态；如果应用拒绝，界面保持原状。标题通过 + / − 前缀显示收起/展开状态。

build 仅在 expanded=true 时调用一次内容工厂。收起时不生成内容 View；从展开切为收起会卸载整个内容子树。再次展开创建新节点，持久业务数据应放在模型，文本编辑状态可通过 TextInputDraft 保存。

根节点为 Column，标题为 Leaf，内容包装容器为 Column。header_layout 默认有 12 逻辑像素内边距且不收缩；layout、header_layout、panel_layout 可配置尺寸和间距，布局类型由组件固定。header_style 与 text_style 分别配置 Button 样式和文字，默认左对齐。key 用于在父容器中保持整个面板身份；内部标题和内容使用独立稳定 key。

标题占一个 Tab 焦点位置，展开后内容按普通树顺序参与焦点遍历。标题操作引起的展开/收起保留标题焦点；外部收起时，如果焦点位于被卸载内容，核心清理焦点，不自动迁移。外部展开状态变化会取消尚未完成的标题按压，防止释放旧手势触发过期操作。

disabled 只禁止标题交互，不改变展开状态，也不禁用已经展开的内容。

## 验证与后续

三项真实 WidgetHost/Taffy/Parley 集成测试覆盖受控请求、Space/Enter、外部变化取消按压、惰性构建次数、标题身份保留、内容卸载与重建、焦点清理，以及禁用标题下内容仍可聚焦。绘制复用 Button，本阶段未增加渲染原语或重跑 GPU 像素测试。

当前提供单面板基础。[Accordion](accordion.md) 已提供稳定项身份与单项/多项展开策略，并支持组内方向键导航；高度过渡动画、保留收起内容树、无障碍展开语义及原生设备验收也未完成。
