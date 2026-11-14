# 复合控件、菜单与弹层

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [现有控件完善与 P0 控件](#p0-controls)
- [P1 控件](#p1-controls)
- [P1 / P2 控件清单与使用边界](#controls-catalog)
- [已有控件增强：进展与边界](#advanced-controls)
- [菜单悬停容错](#menu-hover)
- [弹窗标题与系统窗口装饰](#dialog-window-chrome)

<a id="p0-controls"></a>


<a id="p0-controls--现有控件完善与-p0-控件"></a>

## 现有控件完善与 P0 控件

这一批保持三层职责：控件表达交互与绘制；core 处理通用事件、焦点和节点生命周期；应用持有业务状态、弹出状态及消息映射。所有新增控件仍在 no_std + alloc 的 zui-widgets，未引入窗口或渲染器依赖。


<a id="p0-controls--现有控件"></a>

### 现有控件


<a id="p0-controls--textinput"></a>

#### TextInput

- `placeholder`：空内容且没有输入法预编辑时显示提示，不进入实际文本、选择或提交值。
- `invalid`：应用提供的校验状态，使用主题 Danger 对应的 `style.invalid_border`；不自动拒绝输入。
- `read_only`：保留焦点、选择和复制，阻止修改。
- `password`：每个 Unicode 字素显示一个圆点，中文、组合字符、emoji 序列保持正确的光标、选择和输入法几何。复制和剪切被禁止，粘贴正常。模型、草稿与撤销历史仍保留原文；这是显示/剪贴板策略，不是加密存储或内存清零机制。
- Shift+点击扩展选择，使用 core 在每次输入上提供的修饰键快照。


<a id="p0-controls--listbox"></a>

#### ListBox

原有单选行为保持默认。`activation_only=true` 时，方向键、分页和字符定位只移动活动项，点击、Space 或 Enter 才发出 `ListSelectionChanged`；选中当前项也可确认，用于菜单和下拉框。

`multi_selection=Some(BTreeSet<ListItemId>)` 开启受控多选，返回 `ListMultiSelectionChanged`：普通点击/导航替换选择，Ctrl/Command 点击或 Space 切换单项，Shift 扩展范围，Ctrl/Command+Shift 合并范围，Ctrl/Command+A 选择所有可用项，Ctrl/Command+方向键仅移动活动项。范围跳过禁用项。应用收到动作后更新集合；控件不提前修改业务选择。多选与 activation_only 不能同时开启。

VirtualList 仍负责可见行实现与定位，行级业务选择由应用持有；没有把 ListBox 的全量行存储移入虚拟列表。


<a id="p0-controls--popup-与焦点范围"></a>

### Popup 与焦点范围

`PopupView` 支持居中、根坐标锚点和根坐标点定位。锚点优先向下展开，底部不足而上方足够时翻转；最终位置限制在视口内，过大内容受视口约束和裁剪。

应用在视口大小的 `stack` 中，先放普通页面，再放 Popup：

```rust
let popup = PopupView::new(
    PopupPlacement::Anchor(anchor_in_root),
    Size::new(240.0, 180.0),
    dismiss_action,
).build(content);
let root = stack([page.keyed("page"), popup.keyed("popup")])
    .style(|s| {
        s.width = Dimension::Percent(1.0);
        s.height = Dimension::Percent(1.0);
    })
    .build();
```

锚点由应用以根局部逻辑坐标提供；没有隐式 DOM 查询或自动绑定任意节点的 portal。窗口布局、滚动或触发点变化后，应用需要提供新的锚点。示例的触发区位于固定顶部工具区。

交互 Popup 使用公共 `Widget::focus_scope()` 契约。core 在布局收敛后进入最上层范围，Tab/Shift+Tab 不越界，显式越界焦点请求失败，背景输入不会送给应用快捷键。新增范围取消外部指针捕获，移除后恢复仍存在且可用的旧焦点；嵌套范围按层恢复。初始焦点优先进入可聚焦容器内的叶子控件；没有可聚焦子项时，范围自身作为键盘后备接收者。

Escape 或外部指针按下产生 `PopupDismissed { reason }`，应用决定何时移除描述。外部点击被当前层消费，不穿透激活背景。`close_on_outside=false` 适用于确认弹窗；Escape 仍可关闭。被动 Popup（`dismiss=None`）不占焦点，不自身参与命中，内容应使用非交互控件。


<a id="p0-controls--select--combobox"></a>

### Select / ComboBox

`SelectView::themed` 接收 ListItem、受控选中 ID、字体和 ActionId；`build()` 返回触发按钮，`popup(anchor,height,dismiss)` 返回应挂载到根层的列表。

`SelectEvent::Open` 请求展开，`Selected(id)` 提交选择。方向键、Home/End、PageUp/PageDown 和字符定位移动活动项，跳过禁用项，Enter/Space 或点击提交；Escape 关闭而不改变业务选中项。应用确认后移除弹出层并恢复触发按钮焦点。

`SelectView` 为非编辑选择；`ComboBoxView` 已独立实现搜索、自由文本和多选下拉，使用独立 ComboBoxEvent，见 [控件增强](advanced-widgets.md#advanced-controls)。


<a id="p0-controls--menu--menubar--contextmenu"></a>

### Menu / MenuBar / ContextMenu

- `MenuView`：菜单项、禁用态、勾选标记、快捷键文字以及子菜单。
- `MenuBarView`：顶层按钮发出 `MenuEvent::Open(id)`，应用选择对应菜单描述。
- `ContextMenuRegion`：包装内容，右键发出区域局部位置；放在根层时可直接作为根坐标。Shift+F10 也可请求菜单。
- `MenuEvent::OpenPath(path)` 请求打开子菜单；Right 打开，Left 返回，Enter/Space 执行。应用持有 open_path；所有层处于同一焦点作用域，内部复用列表行为，父层仍可接收鼠标输入。路径上每层验证 ID，最多 16 层。
- `MenuEvent::Selected(id)` 表示执行命令；关闭和业务执行由应用处理。
- `MenuShortcut` 与 `resolve_shortcut()` 解析启用命令，忽略按键重复与禁用分支。应用决定快捷键相对文本编辑器和模态窗口的优先级；不在底层注册全局快捷键。

菜单支持跨层悬停切换，不包含原生系统菜单。`MenuView.behavior` 默认悬停延迟 180ms、朝向已打开子菜单时容错 350ms、键盘操作后鼠标抖动阈值 4 个逻辑像素。详细规则见 [菜单悬停容错](advanced-widgets.md#menu-hover)。点击与键盘打开不等待悬停延迟。


<a id="p0-controls--dialog--messagebox"></a>

### Dialog / MessageBox

`DialogView` 组合标题、调用方提供的主体和底部内容，默认居中、遮罩、禁止外部点击关闭。`MessageBoxView` 提供消息文字和结果按钮，可配置 Accept/Cancel/Yes/No 等 `DialogResult`。

内容区提供滚动，视口缩小时仍可通过滚动与焦点揭示到达结果按钮。确认和取消发出动作，应用决定执行或保持业务状态。焦点范围、嵌套与关闭恢复复用 Popup；不存在 core 对 Dialog 类型的特殊判断。示例确认后把数字输入、Slider、Stepper、进度条和 Canvas 一起恢复为 50%。


<a id="p0-controls--tooltip"></a>

### Tooltip

`TooltipView` 是根级提示宿主：接收纯内容工厂和根坐标 `TooltipAnchor` 列表，保留稳定内容节点。默认悬停 500 ms 后出现，移出、点击、滚动或 Escape 隐藏；静止时按期限唤醒，不持续刷新。Tooltip 绘制在普通内容后，不占焦点、不拦截指针。

`focused` 可由应用显式指定键盘提示索引；尚不自动查找任意后代焦点对应的提示。内容工厂可能重复调用，不能在其中修改模型或执行 I/O。core 对祖先发送成对的进入/离开通知，避免鼠标离开整个内容区后遗留提示。


<a id="p0-controls--numberinput"></a>

### NumberInput

`NumberInputView` 复用 TextInput 的选择、输入法、剪贴板、撤销和绘制能力。应用提供有限 `value`、`NumericRange`、显示精度（0～12）。单位可用普通 row + Text 组合，不混入数值解析。

- 编辑中的空字符串、负号和小数点保留在本地，不立即写入业务数值。
- Enter 解析并检查范围：成功发出 `NumberChanged`，失败发出 `NumberRejected` 并显示错误边框；不静默截断或钳制非法输入。
- Up/Down 按步长提出新值，Shift 为十步；外部值仍由应用确认。
- Escape 取消编辑，失去焦点也恢复已确认值；没有待取消内容时，Escape 继续交给外层弹窗。本版不在失焦时自动提交。输入法预编辑期间，候选键优先交给输入法。
- 外部值、范围或精度改变会更新本地编辑描述，避免保留过时的编辑内容。


<a id="p0-controls--示例与验证入口"></a>

### 示例与验证入口

运行 `cargo run -p widget-gallery -- cpu`。顶部展示下拉选择、菜单/右键菜单、确认弹窗、数字输入、密码与只读切换；下方展示列表多选及原有控件。GPU 使用 `--no-default-features --features gpu -- gpu`。

`tools/render-check/tests/p0.rs` 覆盖焦点范围/恢复、防穿透、边界定位、选择确认、子菜单、快捷键、数值编辑、密码字素映射和剪贴板策略、多选范围、Tooltip 时序、占位提示，以及共用 CPU/GPU 像素用例。真实 GPU 测试需要显式 `--ignored`。完整无障碍平台适配、触摸手势和真实嵌入式设备验收不属于本批完成范围。


<a id="p0-controls--矢量图标"></a>

#### 矢量图标

Select 的下拉箭头及 Menu 的勾选/子菜单箭头已改用统一矢量图标，不再依赖字体符号覆盖。Button 和 ListItem 提供独立图标槽位；自定义路径与主题配置见 [图标说明](appearance.md#icons)。

<a id="p1-controls"></a>


<a id="p1-controls--p1-控件"></a>

## P1 控件

本轮范围：TreeView、TableView、SplitterView、TextAreaView。实现均位于 zui-widgets，只依赖既有下层公共接口；没有新增库依赖，core 和渲染后端不识别控件类型。


<a id="p1-controls--treeview"></a>

### TreeView

TreeItem 使用稳定 TreeItemId，支持层级、禁用节点和子节点；expanded 与 selected 由应用维护。禁用节点的后代也不可选择。构建检查全树 ID 唯一性、选中/展开 ID 是否存在，以及最大 64 层深度，隐藏分支也参与检查。

- 点击标签选择；点击矢量箭头展开/收起。
- 上下、Home/End、PageUp/PageDown 和字符导航复用 ListBox。
- 右方向键展开分支或进入已展开分支的可用子节点；左方向键收起分支或选择父节点；Enter/Space 切换分支展开状态。
- TreeEvent::Selected / Expansion 只是请求，应用更新 selected/expanded 后重建。

折叠隐藏当前选中项时，业务选择仍可保留；可见列表不高亮隐藏项。若应用希望折叠后选中父节点，应同时更新 selected。当前已补充懒加载请求、多选、普通树拖放请求和虚拟树入口；虚拟拖放等组合能力尚未完成，见 [控件增强](advanced-widgets.md#advanced-controls)。


<a id="p1-controls--tableview"></a>

### TableView

TableColumn 定义稳定列 ID、标题、逻辑宽度及 sortable；TableRow 定义稳定行 ID、文本单元格和 disabled。构建检查重复 ID、列宽（至少 32）、单元格数量、选中项及排序状态。

- 点击行或在该行获得焦点后按 Enter/Space，发出 TableEvent::Selected。
- Tab 在可交互表头/行间切换；禁用行不获取焦点。
- 点击可排序表头发出 SortRequested(TableSort)，同列升降序交替，矢量箭头显示已确认状态。
- 排序与选中由应用执行；框架不猜测字符串、数字、日期的排序规则。
- 固定列宽、单元格换行、按行自适应高度、水平和垂直滚动。

当前是只读文本表格，表头随内容一起滚动。暂不包含冻结表头/列、列拖拽与调整宽度、单元格编辑、范围多选、方向键跨行焦点管理及虚拟化；不将其表述为完整 DataGrid。


<a id="p1-controls--splitterview"></a>

### SplitterView

`SplitterView::themed(theme, ratio, axis, action).build(first, second)` 返回两栏布局。Horizontal 表示左右分栏，Vertical 表示上下分栏。外层尺寸通过返回 View 的 style 设置，保留构造器生成的 Grid 结构。

分隔条默认 8 逻辑像素、比例范围 0.1–0.9；支持拖动、指针捕获、越界比例钳制，以及焦点状态。键盘沿分栏方向每次调整 1%，Shift 调整 10%，Home/End 到最小/最大比例。Escape 结束拖动，已经被应用确认的比例不回滚。SplitterChanged 请求由应用确认，两个面板使用稳定 key 保留状态。

最小值按比例配置，不是面板像素最小尺寸。面板内容超出时裁剪，需滚动时由面板内 ScrollView 承担。当前不包含多级拖放停靠、分栏吸附、自动持久化或鼠标指针形状适配。


<a id="p1-controls--textareaview"></a>

### TextAreaView

复用 TextInput 的 TextBuffer、Unicode 字素选择、撤销/重做、剪贴板、IME、草稿交接和焦点机制，不复制编辑引擎。`editor` 提供文本、action、只读/禁用、校验状态、占位文本、字体、主题和预算配置；TextArea 强制多行且关闭密码模式。

- 默认自动换行（wrap=true），可关闭；visible_lines 默认 4，仅决定自然高度。
- Enter 插入换行，Tab 保留焦点遍历；通过 TextInputEvent::Changed 报告文本，不再以普通 Enter 提交。
- 上下方向键按视觉行移动并保留目标横向位置；Home/End 到视觉行首/尾，Ctrl/Command+Home/End 到全文首/尾；Shift 扩展选区。
- 指针命中使用二维文字几何，光标自动滚动到可见区域；滚轮移动视口，边界未消费的滚动可交给外层。
- 输入法组合期间，候选操作不会误插入换行。多行粘贴和 IME 提交进入同一撤销历史。
- TextInputDraft 保留已提交内容/选择/历史，区分单行与多行模式；不会保存进行中的输入法组合。

TextArea 是纯文本编辑器，当前无行号、语法高亮、富文本、可拖动内部滚动条或超大文档分块布局。仍按既有 EditLimits 限制文本和历史内存，不宣称大文件编辑器性能。


<a id="p1-controls--展厅与验证"></a>

### 展厅与验证

```sh
cargo run --release -p widget-gallery -- cpu
```

展厅滚动内容顶部展示 P1：树和表格共享设备选择；表格标题可排序；分栏拖动调整两侧宽度；下方多行巡检记录实时更新字数。

专项测试位于 tools/render-check/tests/p1.rs，覆盖树展开/选择/校验、表格选择与排序请求、分栏拖动/键盘/非法比例、TextArea 换行/导航/撤销/只读/IME/多行粘贴/滚动/草稿重挂载，以及 CPU/GPU 分隔条像素。既有 P0、ListBox 和单行编辑器测试继续回归。完整无障碍平台映射、触摸及 MCU 实机验收不在本轮交付范围。


<a id="p1-controls--补充完整清单中的其余-p1-与-p2"></a>

### 补充：完整清单中的其余 P1 与 P2

新增 Form/FormField、GroupBox/Separator，以及日期时间、工具栏/状态栏、通知、面包屑/分页、文件和颜色选择器。另新增 DataGridView，在保持 TableView 只读接口的同时提供列宽拖动、单元格编辑与校验。以上接口、状态职责和限制见 [完整控件清单](advanced-widgets.md#controls-catalog)。

<a id="controls-catalog"></a>


<a id="controls-catalog--p1--p2-控件清单与使用边界"></a>

## P1 / P2 控件清单与使用边界

基础控件的状态优先级、圆角、焦点环和分段共享边线见 [公共外观层](appearance.md#widget-appearance)。

本清单对应传统应用控件的补齐范围。新增实现集中在 zui-widgets 的独立模块，复用既有输入、布局、主题、滚动和绘制接口，不引入运行时、文件系统或具体后端依赖。应用持有业务状态，通过 ActionEvent 接收请求并重建视图。

| 优先级 | 控件 / 公共接口 | 当前能力 |
| --- | --- | --- |
| P1 | TreeView | 层级、展开、选择、禁用、滚动、键盘导航 |
| P1 | TableView / DataGridView | 只读表格 / 可编辑文本单元格、列宽拖动与键盘调整、排序请求、行选择、校验错误、双轴滚动 |
| P1 | TextAreaView | 多行、可选换行、滚动、选择、撤销、IME、剪贴板 |
| P1 | SplitterView / SplitPaneView | 左右/上下分栏、比例限制、拖动和键盘调整；SplitPaneView 为同一实现的别名 |
| P1 | FormView / FormField | 标签、必填标记、帮助文字、错误信息、统一标签宽度或上下排列 |
| P1 | GroupBoxView / SeparatorView | 标题分组与主题边框，横向/纵向分隔线 |
| P2 | CalendarView / DatePickerView / DateRangePickerView | 公历月份、闰年、日期选择、最小/最大日期、弹层、起止日期组合 |
| P2 | TimePickerView | 24 小时 HH:MM:SS 输入、提交校验、撤销本次草稿 |
| P2 | ToolbarView / StatusBarView | 可换行工具栏，左右状态内容；可组合现有按钮、图标、菜单等 View |
| P2 | NotificationView / ToastView | 四类语义状态、关闭请求、非模态展示、可配置自动隐藏计时 |
| P2 | BreadcrumbView / PaginationView | 层级导航请求、有界页码窗口、首页/末页/前后页、禁用边界 |
| P2 | FilePickerView | 打开/保存/选择目录三种模式、目录列表、扩展名和隐藏文件过滤、选择及进入目录请求 |
| P2 | ColorPickerView | 预设色、RGBA 四通道、十六进制颜色输入、透明度棋盘预览 |


<a id="controls-catalog--datagrid编辑和列宽"></a>

### DataGrid：编辑和列宽

DataGridView 包含一个 TableView，复用列、行、选择、排序和主题数据；editable_columns 指定可编辑列，cell_errors 提供按 CellAddress 定位的应用校验信息，edit_revision 用于主动重置编辑草稿。

- 可编辑单元格直接显示 TextInput；输入时保留本地草稿，Enter 发出 `CellEdited { cell, value }`，不直接修改原始行。
- 应用接受后更新 rows；拒绝则保留原值并设置 cell_errors。错误文字和输入边框同时显示。
- Escape 恢复原始值并发出 EditCancelled，应用可清除该格的错误。失焦不会提交或丢弃 DataGrid 草稿。
- 鼠标点击编辑格会请求选中所在行；Tab 在表头、列宽手柄和单元格间遍历。排序请求由应用按真实数据类型执行。
- 拖动表头右侧手柄请求 ColumnResized；聚焦手柄后左右键调整 8 像素，Shift 为 32 像素，Home/End 到 32/4096 像素。范围为逻辑尺寸；应用更新列宽后继续拖动不会累积坐标误差。

现已提供固定表头、虚拟行及自定义单元格/编辑器入口；冻结列、列拖放重排、范围选择和公式尚未实现。详见 [控件增强](advanced-widgets.md#advanced-controls)。TableView 保持轻量只读用途，DataGridView 提供上述编辑能力，不把两者混同为电子表格。


<a id="controls-catalog--表单和页面组合"></a>

### 表单和页面组合

```rust
let mut field = FormField::new("device-name", "设备名称", editor_view);
field.required = true;
field.error = validation_error;
let mut form = FormView::themed(&theme, families.clone());
form.layout = FormLayout::Inline { label_width: 120.0 };
let view = GroupBoxView::themed(&theme, "设备设置", families)
    .build(form.build(vec![field]));
```

required 只提供标记，不检查未知子控件的业务值；校验与提交由应用负责。FormField 可容纳任意 View，不依赖具体编辑器。应用应同时设置子编辑器的 invalid 状态；FormField 自身负责标签、帮助和错误展示。窄窗口可选择 Stacked，框架不会猜测应用的响应式断点。字段 key 必须在同一表单中唯一。

Toolbar 接收任意控件 View；StatusBar 接收左、右两个 View。它们不注册全局快捷键、不访问系统窗口，不引入平台菜单行为。


<a id="controls-catalog--日期和时间"></a>

### 日期和时间

Date 是 1–9999 年的公历日期，不包含时区、系统当前日期或本地化规则。应用显式提供显示月份、选中日期和范围。Calendar 的上月/下月、日期选择均返回 DateEvent；应用确认月份或日期后更新描述。日期格通过 Tab/Enter 操作，尚未实现专用的方向键日历焦点导航。

DatePicker 的触发器返回 Open，`popup(anchor, dismiss)` 复用通用 Popup，锚点为根窗口坐标；应用也可将 Calendar 放入居中弹层。DateRangePicker 组合两个独立 ActionId 的日期触发器并检查起止顺序；跨字段 min/max 约束由应用设置。TimePicker 使用严格的 `HH:MM:SS` 格式（00:00:00–23:59:59），Enter 确认，非法输入保留草稿并标记错误，Escape/失焦恢复已确认值。它不提供时钟表盘、时区换算或闰秒。


<a id="controls-catalog--通知和导航"></a>

### 通知和导航

NotificationView 展示 Info/Success/Warning/Error，设置 dismiss 后返回带通知 ID 的关闭请求。ToastView 默认 3 秒自动隐藏，由已注入的单调 elapsed 时间驱动，不读取系统时间、不创建后台线程、不建立模态焦点作用域。相同 id/revision 的普通重建不重启计时；新 id 或增加 revision 可重新显示。自动隐藏后保留空宿主，应用拥有通知队列和最终移除职责；本轮不提供系统通知服务或全局通知队列。

Breadcrumb 最后一项表示当前位置，不可再次激活；ID 必须唯一。Pagination 使用 1 起始页码，page_count 必须大于零，最多展示五个数字页，另有首页/前页/后页/末页；即使总页数很大，也不会为每一页创建按钮。应用负责翻页查询、加载状态和数据总数。


<a id="controls-catalog--文件选择控件与-io-分离"></a>

### 文件选择：控件与 I/O 分离

FilePickerView 的 entries、directory、selected、filename、extensions 等均由应用提供。稳定 FileEntryId 是应用目录记录的标识，不是运行时节点 ID。

- 单击或键盘导航请求 Selected；列表 Enter 或“打开”按钮请求 EnterDirectory/OpenFile。
- “上级目录”请求 Parent；应用读取新目录、清除旧选择并更新 entries。can_go_parent 可限制到应用根目录。
- 保存模式的文件名编辑使用 filename_action（TextInputEvent）；应用更新 filename 后，“保存”按钮发出 SaveFile 请求。
- 选择目录模式确认当前 directory；文件行不可选择。文件名不能为空、不能为 `.`/`..`，不能含路径分隔符或控制字符。
- 扩展名过滤与基本文件名校验只服务界面体验，不替代应用的路径规范化、权限检查、平台文件名规则和覆盖确认。

该组件不是操作系统原生文件对话框，不枚举磁盘、不打开/写入文件。真实桌面或嵌入式应用可注入本地、远程或虚拟文件源；平台适配属于上层应用/独立 I/O 适配模块。展厅使用明确标注的演示目录，确认只更新状态信息，不产生文件写入。


<a id="controls-catalog--颜色选择"></a>

### 颜色选择

ColorPicker 用 RGBA8 表示颜色。十六进制字段接受 `#RRGGBB` 或 `#RRGGBBAA`（可省略 #），通道字段只接受 0–255 整数；Enter 返回 ColorChanged，Escape/失焦撤销本次未确认输入。预设色点击直接请求变化。透明度用矢量棋盘底显示，复用 CPU/GPU 绘制，不依赖位图或图标字体。当前没有 HSV 色轮、屏幕取色、ICC 色彩管理或渐变编辑器。


<a id="controls-catalog--展厅和验证"></a>

### 展厅和验证

```sh
cargo run --release -p widget-gallery -- cpu
```

滚动内容顶部展示本轮补充控件：必填表单校验、日期范围弹层、时间输入、DataGrid 数值校验与分页排序、文件模式与面包屑联动、颜色输入、自动隐藏通知和状态栏。

专项测试入口为 `tools/render-check/tests/catalog.rs`，展厅联动/宽窄布局测试位于 `examples/widget-gallery/src/app/catalog.rs`。完整无障碍平台映射、触摸、真实 MCU 显示设备、系统文件对话框和百万行数据性能均不在当前已验收范围。


<a id="controls-catalog--已有控件增强进展"></a>

### 已有控件增强进展

搜索多选、树加载/多选/虚拟化、虚拟表格、页签操作、菜单分隔与 HSV 的当前能力及剩余缺项见 [控件增强](advanced-widgets.md#advanced-controls)。增强任务尚未全部完成。

<a id="advanced-controls"></a>


<a id="advanced-controls--已有控件增强进展与边界"></a>

## 已有控件增强：进展与边界

本页记录“补齐已有控件缺项”的实际进度，整个增强任务尚未全部完成。没有把请求事件、扩展插槽或模拟数据等同于原生平台实现。

| 控件 | 本批新增 | 仍未完成 |
| --- | --- | --- |
| ComboBox | 独立控件、搜索、自由文本提交、多选，过滤后保留隐藏选项的选择 | 输入框方向键直接切入候选列表；目前通过 Tab 切换 |
| TreeView | 懒加载状态/请求、多选、普通树节点拖放请求、固定行高虚拟树 | 虚拟树拖放与自动滚动，虚拟模式 Shift 鼠标跨行范围选择 |
| DataGrid | 测量行高的虚拟行、固定表头、自定义单元格/编辑器、草稿句柄 | 冻结列、列拖动重排、单元格范围选择、内置跨未实现行的焦点导航 |
| Tabs | 关闭、拖动重排、键盘关闭/重排、拖出请求 | 宿主创建独立窗口和页面迁移 |
| 菜单 | 分隔项、跨层悬停切换、延迟与朝向子菜单的容错 | 系统原生菜单 |
| ColorPicker | HSV 平面/色相条、鼠标拖动、键盘调整 | 系统屏幕吸管 |
| TextArea | 本批未扩展 | 语法高亮、大文档分块 |
| FilePicker | 本批未扩展 | 系统原生文件对话框适配 |


<a id="advanced-controls--combobox"></a>

### ComboBox

`SelectView` 保持原来的非编辑选择行为。`ComboBoxView` 不再是它的类型别名，使用独立 `ComboBoxEvent`：Open、QueryChanged、Submitted、Selected、SelectionChanged。

`query`、`selection` 和弹层是否打开均由应用控制。默认支持不区分大小写的子串搜索；`allow_custom=true` 允许 Enter 提交自由文本；`selection=Some(BTreeSet)` 启用多选。应用接收 QueryChanged 后更新 query，再重建弹层。多选后通常保持弹层打开，单选或提交后可关闭。

搜索框复用完整 TextInput，包括 IME、剪贴板、撤销与光标。Tab 进入候选列表，列表沿用键盘多选操作。过滤不会删除隐藏选项的已选状态；全选作用于当前匹配项。远程搜索由应用提供过滤后的条目。


<a id="advanced-controls--treeview"></a>

### TreeView

`TreeItem.children_state` 为 Loaded、Unloaded、Loading 或 Failed。非 Loaded 的空节点仍有展开箭头；展开 Unloaded/Failed 节点同时发出 Expansion 和 TreeLoadRequested。应用应立即设为 Loading，再提交异步结果或 Failed，避免重复请求。控件本身不创建线程、不读取目录。

树节点单击文字选中，双击文字区域展开或折叠分支（默认间隔 500 毫秒，可通过 `double_click_interval` 设置平台偏好，零值禁用；位置偏差小于 6 个逻辑像素）；箭头仍为单击切换。叶节点和禁用节点不响应展开。拖动、滚动、键盘操作或失焦会取消待判定的双击，展开懒加载节点仍发送 TreeLoadRequested。

`selection=Some(BTreeSet<TreeItemId>)` 启用多选，接收 TreeSelectionChanged；折叠隐藏的选项仍保留。`selected` 表示主选中/导航项。`draggable=true` 的普通树在拖动距离超过 6 个逻辑像素、释放到另一个可用节点时发出 TreeDropRequested。当前放置语义为“移入目标节点”；应用负责业务校验、更新结构和拒绝非法移动。控件拒绝将可见源节点移入自身或其后代，Escape 取消拖动。

`build_virtual(row_height, viewport_height)` 复用 VirtualList，只创建可见行。选中项变化后自动显露；Home/End、方向键、PageUp/PageDown 在完整的可见逻辑树上导航。固定行高应足够容纳当前字号和标签，长标签可能裁剪。虚拟树仍保留完整输入数据与展平元数据，不代表数据本身为流式存储。

虚拟模式与 draggable 同时启用会返回 VirtualDragUnsupported，避免无提示地失去拖放能力。普通树保留完整 ListBox 的鼠标 Shift 范围选择；虚拟模式支持键盘 Shift 范围选择与 Ctrl 单项切换，尚无 Shift 鼠标跨行范围选择。


<a id="advanced-controls--datagrid"></a>

### DataGrid

`virtual_rows=Some(DataGridVirtualRows { estimated_height, viewport_height, revision })` 开启自动测量行高的虚拟列表。内容、顺序、字体或校验信息变化时增加 revision。虚拟模式表头固定；普通模式可设置 `freeze_header=true`，并给外层一个明确高度。横向滚动由整个表格共享，表头与正文列对齐。

`cell_factory: Option<DataGridCellFactory>` 接收 CellAddress、当前文本和 disabled，返回任意 View。设置后负责所有单元格内容，可组合日期、数值、下拉等编辑器；自定义控件自行发出应用事件，不强制走文本 CellEdited。默认仍使用原有文本编辑器。

`drafts: BTreeMap<CellAddress, TextInputDraft>` 为需要保留未提交草稿的文本单元格提供稳定句柄。应用拥有句柄和缓存预算；未提供句柄时，被回收行的本地草稿不会保留。普通业务数据仍由应用持有。草稿句柄只能被一个活跃编辑器占用。该机制保留编辑状态，不保存焦点或 IME 预编辑会话。

虚拟模式尚未自动装配跨未创建行的 Tab 导航；当前鼠标滚动可以访问所有行。这一限制与已完成的普通表格键盘操作分开记录。


<a id="advanced-controls--tabs"></a>

### Tabs

`closable` 指定可关闭 TabId，显示矢量关闭按钮；Ctrl/Command+W 请求关闭当前页签。`reorderable=true` 启用拖动排序及 Ctrl/Command+Shift+左右键。应用分别接收 TabCloseRequested、TabReorderRequested 并更新 tabs/selected，关闭当前页后要选择有效替代项或 None。

`detachable=true` 时，拖出页签栏上下边界超过 16 个逻辑像素产生 TabDetachRequested，position 为页签栏局部坐标。该事件不会创建窗口；当前单窗口宿主尚未接入页面迁移。展厅启用关闭和重排，没有将拖出请求伪装成独立窗口。


<a id="advanced-controls--菜单和-hsv"></a>

### 菜单和 HSV

`MenuItem::separator(id)` 创建不可激活的分隔行，不参与快捷键解析。它沿用行高以保持菜单定位一致。各层菜单共用一个焦点作用域，支持跨层悬停切换；延迟、三角形容错和键盘后抖动过滤见[菜单悬停容错](#menu-hover)。

ColorPicker 内置 HsvPickerView。HsvChanged 同时给出 HSV 和 RGBA；ColorChanged 继续兼容已有颜色联动。应用可消费其中一种，或同时处理，但需避免重复业务副作用。将 HsvChanged.hsv 保存到 ColorPickerView.hsv，可在饱和度/亮度为零时保留所选色相；从预设或十六进制设置新颜色时同步或清除该值。

方向键调整饱和度/亮度（Shift 为较大步长），PageUp/PageDown 调整色相，透明度沿用 RGBA 输入。绘制采用固定数量的小矩形，CPU/GPU 共用，无平台依赖。它不是系统屏幕取色器。


<a id="advanced-controls--展厅与验证"></a>

### 展厅与验证

`cargo run -p widget-gallery -- cpu` 的“增强交互”展示搜索多选、自定义输入、页签关闭/重排、按展开请求加载设备、树多选/移动，以及万行虚拟表格。原颜色区域增加 HSV 控件。

专项测试覆盖过滤后的隐藏选择、自由文本提交、懒加载状态、多选/拖放请求、页签关闭/重排/拖出请求、万行表格节点上限、万节点虚拟树末项导航和焦点保留、编辑草稿回收恢复、菜单分隔/悬停及 HSV 转换/键盘/绘制。测试不等同于所有交互的人工验收或 MCU 实机运行。

桌面呈现主题、控件层级和跨缩放验证见 [桌面控件呈现](appearance.md#desktop-presentation)。

<a id="menu-hover"></a>


<a id="menu-hover--菜单悬停容错"></a>

## 菜单悬停容错

`MenuView.behavior: MenuBehavior` 控制三个参数：

| 参数 | 默认值 | 含义 |
| --- | --- | --- |
| `hover_delay` | 180ms | 停留在新的可用项后才请求切换子菜单 |
| `aim_delay` | 350ms | 朝向当前子菜单的斜向路径上，临时经过相邻项时保留原菜单 |
| `pointer_threshold` | 4 逻辑像素 | 键盘操作后，鼠标相对基准点的累计位移阈值 |

容错三角形从父项上的最近位置连接到实际子菜单近侧边的上下端点（额外留 6px 纵向余量）。右侧空间不足时向左展开，垂直位置按视口裁剪，父项锚点来自实际排版后的行矩形而非固定行高。居中菜单、长标签换行和滚动偏移均使用相同定位路径。

指针反向移动时缩短为普通悬停延迟；停在相邻项最多等待本次容错期限。进入子菜单即取消父层待切换请求。普通叶子项停留到期只请求收起子菜单，不执行命令。分隔项与禁用项不触发打开。应用仍通过 `MenuEvent::OpenPath` 接受或拒绝路径变化。

键盘导航取消悬停请求并记录鼠标位置；阈值以内的累计抖动不改变列表反馈，也不启动新请求。没有已知位置时，第一条移动仅建立基准。明确移动、点击或滚动恢复鼠标控制。点击、Right、Enter/Space 立即响应；Left 返回父层，Escape 与外部点击维持原有关闭动作。

离开、捕获丢失、失焦、滚动、点击、键盘操作、相关描述更新或几何变化会取消待处理请求。根菜单共享单个焦点作用域，子菜单不会屏蔽父层输入。返回父层保留其活动项。


<a id="menu-hover--定时动作"></a>

### 定时动作

调度使用注入的单调时间，不读取系统时钟、不创建后台线程，也不持续请求动画帧。静止指针也能在到期时打开菜单。

- `Widget::animate` 保持纯视觉推进与期限查询；菜单在该路径上不发送或消费到期动作。
- `Widget::animate_with_actions` 默认复用 `animate`，需要定时动作的控件可额外输出不透明 `WidgetAction`。
- 自行装配 Tree 的调用方使用 `advance_animations_with_actions` 获取带来源节点的动作，再按普通动作处理。旧的 `advance_animations` 不分发业务动作。
- `WidgetHost` 自动完成动作映射与模型更新。`Application::animate_action` 将重绘、退出和业务唤醒请求完整交给窗口驱动；旧 `animate` 布尔接口继续兼容已有应用。绘制阶段不消费定时动作，待视图重建时不向旧描述分发到期动作。

核心与渲染器不识别菜单类型；菜单几何、容错和选择语义都留在 `zui-widgets`。


<a id="menu-hover--验证"></a>

### 验证

`tools/render-check/tests/menu_hover.rs` 覆盖延迟、静止到期、左右方向、容错超时、反向移动、子菜单入口取消、抖动过滤、点击立即生效及更新取消。原有菜单键盘/分隔条目测试继续运行；核心测试验证移除节点不再发送动作，Host 测试验证无新输入时仍能更新模型并保留窗口意图。

<a id="dialog-window-chrome"></a>


<a id="dialog-window-chrome--弹窗标题与系统窗口装饰"></a>

## 弹窗标题与系统窗口装饰

应用内弹窗和独立系统窗口分别配置。`DialogView` / `MessageBoxView` 是同一个窗口里的模态弹层，不创建独立操作系统窗口，也没有最小化/最大化语义。


<a id="dialog-window-chrome--应用内弹窗"></a>

### 应用内弹窗

```rust
let mut dialog = DialogView::themed(theme, "设置", families, dismiss_action);
dialog.show_title = true;
dialog.show_close_button = true;
let view = dialog.build(body, footer);
```

`show_title` 和 `show_close_button` 独立控制，可组成“标题+关闭”“仅标题”“仅关闭”“都隐藏”四种布局。关闭按钮发送 `PopupDismissed { reason: DismissReason::CloseButton }` 到 `dismiss`，不会发送确认结果。Escape 仍沿用原来的取消事件，外部点击由 `close_on_outside` 决定。隐藏关闭按钮不表示禁止取消。

为兼容原应用的初始焦点，默认仍为显示标题、不显示关闭按钮。启用关闭按钮后，该按钮参与 Tab 导航。正文在独立区域滚动，标题与操作区保持固定；正文滚动容器自身不占 Tab 停靠点，内部输入控件仍可获取焦点。`MessageBoxView` 可以通过 `message_box.dialog` 设置同样的选项。


<a id="dialog-window-chrome--独立系统窗口"></a>

### 独立系统窗口

```rust
let config = WindowConfig {
    decorations: true,
    close_button: true,
    minimize_button: false,
    maximize_button: false,
    resizable: false,
    ..Default::default()
};
```

默认保留系统标题栏和三个按钮。`decorations=false` 关闭系统窗口装饰，可用于无标题栏窗口或应用绘制自己的标题栏。后端不自动生成自绘标题栏；应用用普通 View 组合标题和按钮，并在动作处理时调用 `NativeWindow::drag_window`、`set_minimized`、`set_maximized`。拖动必须由标题区域的鼠标按下触发。关闭自绘窗口走现有 `WindowAction::exit()` / `ViewAction::exit()`。不支持这些能力的后端返回 `PlatformError::Unsupported`。

`resizable=false` 时后端同时禁用最大化按钮。按钮设置是原生能力请求：Windows/macOS 的按钮可能表现为隐藏或禁用，不保证完全相同；其他平台可能不支持。按钮开关不拦截系统关闭请求，不作为退出权限控制。

控件展厅可直接预览系统装饰设置：

```sh
cargo run -p widget-gallery -- cpu --no-minimize --no-maximize
cargo run -p widget-gallery -- cpu --no-titlebar
```

还支持 `--no-close`。无系统标题栏的展厅可按 Escape 退出；该参数用于验证无边框窗口，不会额外创建自绘标题栏。

验证：`cargo test -p zui-backend-winit`、`cargo test -p widget-gallery`、`cargo test -p zui-render-check --test p0 --test scroll`。Windows 的实际窗口按钮外观需在 Windows 实机验收。
