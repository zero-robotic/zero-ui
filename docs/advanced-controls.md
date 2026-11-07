# 已有控件增强：进展与边界

本页记录“补齐已有控件缺项”的实际进度，整个增强任务尚未全部完成。没有把请求事件、扩展插槽或模拟数据等同于原生平台实现。

| 控件 | 本批新增 | 仍未完成 |
| --- | --- | --- |
| ComboBox | 独立控件、搜索、自由文本提交、多选，过滤后保留隐藏选项的选择 | 输入框方向键直接切入候选列表；目前通过 Tab 切换 |
| TreeView | 懒加载状态/请求、多选、普通树节点拖放请求、固定行高虚拟树 | 虚拟树拖放与自动滚动，虚拟模式 Shift 鼠标跨行范围选择 |
| DataGrid | 测量行高的虚拟行、固定表头、自定义单元格/编辑器、草稿句柄 | 冻结列、列拖动重排、单元格范围选择、内置跨未实现行的焦点导航 |
| Tabs | 关闭、拖动重排、键盘关闭/重排、拖出请求 | 宿主创建独立窗口和页面迁移 |
| 菜单 | 分隔项、当前活动菜单层悬停展开子菜单 | 跨已打开层的悬停切换、系统原生菜单 |
| ColorPicker | HSV 平面/色相条、鼠标拖动、键盘调整 | 系统屏幕吸管 |
| TextArea | 本批未扩展 | 语法高亮、大文档分块 |
| FilePicker | 本批未扩展 | 系统原生文件对话框适配 |

## ComboBox

`SelectView` 保持原来的非编辑选择行为。`ComboBoxView` 不再是它的类型别名，使用独立 `ComboBoxEvent`：Open、QueryChanged、Submitted、Selected、SelectionChanged。

`query`、`selection` 和弹层是否打开均由应用控制。默认支持不区分大小写的子串搜索；`allow_custom=true` 允许 Enter 提交自由文本；`selection=Some(BTreeSet)` 启用多选。应用接收 QueryChanged 后更新 query，再重建弹层。多选后通常保持弹层打开，单选或提交后可关闭。

搜索框复用完整 TextInput，包括 IME、剪贴板、撤销与光标。Tab 进入候选列表，列表沿用键盘多选操作。过滤不会删除隐藏选项的已选状态；全选作用于当前匹配项。远程搜索由应用提供过滤后的条目。

## TreeView

`TreeItem.children_state` 为 Loaded、Unloaded、Loading 或 Failed。非 Loaded 的空节点仍有展开箭头；展开 Unloaded/Failed 节点同时发出 Expansion 和 TreeLoadRequested。应用应立即设为 Loading，再提交异步结果或 Failed，避免重复请求。控件本身不创建线程、不读取目录。

`selection=Some(BTreeSet<TreeItemId>)` 启用多选，接收 TreeSelectionChanged；折叠隐藏的选项仍保留。`selected` 表示主选中/导航项。`draggable=true` 的普通树在拖动距离超过 6 个逻辑像素、释放到另一个可用节点时发出 TreeDropRequested。当前放置语义为“移入目标节点”；应用负责业务校验、更新结构和拒绝非法移动。控件拒绝将可见源节点移入自身或其后代，Escape 取消拖动。

`build_virtual(row_height, viewport_height)` 复用 VirtualList，只创建可见行。选中项变化后自动显露；Home/End、方向键、PageUp/PageDown 在完整的可见逻辑树上导航。固定行高应足够容纳当前字号和标签，长标签可能裁剪。虚拟树仍保留完整输入数据与展平元数据，不代表数据本身为流式存储。

虚拟模式与 draggable 同时启用会返回 VirtualDragUnsupported，避免无提示地失去拖放能力。普通树保留完整 ListBox 的鼠标 Shift 范围选择；虚拟模式支持键盘 Shift 范围选择与 Ctrl 单项切换，尚无 Shift 鼠标跨行范围选择。

## DataGrid

`virtual_rows=Some(DataGridVirtualRows { estimated_height, viewport_height, revision })` 开启自动测量行高的虚拟列表。内容、顺序、字体或校验信息变化时增加 revision。虚拟模式表头固定；普通模式可设置 `freeze_header=true`，并给外层一个明确高度。横向滚动由整个表格共享，表头与正文列对齐。

`cell_factory: Option<DataGridCellFactory>` 接收 CellAddress、当前文本和 disabled，返回任意 View。设置后负责所有单元格内容，可组合日期、数值、下拉等编辑器；自定义控件自行发出应用事件，不强制走文本 CellEdited。默认仍使用原有文本编辑器。

`drafts: BTreeMap<CellAddress, TextInputDraft>` 为需要保留未提交草稿的文本单元格提供稳定句柄。应用拥有句柄和缓存预算；未提供句柄时，被回收行的本地草稿不会保留。普通业务数据仍由应用持有。草稿句柄只能被一个活跃编辑器占用。该机制保留编辑状态，不保存焦点或 IME 预编辑会话。

虚拟模式尚未自动装配跨未创建行的 Tab 导航；当前鼠标滚动可以访问所有行。这一限制与已完成的普通表格键盘操作分开记录。

## Tabs

`closable` 指定可关闭 TabId，显示矢量关闭按钮；Ctrl/Command+W 请求关闭当前页签。`reorderable=true` 启用拖动排序及 Ctrl/Command+Shift+左右键。应用分别接收 TabCloseRequested、TabReorderRequested 并更新 tabs/selected，关闭当前页后要选择有效替代项或 None。

`detachable=true` 时，拖出页签栏上下边界超过 16 个逻辑像素产生 TabDetachRequested，position 为页签栏局部坐标。该事件不会创建窗口；当前单窗口宿主尚未接入页面迁移。展厅启用关闭和重排，没有将拖出请求伪装成独立窗口。

## 菜单和 HSV

`MenuItem::separator(id)` 创建不可激活的分隔行，不参与快捷键解析。它沿用行高以保持菜单定位一致。当前活动层鼠标进入分支行时发出 OpenPath；已打开子菜单的焦点范围限制仍存在，跨层悬停切换尚未补齐。

ColorPicker 内置 HsvPickerView。HsvChanged 同时给出 HSV 和 RGBA；ColorChanged 继续兼容已有颜色联动。应用可消费其中一种，或同时处理，但需避免重复业务副作用。将 HsvChanged.hsv 保存到 ColorPickerView.hsv，可在饱和度/亮度为零时保留所选色相；从预设或十六进制设置新颜色时同步或清除该值。

方向键调整饱和度/亮度（Shift 为较大步长），PageUp/PageDown 调整色相，透明度沿用 RGBA 输入。绘制采用固定数量的小矩形，CPU/GPU 共用，无平台依赖。它不是系统屏幕取色器。

## 展厅与验证

`cargo run -p zui-controls-example -- cpu` 的“增强交互”展示搜索多选、自定义输入、页签关闭/重排、按展开请求加载设备、树多选/移动，以及万行虚拟表格。原颜色区域增加 HSV 控件。

专项测试覆盖过滤后的隐藏选择、自由文本提交、懒加载状态、多选/拖放请求、页签关闭/重排/拖出请求、万行表格节点上限、万节点虚拟树末项导航和焦点保留、编辑草稿回收恢复、菜单分隔/悬停及 HSV 转换/键盘/绘制。测试不等同于所有交互的人工验收或 MCU 实机运行。
