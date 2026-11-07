# Accordion 分组折叠容器

AccordionView 在 widgets 层组合 DisclosureView；核心不感知组、项身份或展开策略。每项以稳定 AccordionId 作为子树 key，重排不改变同一项的节点身份。标题与内容的布局、交互及收起卸载规则继承 [Disclosure](disclosure.md)。

```rust,ignore
let view = AccordionView::themed(
    &theme,
    vec![
        AccordionItem::new(AccordionId(1), "设备参数"),
        AccordionItem::new(AccordionId(2), "高级设置"),
    ],
    families,
    AccordionExpansion::Single(model.expanded),
    ActionId(20),
).build(|id| model.settings_page(id))?;
```

## 展开策略和动作

AccordionExpansion::Single(Option<AccordionId>) 最多展开一项；打开其他项时，请求状态只保留新项。激活当前展开项请求全部收起。

AccordionExpansion::Multiple(BTreeSet<AccordionId>) 允许多项同时展开；激活某项只切换该项，保留其他项。两种模式均允许零项展开；切换模式由应用显式替换 expansion。

标题发出 AccordionChanged { item, expansion }，包含操作项及完整的请求状态，ActionId 使用组的 action。应用读取动作、确认新的 expansion 后重建；也可以拒绝请求。控件不先行修改状态。该状态是基于生成标题时的模型快照，异步业务应自行校验或合并请求。

所有重复 ID 与未知展开 ID 在调用任何内容工厂前返回 AccordionError。合法时按 items 显示顺序，仅为展开项调用一次 FnMut 内容工厂。禁用项可以保持展开；组 disabled 或项 disabled 仅禁止标题操作，不禁用其内容。

## 布局与生命周期

layout 配置整个组，item_layout 配置每个面板，header_layout 与 panel_layout 配置标题和内容包装；Column/Leaf 类型由组件固定。header_style/text_style 统一设置标题外观；key 保持组在父节点中的身份。

收起或移除项会卸载相应内容，焦点在被删除内容中时由核心清理。重排保留稳定项、标题和仍展开的内容身份。业务数据放在应用模型，编辑状态可使用 TextInputDraft。

每个可用标题是一个 Tab 停靠点，展开内容按树顺序参与遍历；Space/Enter 激活标题。组状态变化使标题待发送请求改变时，会取消其未完成的按压，避免发送过期状态。

标题获得焦点时，ArrowDown/ArrowUp 在可用标题间按显示顺序循环，Home/End 定位首尾可用标题；只移动焦点，不修改展开状态，不调用内容工厂。忽略按键释放和 Ctrl/Alt/Super 组合，允许重复按键。内容区按键不参与标题导航；Tab/Shift+Tab 仍按普通树顺序进入内容。导航会取消尚未完成的标题激活手势。

每次描述的标题共享可用顺序和节点登记表，随挂载/更新/卸载维护。重排保留节点身份并更新导航顺序，删除项移除登记；没有目标时不发焦点请求。核心通过 FocusRequest::Target 校验实际节点资格，复用焦点通知与自动滚入。标题列表查找为 O(N)，登记表为 O(N) 存储，不向核心增加 Accordion 特例。

本实现适合设置面板等有限项数的分组。所有标题共享一份不可变展开状态快照，仅在激活标题时生成完整请求集合；状态存储为 O(N + M)，其中 N 为标题数、M 为展开项数。多项模式每次激活复制集合的成本为 O(M)；普通重建比较快照内容仍可能需要 O(N × M) 时间。所有标题均构建，不用于海量虚拟树。

## 验证与后续

三项集成测试覆盖单项/多项模型回写、收起、替换展开、重排后内容/焦点身份保留、非法 ID 提前拒绝及禁用展开项的内容工厂顺序；同时运行三项 Disclosure 回归。另有两项单元测试验证多个请求共享快照、激活结果不修改原快照，以及相同状态的新快照不会误判为变化；集成测试验证按住 Space 重排后仍能正常释放激活。

导航测试另外覆盖禁用跳过、首尾循环、Home/End、修饰键/释放排除、内容区隔离、重排/删除登记更新与 Space 手势取消。

当前未实现至少保留一项展开策略、展开动画、收起子树保留与无障碍组/展开语义。本阶段复用已有绘制，没有进行新的 GPU 或原生设备验收。

## 渲染预览

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/accordion-cpu.png accordion
```

640×360 预览并排展示单项与多项展开、禁用标题，以及内容区文字和进度条。该入口只生成静态图片，不处理交互。最终 CPU 图片已目视检查，示例间距调整后两列完整显示。

CPU 像素测试在 1×/2× 下依次切换多项展开、单项展开和全部收起，检查各内容面板的实际像素面积、位置及收起后像素消失，包含禁用项保持展开的情况。没有新增绘制原语，本阶段未重跑 GPU。
