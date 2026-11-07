# ScrollView 单轴与双轴滚动

`ScrollView::vertical(content)`、`horizontal(content)` 和 `both(content)` 接收一个普通 View，使用实际布局结果计算启用轴的滚动范围。内容可以是 Column、Grid、文字或自定义组合控件，不需要预先给出内容高度。三个入口共享 Scroll 持久状态、滚动条和区域滚入实现；触摸惯性和虚拟化仍在后续范围内。

## 构造与布局

```rust
use zui_api::{action::ActionId, layout::{Dimension, Edges}};
use zui_widgets::{ScrollView, column};

let content = column(items).build(); // items 是应用创建的子 View 集合。
let list = ScrollView::vertical(content)
    .keyed("devices")
    .on_scroll(ActionId(10))
    .line_step(40.0)
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Length(240.0);
        s.padding = Edges::all(12.0);
    })
    .build();
```

视口在需要滚动的轴上必须由自身或父级约束得到有限尺寸。自动尺寸且没有外部限制时，容器可以随内容增大，因此不会凭空出现滚动范围。上例固定的是视口高度，内容高度仍由布局计算。

构造器内部使用不收缩的单列弹性网格包装内容，在实际列宽确定后测量高度，不覆写调用方内容 View 的样式。纵向模式的宽度等于视口内容宽度、高度自动，文字按视口宽度换行；横向/双向模式的内容宽度使用 MaxContent，自然宽度可以超过视口，横向模式的高度约束到视口，双向模式高度自动。应用显式设置的内容宽高与换行规则仍然有效。

`style` 可设置尺寸、min/max、padding、margin 及参与父级 Flex 的属性。外层构造器拥有 Column、Start 对齐、不换行、零 gap、无轨道以及溢出策略，build 时统一设置，避免与滚动语义冲突。每层动态内容仍应提供稳定 Key。

## 范围与状态

公共 `LayoutBox::scroll_extent` 表达从本地边框盒原点出发可到达的内容范围，包含 padding，至少等于边框盒尺寸。布局适配器从 Taffy 的溢出结果转换为 ZUI 的 Size；核心验证数值并通过 PrepareContext.layout 传给控件。其他布局实现可以用 `LayoutBox::new(bounds)` 表示没有已知溢出的盒子，也可以填写真实范围。负向起始侧溢出和呈现阶段平移不计入可滚动范围。

`Overflow::Visible/Clip/Scroll` 是布局策略，控制后代溢出贡献及自动最小尺寸，不自动实施绘制裁剪。ScrollView 在启用的轴上使用 Scroll 策略，让视口可以小于内容；通过 ChildViewport 单独负责内容平移和裁剪。子滚动容器的内部溢出不会被重复算入父滚动容器。

每个启用轴的最大偏移为该轴的 `scroll_extent - bounds.size`，未启用轴为零。持久控件 Scroll 用 Point 保存二维偏移，内容缩短或视口变大后自动夹到有效范围；普通 View 重建保留偏移。滚动本身只请求 Paint，不重新执行布局。裁剪固定在 content_bounds，padding 区域不随内容滚动。

`scroll_to(Point::new(x, y), revision)` 是一次显式跳转请求，两轴数值必须有限且非负，超出范围时夹到末尾，未启用轴归零。offset 或 revision 变化会重新应用请求；重建时保持二者不变，不会覆盖用户随后进行的滚动。需要重复跳到同一位置时增加 revision。移除请求只恢复普通状态保留语义，不自动归零。

`on_scroll` 可接收用户交互产生的 `ScrollChanged { offset: Point, max_offset: Point }`，由应用解释。程序跳转和布局后的范围收缩不发送交互动作。核心只搬运不透明动作载荷。

## 输入与嵌套

- 逻辑像素滚轮直接使用平台转换后的数值；行滚轮乘以 line_step，默认每行 40 逻辑像素。两轴均为正增量减少偏移、负增量增加偏移；未启用轴不消费。不将 y 滚轮隐式改为横向滚动，以便嵌套纵向容器继续处理；Shift+滚轮映射尚未提供。
- 滚轮在目标或冒泡阶段处理，捕获阶段不抢占子容器。有实际位移才停止传播；已在该方向边界时继续冒泡。
- 单次滚轮超出边界时，本次在子容器夹到边界并消费，剩余距离暂不在同次事件中分配给父级；下一次同方向事件可交给父级。
- 容器可以通过 Tab 或点击自身空白/padding 区域获得焦点。自身作为键盘目标时，方向键按对应轴处理；PageUp/PageDown 在纯横向模式处理 x，其他模式处理 y；Home/End 将全部启用轴移到起点/末尾。有修饰键时不处理。子输入框等控件的键盘目标不被父滚动容器接管。
- `disabled(true)` 停止容器的用户滚动及自身焦点能力，但不递归禁用内容控件，也不阻止显式程序跳转。零内容视口不消费滚轮。

