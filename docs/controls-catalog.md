# P1 / P2 控件清单与使用边界

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

## DataGrid：编辑和列宽

DataGridView 包含一个 TableView，复用列、行、选择、排序和主题数据；editable_columns 指定可编辑列，cell_errors 提供按 CellAddress 定位的应用校验信息，edit_revision 用于主动重置编辑草稿。

- 可编辑单元格直接显示 TextInput；输入时保留本地草稿，Enter 发出 `CellEdited { cell, value }`，不直接修改原始行。
- 应用接受后更新 rows；拒绝则保留原值并设置 cell_errors。错误文字和输入边框同时显示。
- Escape 恢复原始值并发出 EditCancelled，应用可清除该格的错误。失焦不会提交或丢弃 DataGrid 草稿。
- 鼠标点击编辑格会请求选中所在行；Tab 在表头、列宽手柄和单元格间遍历。排序请求由应用按真实数据类型执行。
- 拖动表头右侧手柄请求 ColumnResized；聚焦手柄后左右键调整 8 像素，Shift 为 32 像素，Home/End 到 32/4096 像素。范围为逻辑尺寸；应用更新列宽后继续拖动不会累积坐标误差。

现已提供固定表头、虚拟行及自定义单元格/编辑器入口；冻结列、列拖放重排、范围选择和公式尚未实现。详见 [控件增强](advanced-controls.md)。TableView 保持轻量只读用途，DataGridView 提供上述编辑能力，不把两者混同为电子表格。

## 表单和页面组合

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

## 日期和时间

Date 是 1–9999 年的公历日期，不包含时区、系统当前日期或本地化规则。应用显式提供显示月份、选中日期和范围。Calendar 的上月/下月、日期选择均返回 DateEvent；应用确认月份或日期后更新描述。日期格通过 Tab/Enter 操作，尚未实现专用的方向键日历焦点导航。

DatePicker 的触发器返回 Open，`popup(anchor, dismiss)` 复用通用 Popup，锚点为根窗口坐标；应用也可将 Calendar 放入居中弹层。DateRangePicker 组合两个独立 ActionId 的日期触发器并检查起止顺序；跨字段 min/max 约束由应用设置。TimePicker 使用严格的 `HH:MM:SS` 格式（00:00:00–23:59:59），Enter 确认，非法输入保留草稿并标记错误，Escape/失焦恢复已确认值。它不提供时钟表盘、时区换算或闰秒。

## 通知和导航

NotificationView 展示 Info/Success/Warning/Error，设置 dismiss 后返回带通知 ID 的关闭请求。ToastView 默认 3 秒自动隐藏，由已注入的单调 elapsed 时间驱动，不读取系统时间、不创建后台线程、不建立模态焦点作用域。相同 id/revision 的普通重建不重启计时；新 id 或增加 revision 可重新显示。自动隐藏后保留空宿主，应用拥有通知队列和最终移除职责；本轮不提供系统通知服务或全局通知队列。

Breadcrumb 最后一项表示当前位置，不可再次激活；ID 必须唯一。Pagination 使用 1 起始页码，page_count 必须大于零，最多展示五个数字页，另有首页/前页/后页/末页；即使总页数很大，也不会为每一页创建按钮。应用负责翻页查询、加载状态和数据总数。

## 文件选择：控件与 I/O 分离

FilePickerView 的 entries、directory、selected、filename、extensions 等均由应用提供。稳定 FileEntryId 是应用目录记录的标识，不是运行时节点 ID。

- 单击或键盘导航请求 Selected；列表 Enter 或“打开”按钮请求 EnterDirectory/OpenFile。
- “上级目录”请求 Parent；应用读取新目录、清除旧选择并更新 entries。can_go_parent 可限制到应用根目录。
- 保存模式的文件名编辑使用 filename_action（TextInputEvent）；应用更新 filename 后，“保存”按钮发出 SaveFile 请求。
- 选择目录模式确认当前 directory；文件行不可选择。文件名不能为空、不能为 `.`/`..`，不能含路径分隔符或控制字符。
- 扩展名过滤与基本文件名校验只服务界面体验，不替代应用的路径规范化、权限检查、平台文件名规则和覆盖确认。

该组件不是操作系统原生文件对话框，不枚举磁盘、不打开/写入文件。真实桌面或嵌入式应用可注入本地、远程或虚拟文件源；平台适配属于上层应用/独立 I/O 适配模块。展厅使用明确标注的演示目录，确认只更新状态信息，不产生文件写入。

## 颜色选择

ColorPicker 用 RGBA8 表示颜色。十六进制字段接受 `#RRGGBB` 或 `#RRGGBBAA`（可省略 #），通道字段只接受 0–255 整数；Enter 返回 ColorChanged，Escape/失焦撤销本次未确认输入。预设色点击直接请求变化。透明度用矢量棋盘底显示，复用 CPU/GPU 绘制，不依赖位图或图标字体。当前没有 HSV 色轮、屏幕取色、ICC 色彩管理或渐变编辑器。

## 展厅和验证

```sh
cargo run --release -p zui-controls-example -- cpu
```

滚动内容顶部展示本轮补充控件：必填表单校验、日期范围弹层、时间输入、DataGrid 数值校验与分页排序、文件模式与面包屑联动、颜色输入、自动隐藏通知和状态栏。

专项测试入口为 `tools/render-check/tests/catalog.rs`，展厅联动/宽窄布局测试位于 `examples/controls/src/demo/catalog.rs`。完整无障碍平台映射、触摸、真实 MCU 显示设备、系统文件对话框和百万行数据性能均不在当前已验收范围。

## 已有控件增强进展

搜索多选、树加载/多选/虚拟化、虚拟表格、页签操作、菜单分隔与 HSV 的当前能力及剩余缺项见 [控件增强](advanced-controls.md)。增强任务尚未全部完成。
