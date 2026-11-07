# P1 控件

本轮范围：TreeView、TableView、SplitterView、TextAreaView。实现均位于 zui-widgets，只依赖既有下层公共接口；没有新增库依赖，core 和渲染后端不识别控件类型。

## TreeView

TreeItem 使用稳定 TreeItemId，支持层级、禁用节点和子节点；expanded 与 selected 由应用维护。禁用节点的后代也不可选择。构建检查全树 ID 唯一性、选中/展开 ID 是否存在，以及最大 64 层深度，隐藏分支也参与检查。

- 点击标签选择；点击矢量箭头展开/收起。
- 上下、Home/End、PageUp/PageDown 和字符导航复用 ListBox。
- 右方向键展开分支或进入已展开分支的可用子节点；左方向键收起分支或选择父节点；Enter/Space 切换分支展开状态。
- TreeEvent::Selected / Expansion 只是请求，应用更新 selected/expanded 后重建。

折叠隐藏当前选中项时，业务选择仍可保留；可见列表不高亮隐藏项。若应用希望折叠后选中父节点，应同时更新 selected。当前已补充懒加载请求、多选、普通树拖放请求和虚拟树入口；虚拟拖放等组合能力尚未完成，见 [控件增强](advanced-controls.md)。

## TableView

TableColumn 定义稳定列 ID、标题、逻辑宽度及 sortable；TableRow 定义稳定行 ID、文本单元格和 disabled。构建检查重复 ID、列宽（至少 32）、单元格数量、选中项及排序状态。

- 点击行或在该行获得焦点后按 Enter/Space，发出 TableEvent::Selected。
- Tab 在可交互表头/行间切换；禁用行不获取焦点。
- 点击可排序表头发出 SortRequested(TableSort)，同列升降序交替，矢量箭头显示已确认状态。
- 排序与选中由应用执行；框架不猜测字符串、数字、日期的排序规则。
- 固定列宽、单元格换行、按行自适应高度、水平和垂直滚动。

当前是只读文本表格，表头随内容一起滚动。暂不包含冻结表头/列、列拖拽与调整宽度、单元格编辑、范围多选、方向键跨行焦点管理及虚拟化；不将其表述为完整 DataGrid。

## SplitterView

`SplitterView::themed(theme, ratio, axis, action).build(first, second)` 返回两栏布局。Horizontal 表示左右分栏，Vertical 表示上下分栏。外层尺寸通过返回 View 的 style 设置，保留构造器生成的 Grid 结构。

分隔条默认 8 逻辑像素、比例范围 0.1–0.9；支持拖动、指针捕获、越界比例钳制，以及焦点状态。键盘沿分栏方向每次调整 1%，Shift 调整 10%，Home/End 到最小/最大比例。Escape 结束拖动，已经被应用确认的比例不回滚。SplitterChanged 请求由应用确认，两个面板使用稳定 key 保留状态。

最小值按比例配置，不是面板像素最小尺寸。面板内容超出时裁剪，需滚动时由面板内 ScrollView 承担。当前不包含多级拖放停靠、分栏吸附、自动持久化或鼠标指针形状适配。

## TextAreaView

复用 TextInput 的 TextBuffer、Unicode 字素选择、撤销/重做、剪贴板、IME、草稿交接和焦点机制，不复制编辑引擎。`editor` 提供文本、action、只读/禁用、校验状态、占位文本、字体、主题和预算配置；TextArea 强制多行且关闭密码模式。

- 默认自动换行（wrap=true），可关闭；visible_lines 默认 4，仅决定自然高度。
- Enter 插入换行，Tab 保留焦点遍历；通过 TextInputEvent::Changed 报告文本，不再以普通 Enter 提交。
- 上下方向键按视觉行移动并保留目标横向位置；Home/End 到视觉行首/尾，Ctrl/Command+Home/End 到全文首/尾；Shift 扩展选区。
- 指针命中使用二维文字几何，光标自动滚动到可见区域；滚轮移动视口，边界未消费的滚动可交给外层。
- 输入法组合期间，候选操作不会误插入换行。多行粘贴和 IME 提交进入同一撤销历史。
- TextInputDraft 保留已提交内容/选择/历史，区分单行与多行模式；不会保存进行中的输入法组合。

TextArea 是纯文本编辑器，当前无行号、语法高亮、富文本、可拖动内部滚动条或超大文档分块布局。仍按既有 EditLimits 限制文本和历史内存，不宣称大文件编辑器性能。

## 展厅与验证

```sh
cargo run --release -p zui-controls-example -- cpu
```

展厅滚动内容顶部展示 P1：树和表格共享设备选择；表格标题可排序；分栏拖动调整两侧宽度；下方多行巡检记录实时更新字数。

专项测试位于 tools/render-check/tests/p1.rs，覆盖树展开/选择/校验、表格选择与排序请求、分栏拖动/键盘/非法比例、TextArea 换行/导航/撤销/只读/IME/多行粘贴/滚动/草稿重挂载，以及 CPU/GPU 分隔条像素。既有 P0、ListBox 和单行编辑器测试继续回归。完整无障碍平台映射、触摸及 MCU 实机验收不在本轮交付范围。

## 补充：完整清单中的其余 P1 与 P2

新增 Form/FormField、GroupBox/Separator，以及日期时间、工具栏/状态栏、通知、面包屑/分页、文件和颜色选择器。另新增 DataGridView，在保持 TableView 只读接口的同时提供列宽拖动、单元格编辑与校验。以上接口、状态职责和限制见 [完整控件清单](controls-catalog.md)。