内部偏移、范围和输入规则归 ScrollView 所有；核心只负责通用布局结果、事件路由和子视口坐标。渲染器收到普通绘制命令，不认识滚动控件。

## 验证与当前边界

已验证内容范围、padding、滚轮行/像素单位、Key 状态保留、版本跳转、内容缩短、视口变大、嵌套边界冒泡、键盘、禁用、零视口和非法配置恢复。真实 Parley 字体测试验证文字换行改变高度与范围；CPU/GPU 检查顶部、中间、末尾在 1×/2× 下的裁剪像素。核心另验证不合法的滚动范围不能发布布局。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/scroll-cpu.png scroll
cargo test -p zui-render-check --test scroll --all-features --locked
# 需要真实 GPU。
cargo test -p zui-render-check --test scroll --all-features --locked -- --ignored
```

目前所有内容节点仍会创建并布局，裁剪不等于虚拟化。输入光标/选区自动滚入、滚动惯性、触摸手势、同次滚轮余量分配、无障碍滚动语义及原生设备输入验收尚未完成。现有数据大屏或大型表格不能据此宣称具备完整滚动性能和交互能力。


## 前景滚动条

默认在内容区域右侧/底部按启用轴显示覆盖式滚动条，仅当有可滚动范围且视口非空时显示。它不占用布局宽度，因此显示与隐藏不会使文字反复换行；右侧/底部内容会被滚动条覆盖，应用可在内容内部留出空白。`.scrollbar(None)` 隐藏滚动条，但保留滚轮、键盘和程序跳转。

`ScrollbarStyle` 配置宽度、最小滑块长度及轨道、滑块、拖动和禁用颜色。宽度与最小长度必须有限且大于零；实际尺寸不超过视口。滑块长度按可见内容比例计算，并受最小长度约束。轨道太短而没有可移动空间时不启动拖动，仍可使用滚轮和键盘。

按下滑块时获取焦点和指针捕获，按下点在滑块内的比例用于保持拖动锚点；指针移到视口外仍可拖动，松开会应用最终位置并释放捕获。拖动期间视口改变时使用新的滑块几何。点击滑块前后轨道每次移动一页，暂未实现长按连续翻页、自动隐藏或悬停动画。

窗口取消输入或失去捕获会终止拖动；禁用控件由核心通用规则清除捕获。拖动时隐藏滚动条或内容不再可滚动，下一次指针移动/松开会结束该手势并释放捕获；节点移除也沿用核心清理机制。普通样式更新不重置偏移。

绘制顺序为容器背景、子内容、前景滚动条和焦点框。命中顺序相反：滚动条区域先于子控件，其他位置继续命中内容。`Widget::paint_foreground` 和 `hit_test_foreground` 是通用公共能力，核心不引用 ScrollView 或 ScrollbarStyle。前景不受该节点自身的子内容平移/裁剪影响，仍受祖先裁剪与节点裁剪约束；只绘制的装饰应保持前景命中为 false。

新增测试覆盖覆盖区域命中优先级、轨道翻页、越界拖动、松开、缩放期间锚点、取消、禁用、隐藏、短轨道、非法样式恢复，以及 CPU/GPU 在三种位置的 1×/2× 前景像素。原有不显示滚动条的像素测试显式使用 scrollbar(None)，继续验证纯内容滚动。

焦点切换后的控件边框盒滚入和显式区域请求已接入，采用最近边界并从内到外处理嵌套容器；不会在普通重建时反复覆盖用户滚动，详见 [焦点与区域自动滚入](reveal.md)。


## 二维状态与接口更新

```rust
let matrix = ScrollView::both(content)
    .scroll_to(zui_types::Point::new(100.0, 50.0), 1)
    .style(|s| {
        s.width = Dimension::Length(600.0);
        s.height = Dimension::Length(300.0);
    })
    .build();
```

当前开发版将 VerticalScroll 更名为 Scroll，offset()/max_offset() 和 ScrollChanged 的对应字段改为 Point，scroll_to 的标量参数改为 Point。旧纵向调用迁移为 Point::new(0.0, y)，读取 offset().y。本次以统一二维契约替代旧的纵向标量接口，调用方需要同步迁移。

三个构造入口使用同一 Widget 类型，稳定 Key 下切换轴向保留仍有效的偏移；新禁用轴的偏移归零，内容重新布局并按新范围夹取。两条滚动条同时存在时分别缩短轨道，右下角保留为普通内容区域，避免两个拖动命中重叠。拖动只修改所抓取轴，二维滚轮、Home/End 与显式跳转可以同时修改两轴。

焦点和区域滚入对所有启用轴使用最近边界，继续复用核心从内到外的通用契约。双向模式并不自动虚拟化或冻结表头；大型表格和数据画布的专项能力仍需后续实现。

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/scroll-both-cpu.png scroll-both
cargo test -p zui-render-check --test scroll_axes --all-features --locked
```
