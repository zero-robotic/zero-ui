# 文字服务、编辑与输入集成

[文档索引](README.md) · [模块边界](module-boundaries.md) · [未完成事项](implementation-plan.md)

按架构职责归并的专题参考。设计目标与接口约束分开理解；待办统一维护在实施计划中，各节的验证说明仅覆盖所述范围。

## 章节导航

- [文字服务与字形绘制](#text-design)
- [Unicode 编辑状态](#editing)
- [单行输入框基础](#text-input)
- [输入法会话与候选位置桥接](#ime)
- [文本剪贴板](#clipboard)

<a id="text-design"></a>


<a id="text-design--文字服务与字形绘制"></a>

## 文字服务与字形绘制

当前实现包含排版、渲染、只读文本与交互几何；单行编辑和 IME 接入进展见 [输入框](text.md#text-input) 与 [输入法会话](text.md#ime)。


<a id="text-design--模块边界"></a>

### 模块边界

- `zui-api::text` 定义 `TextService`、请求、样式、排版结果和错误；不公开 Parley 类型。
- `zui-text::ParleyText` 使用锁定的 Parley 0.12.0，实现显式字体注册、测量、换行、对齐和字形定位。开启 `complex-scripts` 数据支持。
- `zui-paint` 保存字体资源、字形 ID、逻辑坐标和可变字体归一化坐标。`TextLayout::paint` 使用已经定位的字形，不重新排版。
- `zui-vello` 转换公共字形描述并校验字体格式与集合索引；CPU/GPU 适配器调用各自 Vello 0.3.0 的文字绘制能力。渲染器不依赖文字服务或控件。

调用方创建文字服务并通过 `Tree::layout_with_text` 注入消费者。布局实现仅使用测量契约，不直接引用 Parley；核心和标准控件也不引用具体文字实现。


<a id="text-design--只读文本控件"></a>

### 只读文本控件

`zui-widgets::TextView` 描述文本、样式和颜色，构造持久 `Text` 控件。使用 `View::new(TextView::new(...)).styled(...)` 配置布局与内边距；通过正常 View 协调更新，不需要核心识别文本类型。

`MeasureContext` 借用文字服务，控件根据已知内容宽度或可用空间测量。布局可能多次试测，也可能因宽高都已确定而跳过测量，因此不能直接绘制最后一次试测结果。核心在完整布局结果验证后，对每个控件调用 `Widget::prepare`，传入最终内容尺寸和文字服务；Text 在最终宽度下排版并保存结果，之后绘制只消费这个结果，不再排版。Text 现在维护最多两个本地排版结果：已提交的绘制结果和最近一次试测结果。相同内容、样式、宽度和字体 epoch 可复用；最终准备仅在试测宽度与最终内容宽度一致时接管试测结果，避免重复排版。其他控件引起全树布局时，未变化的 Text 也可保留排版。

`PaintContext` 同时给出边框盒尺寸和扣除 padding 的内容区域。文字在内容原点绘制；溢出由 View 的裁剪配置控制。最终准备失败时不提交新几何，树保持布局失效，禁止生成场景或进行命中测试；控件私有缓存可能已改变，重试成功前不得使用，当前不承诺缓存事务回滚。

仅颜色变化报告 Paint 失效，不调用排版；文字或样式变化报告 Layout 失效。字体 epoch 变化会在下一次 `layout_with_text` 中触发重排；替换文字服务时必须使用不同 epoch，或重建 Tree；单纯使布局失效不会清除控件内的有效排版缓存。没有文字服务的 `Tree::layout` 仍可用于非文字控件，文字请求返回 `ServiceUnavailable`。


<a id="text-design--当前语义"></a>

### 当前语义

默认模式由调用方提供 `Arc<[u8]>` 并注册，返回可用字体族名，不发现系统字体。可选 `zui-text/system-fonts` feature 提供显式的系统字体模式；仅启用 feature 不改变 `Default` / `new` 的行为。请求指定当前服务可用的字体族顺序；无字体或未知字体族仍返回错误。失败的字体注册不改变活动字体集合和版本号。

桌面装配可以使用：

```rust
let mut text = zui_text::ParleyText::with_system_fonts(Default::default());
let families = text.resolve_families(&[])?;
```

`resolve_families` 按顺序保留已安装的首选字体族，例如 `&["PingFang SC", "Microsoft YaHei", "Noto Sans CJK SC"]`；首选项均不可用或列表为空时，使用系统无衬线字体族。它返回具体名称，不向公共 `TextStyle` 引入平台类型。`font_family_names` 枚举可用字体族名称。解析只处理名称，不能保证每个字体覆盖所有字符；Parley/Fontique 在整形时执行脚本相关的回退。没有系统字体时返回 `NoFonts`，不偷偷内嵌验证字体。

系统发现、文件读取和平台 API 由可选 Fontique 平台适配实现，留在 `zui-text` 内。core、widgets、layout、paint 和 CPU/GPU 渲染器不参与字体发现，也不增加系统字体依赖。Linux/FreeBSD 需要 Fontconfig，构建 Linux 示例需要其开发包；macOS 通过 CoreText，Windows 通过 DirectWrite。系统字体文件不随程序复制或嵌入。

实际使用的系统字体面在输出排版结果时转换为已有 `FontResource`。相同字体面复用资源 ID。系统 TTC 集合按实际使用的字体面提取为独立 SFNT 资源，保留字形 ID、可变字体数据和字体表；移除失效的数字签名并重算偏移与校验和。这样不必把集合内其他字重/语言的字体面也复制到 Scene。原始集合仍可能存在于 Fontique 的源缓存中。按需加载不改变字体选择配置，因此不递增 `font_epoch`；显式注册字体仍递增版本号。服务不提供系统字体热刷新接口；系统字体安装变化后应重新创建服务和 Tree。可用 `with_system_fonts_and_policy` 选择严格缺字策略。

注册字体无法覆盖文字时，ParleyText 默认采用 `MissingGlyphPolicy::FontPlaceholder`：保留整形结果中的字形 0（字体自身的 `.notdef`），不改写、删除或插入替代字符。原文的字节索引、选择区、编辑历史和预编辑状态不变，避免用户输入一个缺字字符就令窗口排版失败。标记外观由字体决定；字体的字形 0 如果为空，不保证可见方框。当前固定 Noto Sans SC 的缺字标记已通过 CPU/GPU 像素验证。

严格字体覆盖检查可使用 `ParleyText::new_with_policy(limits, MissingGlyphPolicy::Reject)`，普通与交互排版均返回 `TextError::MissingGlyph`。策略在服务构造时确定，不提供运行时切换，避免既有缓存与策略不一致。`TextLayout::missing_glyph_count()` 可检查结果中的缺字字形数；它不是缺失字符数或字素数。缺字容错不放宽字号、文本/字形预算、字体格式和其他错误检查，彩色 Emoji 支持仍未验收。

支持统一字号、相对行高、起始/居中/末尾对齐，以及无宽度约束、指定换行宽度和最小内容宽度。不可分割的文字簇可能超过指定宽度；裁剪由控件通过 Painter 设置，不由排版强行截断。

`TextLayout::size` 是文字 advance 宽度（包含尾部空白）和排版高度，不是墨迹包围盒或对齐容器尺寸。居中/末尾对齐后的字形可能位于该宽度之外，消费者应使用请求宽度安排容器。行范围使用原始文本的 UTF-8 字节索引；字形坐标以排版原点为基准，Y 轴向下。基线单独返回。

`font_epoch` 在字体注册成功后改变，供调用方使缓存失效。ParleyText 服务本身不提供跨请求排版缓存；Text 控件提供有界的本地缓存。字号、内容、宽度或字体变化后必须重新排版。当前样式不包含字重、斜体或富文本区间；不支持的合成粗体/斜体返回错误。


<a id="text-design--资源与预算"></a>

### 资源与预算

字体资源使用不可复用的进程内 ID 和共享字节所有权。排版结果与 Scene 持有资源，文字服务销毁后仍能绘制。适配层保留当前场景使用的原生字体身份，避免每帧生成新的资源身份。

文字服务限制显式注册及按需保留的系统字体字节数、字体面数、输入文本字节数、输出字形数和字号。系统字体按提取后实际保留的字体面字节计费，同一面只计一次；导出失败不提交新增资源和预算。Scene 限制字形总数、字体资源字节数，并检查非有限坐标及变换。按资源 ID 计费，同一资源在多个 run 中不重复计费；不同资源即使共享底层字节也可能重复计费。

这些预算不等同于进程内存上限：输出字形预算在整形后检查，系统字体枚举、Fontique 的加载/缓存、Parley 临时分配和 Vello 内部缓存仍需后续独立测量与管理。格式校验用于满足下游字形库的解析前置条件，不代表已经对所有恶意字体完成安全审计。

公共绘制层为 `no_std + alloc`，但当前字体所有权和 ID 需要指针宽度原子操作；Parley 适配使用 `std`。尚未支持所有 MCU，需要后续受限字体格式、存储策略与目标验证，不能仅凭 `no_std` 声明推断 MCU 可用。


<a id="text-design--验证范围"></a>

### 验证范围

固定输入为仓库内的 Noto Sans SC，来源、校验值与许可证见 [字体说明](../assets/fonts/README.md)。测试覆盖中文与拉丁文混排、组合音标、换行、基线、对齐、资源身份、预算及错误路径；真实像素测试覆盖服务销毁后的绘制、1×/2× 缩放、字形裁剪、跨帧清理以及 CPU/GPU 输出误差。

文本控件集成测试另覆盖自动高度、宽度缩小重排、固定宽高准备、padding、颜色更新跳过排版、字体 epoch 失效、失败后恢复，以及两端在 1×/2× 下与最终内容宽度排版的逐像素一致性。

系统字体专项测试验证本机拉丁/中文回退、资源身份、预算与交互几何，并验证服务销毁后的 CPU 绘制。它不代表所有系统和字体组合都已验收。复杂双向文本的最小内容宽度、彩色 Emoji、富文本、字体热替换和嵌入式实机性能仍未验证。命中、光标与选择的当前范围见下节；后续继续完善编辑控件和平台交互链路。


<a id="text-design--交互排版快照"></a>

### 交互排版快照

`TextService::layout_interactive` 返回 `InteractiveTextLayout`，包含普通 TextLayout 和 `Arc<dyn TextGeometry>`。普通 `layout` 路径不保留交互查询数据；未实现交互扩展的文字服务默认返回 `InteractionUnavailable`，不会假装提供不准确的命中结果。

TextGeometry 由公共 API 定义，ParleyText 在适配层实现。它保留生成当前字形的同一份 Parley 布局及完整 Unicode 字素边界索引，公共接口不暴露 Parley 类型，也不依赖编辑模块或控件。查询对象可以克隆共享并在原服务释放后使用；文字、样式、宽度或字体变化后，上层必须重新排版，不应拿旧快照解释新文本。

```rust
let result = service.layout_interactive(TextRequest {
    text: "中文输入",
    style: &style,
    width: TextWidth::AtMost(300.0),
})?;
let position = result.geometry.cursor_at(Point::new(30.0, 12.0))?;
let caret = result.geometry.caret(position, 1.0)?;
// 先绘制选区，再 result.layout.paint(...)，最后绘制光标。
```

公共值类型与查询：

- `TextPosition { byte_index, affinity }` 使用原始 UTF-8 字节偏移；TextAffinity 表达逻辑前侧 Upstream 或后侧 Downstream。在软换行和双向文字边界，同一字节偏移可能有两个视觉位置，不能只保存整数偏移。
- `cursor_at(Point)` 返回光标位置。坐标为排版原点下的逻辑单位；控件负责按内容原点和滚动变换换算，例如文字向左滚动 s 时，命中坐标为局部 x 减内容原点再加 s。非有限值或无法表示为底层 f32 的坐标返回 InvalidPoint。
- `caret(position, width)` 返回逻辑矩形；宽度必须有限且非负，零宽用于几何查询。无效字节位置或完整字素内部位置被拒绝，不静默当成字符索引。
- `selection_rects(anchor, focus)` 支持正反选择，返回跨行或跨双向文字片段的多个矩形。相同逻辑偏移的两种亲和性不构成文本选择。
- `move_visual(position, Left/Right)` 按排版视觉顺序移动，处理双向文字和软换行亲和性；不能用 zui_widgets::edit 的逻辑前后字素移动代替。

交互适配器使用 unicode-segmentation 1.13.3 校验完整扩展字素边界。字体回退造成底层 run 切开字素时，命中通过相邻完整字素的光标几何选取最近边界，视觉移动跳过内部边界；异常几何返回错误，不允许产生可用于拆分字素的编辑位置。此保护分支不等于已完成所有回退字体组合的覆盖测试。

新增五项文字服务测试覆盖普通/交互路径字形一致性、居中对齐、快照生命周期、软换行亲和性、跨行选择、空文本/末尾换行、非法查询、完整字素与双向视觉顺序。双向测试使用显式 RTL 控制字符和已注册的拉丁字形，不代表已验证所有自然语言字体。新增 CPU/GPU 测试在 1×/2× 下绘制选区、光标和字形；预览命令：

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/text-selection-cpu.png text-selection
```

本阶段未实现输入框、系统 IME 光标区域、视觉上下移动、词级/行级选择或无障碍文本接口。交互快照额外保留底层布局与边界索引，仍受现有文字大小/字形预算限制；普通标签无需支付这部分持有成本。

<a id="editing"></a>


<a id="editing--unicode-编辑状态"></a>

## Unicode 编辑状态

状态：`zui_widgets::edit` 已实现与界面无关的文本、选择区、逻辑字素移动及有限撤销历史。已由 TextInput 接入可见编辑；编辑模块自身不处理系统输入法、剪贴板或可视光标。


<a id="editing--模块边界"></a>

### 模块边界

原 `zui-edit` crate 已合入 `zui-widgets`，公开入口改为 `zui_widgets::edit`，原有算法测试迁至 `crates/zui-widgets/tests/editing.rs`。无需额外 feature；该算法也是文本输入控件的基础能力。

`zui_widgets::edit` 使用 `no_std + alloc`，只依赖固定版本 `unicode-segmentation 1.13.3`，算法模块不引用输入设备、字体、具体控件或渲染后端；其宿主 crate 为 zui-widgets。它负责已提交文本的编辑规则；上层负责将键盘/IME/剪贴板行为转换成编辑操作。第三方分段类型不出现在公共接口中。

布局服务负责坐标命中、字形簇与字节偏移映射、双向文字的视觉顺序和选择区域几何。编辑模块只处理逻辑文本顺序，不得反向查询布局或控件树。后续输入框和多行编辑器可依赖此模块，但不能把窗口事件和控件状态移入其中。


<a id="editing--公共入口"></a>

### 公共入口

```rust
use zui_widgets::edit::{EditLimits, Movement, TextBuffer};

let mut buffer = TextBuffer::new("中文👩🏽‍💻", EditLimits::default())?;
buffer.move_cursor(Movement::PreviousGrapheme, true);
assert_eq!(buffer.selected_text(), "👩🏽‍💻");
buffer.replace_selection("Rust")?;
assert_eq!(buffer.text(), "中文Rust");
buffer.undo();
assert_eq!(buffer.text(), "中文👩🏽‍💻");
```

`Selection { anchor, focus }` 使用原始 UTF-8 字节偏移，支持反向选择。只有 `TextBuffer::set_selection` 校验成功后才能设置选择区；两个端点必须是扩展字素簇边界，不能落在 UTF-8 字节中间或组合字符内部。原文不进行 Unicode 规范化，也不改写换行符。

- PreviousGrapheme/NextGrapheme 按逻辑字素簇移动，Shift 等修饰键由上层转换为 `extend = true`。不扩展时，已有选择区分别折叠到起点或终点。
- TextStart/TextEnd 指整个文本的端点；不是换行后的视觉行首/行尾。
- PreviousWord/NextWord 使用 Unicode 词边界分段，跳过纯空白段，分别到前一非空白段起点或后一非空白段终点。标点和 emoji 仍作为分段参与，不进行自然语言词典分词；中文不会自动按词语合并。
- replace_selection 替换选中范围；delete_backward/delete_forward 删除选择区，否则删除相邻完整字素簇。
- delete_word_backward/delete_word_forward 优先删除已有选择区，否则删除到相应词级位置，必要时包含中间空白。一次删除为一个撤销单元，撤销恢复原文与原选择方向。
- select_all、selected_text、selection 提供选择操作与只读查询。
- reset 用于外部整段文本替换，光标放在末尾并清空历史；超出文本预算时不改变任何状态。

插入或删除可能使邻近字符重新组成一个字素簇，例如组合重音、ZWJ emoji 或区域指示符旗帜。编辑后若插入端位于新字素内部，光标前移到该字素末尾；撤销仍恢复编辑前的原文及精确选择区。


<a id="editing--撤销和资源限制"></a>

### 撤销和资源限制

一次文本替换是一个撤销单元，记录起点、删除/插入的 UTF-8 字符串，以及操作前后的选择区，不保存完整文档快照。连续输入尚未合并为词或时间分组。移动光标和选择区本身不增加历史；替换成相同文本只调整选择区，不打断重做链。

EditLimits 分别限制当前文本字节数、撤销和重做合计的字符串载荷字节数、历史条数。默认文本与历史载荷各 1 MiB，最多 256 条历史。历史字节数不包括集合和记录结构的分配开销；条数另行限制。边界扫描目前是线性算法，尚未提供 Rope 或大文档索引。

超出文本预算的操作返回 BudgetExceeded，原文、选择和历史均保持不变。历史预算不足时淘汰最旧撤销记录；单次编辑本身无法纳入历史或历史条数为零时，编辑照常提交并清空旧历史，避免跨越未记录操作后错误应用旧差量。撤销/重做仅在两个队列间移动记录，不扩大历史载荷。


<a id="editing--已分配容量与显式历史释放"></a>

### 已分配容量与显式历史释放

`TextBuffer::memory_usage()` 返回 EditMemoryUsage，分别报告当前文本长度、文本缓冲容量、历史字符串载荷长度及历史总缓冲容量。历史容量包括 undo/redo 的记录数组，以及每条记录插入/删除字符串的容量；clear_history 清空载荷后保留的数组容量仍会计入。allocated_bytes 合计文本与历史容量，不重复合计长度字段。

统计为 O(H)，H 为历史条数，不放入每帧绘制；不包含对象自身、分配器元数据、排版、模型副本或进程其他资源，因此不是 RSS 或整机内存上限。EditLimits 原有载荷限制语义不变，不能据此声称它是严格分配容量限制。

`release_history()` 丢弃撤销/重做并释放两个记录数组，保留文本、当前选择区和文本缓冲容量，返回释放的历史缓冲容量。与保留容器容量的 clear_history 区别明确。重复释放返回 0；之后可以继续编辑并建立新历史。释放内存由分配器接收，不保证操作系统 RSS 立即下降。


<a id="editing--验证与后续"></a>

### 验证与后续

```sh
cargo test -p zui-widgets --test editing --locked
cargo clippy -p zui-widgets --all-targets --locked
```

六项回归测试覆盖中文、组合重音、带肤色的 ZWJ emoji、旗帜、CRLF 与印度文字字素；反向选择、折叠、替换、字符合并、无效偏移、预算失败原子性、历史淘汰和分支重做。边界组合测试遍历多个文本的选择端点与插入片段，逐次验证光标合法性及撤销/重做恢复。

词级回归另覆盖中英文、标点、组合重音、emoji 和纯空白，遍历多个文本的全部字素边界验证双向删除、撤销与重做。词级操作的端点同样保证是完整字素边界；非扩展移动遇到已有选区时先折叠到相应逻辑边缘。

文字服务现已提供可选交互排版快照，支持光标、命中、选择区域与视觉左右移动，详见 [文字服务与字形绘制](text.md#text-design)。[单行输入框基础](text.md#text-input) 已组合编辑状态与这些几何契约，控件层负责只读和单行过滤。IME 预编辑保持为独立临时状态，提交后才写入缓冲区；逻辑字素/词级移动不能冒充双向文字的视觉左右移动。文本剪贴板已通过上层服务注入接入，编辑模块不执行系统 I/O，详见 [文本剪贴板](text.md#clipboard)。视觉词级与行导航、撤销合并、密码策略、业务输入校验与无障碍仍待后续实现。

<a id="text-input"></a>


<a id="text-input--单行输入框基础"></a>

## 单行输入框基础

输入框已接通原生 IME 请求、候选位置与会话代次，详见 [输入法会话与候选位置桥接](text.md#ime)。真实系统输入法的候选显示和回调时序仍待验收。

状态：`TextInput`/`TextInputView` 已接入持久控件树、Unicode 编辑状态与交互排版。支持单行键盘文本、鼠标拖选、删除、撤销/重做、外部同步、横向滚动、光标闪烁与注入的文本剪贴板。完整 IME 验收及无障碍尚未完成。


<a id="text-input--使用与数据所有权"></a>

### 使用与数据所有权

```rust
let input = TextInputView::themed(
    &theme,
    "初始内容",
    families,
    ActionId(5),
);
let view = View::new(input).styled(LayoutStyle {
    width: Dimension::Length(320.0),
    height: Dimension::Length(48.0),
    padding: Edges::all(8.0),
    ..Default::default()
});
```

编辑缓冲区、选择区、撤销历史和滚动偏移属于持久控件。与只输出请求的 CheckBox/Slider 不同，TextInput 本地提交编辑，再发送 `TextInputEvent::Changed(String)` 通知；应用不接收通知也不影响继续输入。应用可按 ActionId 映射 Changed，将字符串写回自身模型。

外部同步规则：

- View.text 初始化缓冲区；View 重建但 text 未变化时，保留本地草稿、光标和历史。
- 外部 text 改成当前草稿时视为确认，保留光标和历史。
- 外部 text 改成其他内容时替换缓冲区，光标置末尾并清空历史。
- 修改 revision 可强制替换，即使外部 text 与上一份 View 相同；这也是应用明确丢弃本地草稿的方式。
- 修改 EditLimits 开始新的编辑会话；改色和普通重建不会清除历史。更换控件 Key 会按通常规则创建新控件。

`TextInputEvent` 还提供 Submitted(String) 和 Rejected(TextInputError)。前者由 Enter 初次按下产生；后者报告单行过滤、编辑预算或几何操作错误。Changed 是已经提交的通知，不应当作可以靠“不更新模型”来拒绝的变更请求。


<a id="text-input--回收后的编辑草稿"></a>

### 回收后的编辑草稿

`TextInputDraft` 是应用持有的可克隆句柄，使用 `TextInputView::with_draft(draft.clone())` 或 draft 字段绑定。应用应按稳定业务 Key 和字段分别保存句柄，在行工厂每次重建时复用，不能每次生成一个新句柄。

```rust
// 创建业务记录时保存，而非每次调用行工厂时创建。
let draft = TextInputDraft::new();
let input = TextInputView::themed(&theme, "初始内容", families, ActionId(5))
    .with_draft(draft.clone());
```

活动控件独占编辑缓冲；销毁时将提交文本、选区、撤销/重做记录、双向光标亲和性和横向滚动偏移移交给句柄。重新生成时取回状态，排版及滚动范围按新环境重新计算。缓冲和历史移动所有权，不复制整份历史；模型/描述中的外部文本快照仍有自己的存储成本。

恢复继续遵守外部同步规则：未变的外部快照保留本地编辑；回写当前提交文本视为确认；不同外部文本、revision 或 EditLimits 改变会开始新编辑状态。活动节点切换到另一个草稿时先归还旧状态，再绑定新草稿并递增 IME 会话代次。重新挂载使用新 NodeId，业务身份由应用映射，不能复用旧运行时 ID。

不保存焦点、指针捕获、拖选状态、IME 预编辑、光标闪烁时钟或排版缓存。虚拟行回收仍会失焦、取消组合并卸载，恢复状态后需明确请求焦点。这是编辑状态保留，不是让离屏节点持续处理输入或维持系统 IME 会话。

一个句柄只能绑定一个活动编辑器。重复绑定得到 DraftInUse 状态，并在布局准备阶段以 InvalidStyle 拒绝，不覆盖原持有者；原持有者释放后，可通过 View 重建重试。失败绑定的控件销毁不会释放其他控件的所有权。`is_attached()` 可检查使用状态，`clear()` 仅在空闲时丢弃保存内容，活动时返回 DraftInUse。

应用负责缓存总量：EditLimits 限制单个文本和历史，不限制所有记录草稿的总内存。删除业务记录时移除句柄，或清理空闲草稿；可使用下文的有界缓存显式维护空闲载荷，仍不提供活动编辑总内存硬预算、跨线程共享或序列化持久化。此实现仍支持 no_std + alloc，不向核心增加具体编辑器依赖。

四项新增回归覆盖选区与撤销/重做恢复、预编辑和焦点不恢复、重复绑定及重试、外部确认/重置、活动节点切换业务草稿，以及真实虚拟列表远距离回收后以新节点恢复编辑记录。


<a id="text-input--草稿容量统计和空闲维护"></a>

### 草稿容量统计和空闲维护

`TextInputDraft::memory_usage()` 报告空闲草稿的 TextInputDraftMemory，包括可用编辑缓冲的 EditMemoryUsage 与保存的外部文本快照容量。allocated_bytes 合计这些自有载荷容量；空草稿为 0，非法编辑缓冲的 edit 为 None，活动草稿返回 DraftInUse，不能把活动占用误计为零。活动控件可用 TextInput::editing_memory_usage 查询相同范围。

`TextInputDraft::release_history()` 仅对空闲草稿生效，释放撤销/重做载荷和记录数组，保留提交文本、选区及其他恢复状态，返回释放的容量。活动时拒绝且不修改状态；空草稿或已释放历史返回 0。clear 会连文本和外部快照一起丢弃。

统计不含 Rc/slot 固定分配、分配器开销、模型其他副本、排版和渲染缓存；同一共享句柄只应计算一次。历史遍历为 O(H)，适合明确的缓存维护时机。应用可以据此先清理空闲历史、再按自己的策略删除草稿，下文缓存可统一执行按访问顺序的显式维护，但不自动调度，也不强制限制活动编辑器的总分配。

底层与控件测试验证载荷和容量差异、两个历史栈的释放、重复维护、新编辑可撤销、活动拒绝，以及维护后重挂载文本/选区保持且旧撤销历史不可用。


<a id="text-input--有界业务草稿缓存"></a>

### 有界业务草稿缓存

`TextInputDraftCache<K: Eq>` 按业务 Key 保存草稿句柄。new(max_entries) 限制登记条目数；get/get_or_insert 返回同一 Key 的共享句柄并更新访问顺序。新 Key 达到上限返回 EntryLimit，不隐式删除其他草稿；已有 Key 在满额时仍可访问。条目数组分配失败返回 AllocationFailed，该错误不覆盖 Rc 分配或应用 Key 自身的分配。

```rust
let mut drafts = TextInputDraftCache::new(64);
let draft = drafts.get_or_insert((record_id, field_id))?;
// 在模型或显式维护步骤中执行；不要放在每帧绘制中。
let report = drafts.maintain(128 * 1024, DraftCachePolicy::HistoryOnly);
if !report.budget_met {
    // 文本和选择仍保留，由应用决定是否扩大预算或明确丢弃草稿。
}
```

maintain 的预算只针对调用结束时的空闲载荷容量。它先从最近最少访问的条目开始释放历史，达标即停。HistoryOnly 不清理提交文本/选区，因此可能无法达标；显式选择 DiscardIdleDrafts 后，才会继续按访问顺序丢弃空闲文本与选区。后者可能丢失尚未写入业务模型的编辑，应用必须明确选择此策略。报告包含前后用量、释放的历史字节数、丢弃草稿数和 budget_met。

维护只清理载荷，保留 Key 与句柄身份，所有外部 clone 看到同一清理结果；它不释放登记名额。remove(key) 明确清理空闲状态并移除 Key，活动条目返回 DraftInUse。删除后应废弃该业务字段的旧描述/句柄，不能将旧句柄与重新登记的同 Key 当作同一个草稿。直接丢弃整个缓存仅释放缓存持有的引用，不清理仍由外部持有的句柄。

usage 报告条目数、活动条目数和空闲载荷容量。活动缓冲不在缓存内部，不能将 budget_met 解释为所有编辑器或进程内存达标；活动编辑器随后卸载仍可增加空闲占用，需要应用再次维护。条目数限制也不约束任意 K 的大小。统计不计 Key、Vec 条目容量、Rc 固定开销、分配器及其他资源。

缓存使用有界 Vec 保持访问顺序，查询/调整为 O(N)，维护为 O(N + H)，H 为保留历史条数。适合有限编辑工作集，不用于预登记百万条业务记录。后台维护、自动登记淘汰、活动编辑硬预算仍未实现；应用可以结合行生命周期和业务缓存策略调用这些接口。

两项新增集成测试覆盖访问顺序、历史优先、只清历史不达标、显式清草稿、共享身份保持、维护幂等、活动保护、满额拒绝、remove 后恢复登记及零容量配置。


<a id="text-input--编辑行为"></a>

### 编辑行为

Control 或 Super 加 C/X/V 通过注入服务复制、剪切和粘贴。剪切写入成功后才删除选区；粘贴失败保留编辑状态，只读仅允许复制。未注入服务时返回 Rejected(Unavailable)，详见 [剪贴板契约与装配](text.md#clipboard)。

文字来自 `KeyInput.text`，不根据物理按键猜测字符。Backspace/Delete 删除选区或相邻完整字素；左右键使用排版视觉顺序，Shift 扩展选区。Home/End 定位逻辑文本首尾。Control 或 Super 加 A 全选，Z 撤销，Shift+Z/Y 重做。Tab 留给核心切换焦点；Enter 提交但不插入换行。

Control 加左右方向键按逻辑顺序移动到 Unicode 词级位置，Shift 扩展选区；Control+Backspace/Delete 删除已有选区或前后一个分段。词边界由独立编辑模块计算，规则见 [编辑状态](text.md#editing)。只读允许词级选择但不删除，预编辑期间不执行这些命令。这是统一的 Control 绑定，尚未实现 macOS Option 词级操作、可配置按键映射或双向文字的视觉词级移动。

鼠标主键按下定位光标并捕获，移动和松开扩展选择；上下拖出单行区域时仍按该行横向位置命中。拖动取消、失焦、删除和禁用遵守公共捕获生命周期。尚不支持 Shift 点击、双击选词或三击选行。

单行输入拒绝 Unicode 控制字符和 U+2028/U+2029，因此换行、Tab 和 NUL 不写入缓冲区；组合字符、连接符及其他非控制字符保持原文。文本超出 EditLimits 时发出 Rejected，原有文本/选择/历史不变。初始或外部替换文本非法时，布局准备失败，不发布场景。

read_only 保留聚焦、导航和选择，禁止修改；disabled 不可聚焦或编辑。首期无占位文字、密码掩码或输入校验回调。默认 ParleyText 对缺字保留原文并输出字体自带的缺字字形，普通输入和预编辑均可继续选择、删除、提交或撤销。显式选用严格策略的文字服务仍会返回 MissingGlyph；该策略由装配层选择，控件不依赖具体文字实现。详见 [文字服务的缺字策略](text.md#text-design)。


<a id="text-input--排版与滚动"></a>

### 排版与滚动

光标通过公共动画契约按需闪烁，默认亮/暗各 500 ms；caret_blink_interval 为 None 时常亮。输入和选择变化重置可见阶段，预编辑与选区期间停止闪烁请求。闪烁不改变 IME 候选矩形或触发布局，详见 [动画调度](runtime.md#animation)。

单行文本采用 Unbounded 交互排版，不随输入框宽度换行。测量/准备复用缓存，字体 epoch 变化或文本/文字样式变化会重新排版。选择变化复用几何结果，保证绘制选区、光标与字形来自同一布局。

内容按外层 padding 裁剪，光标越过可见区域时调整横向滚动。事件命中转换为文字局部坐标：减去内容原点、加上横向滚动偏移。普通指针移动不复制整段文本；提交编辑及通知目前仍使用 String，尚未优化为大型文档的增量通知。

`TextInputStyle` 提供背景、文字、边框、焦点边框、选区、光标、禁用文字色及自然宽度。光标在聚焦且选择折叠时按 caret_blink_interval 闪烁，None 时常亮。公开错误来自 ZUI 契约，控件不依赖核心、Taffy 或 Parley 具体实现。


<a id="text-input--输入法边界"></a>

### 输入法边界

`ImePreedit` 使用独立显示快照替换当前选择区域，绘制预编辑文字、下划线及候选选区，不修改已提交文本或撤销历史，也不发出 Changed。cursor 为 None 时隐藏预编辑光标；字节位置必须是合法 UTF-8 边界，绘制位置再吸附到完整字素边界。快照服从文本预算，非法输入保留之前的有效状态。

`ImeCommit` 将最终文字作为一次编辑提交，一次撤销恢复原文。空预编辑、ImeDisabled、失焦、禁用、只读切换或外部替换撤销临时显示；原来的已提交选择区仍可用于紧随空预编辑到达的提交。非空预编辑期间编辑按键和鼠标选择不会修改底层选择区，按键也不触发窗口快捷键。ImeEnabled 本身不抑制普通键盘文本。

聚焦且可编辑时请求启用原生 IME，候选矩形随光标和横向滚动更新。外部替换和只读等会话变化会重置平台请求。Host 会拒绝与待处理 View 更新前的会话不一致的组合数据；原生事件自身没有会话标识，因此尚不能完整隔离所有迟到事件。真实平台输入验证完成前，不宣称完整中文输入法支持已验收。


<a id="text-input--验证与示例"></a>

### 验证与示例

```sh
cargo run -p zui-render-check --locked -- cpu artifacts/text-input-cpu.png text-input
cargo test -p zui-render-check --all-features --locked --test text_input
# 需要真实 GPU。
cargo test -p zui-render-check --all-features --locked --test text_input -- --ignored
cargo run -p zui-window-check --all-features --locked -- cpu --widgets
```

六项常规控件测试覆盖 Unicode 删除与撤销、确认/强制同步、拖选坐标、视觉键盘选择、横向滚动、拒绝输入、只读/禁用、合成 IME 事件及 CPU 像素。另有 GPU 1×/2× 文字/选区/光标/裁剪测试。窗口 Demo Host 回归验证模型确认、连续输入/删除和新增输入框后的 Tab 顺序。

新增三项预编辑集成测试与一项字素边界单元测试，覆盖临时显示、取消后的像素恢复、提交与撤销、非法输入及外部覆盖。

候选位置与会话测试覆盖滚动、预编辑光标、确认回写、强制替换、只读切换，以及 Host 在外部重置与旧提交交错时的处理。后续继续验证原生 IME 与跨应用剪贴板互操作，并实现更多导航/选择、可访问性和多行编辑。定时窗口冒烟与合成事件测试不等于真实键盘/输入法自动化验收。

<a id="ime"></a>


<a id="ime--输入法会话与候选位置桥接"></a>

## 输入法会话与候选位置桥接

已实现控件能力查询、窗口逻辑坐标换算与平台会话同步，内置 TextInput 已接入。聚焦且可编辑时请求原生 IME，只读、禁用或无可见内容区域时撤销请求。自动化测试覆盖真实控件的合成输入路径及窗口替身，不代表真实系统输入法已经验收。


<a id="ime--职责边界"></a>

### 职责边界

| 模块 | 职责 |
| --- | --- |
| zui-api | 定义 Widget::text_input 与 TextInputState，默认不请求输入法 |
| zui-core | 只查询已聚焦且可用的控件，验证局部矩形，转换为窗口坐标并应用祖先裁剪 |
| zui-platform | 定义 ImeRequest、会话标识与原生窗口操作，不引用控件类型 |
| zui-app | 将核心结果转换为平台请求，合并更新并同步窗口状态 |
| 平台适配器 | 接收开关和候选矩形，调用对应窗口系统接口 |

控件报告相对于自身边框盒的 cursor_area 和 generation。坐标换算使用与绘制一致的内容边界与节点位置；窗口接口接收逻辑坐标，平台完成 DPI 转换，装配层不重复乘以缩放率。矩形必须有限且合法，高度必须为正，允许零宽度光标。无可见交集时不请求 IME。

平台会话标识包含不透明 owner 与 generation；平台不解析 owner 的控件含义。控件需要撤销旧组合输入时推进 generation。核心在布局失效时拒绝查询，避免输出尚未提交的几何。


<a id="ime--同步与生命周期"></a>

### 同步与生命周期

首次请求先设置候选位置，再启用 IME。位置变化只更新矩形；所有者或 generation 变化先停用旧会话，再设置位置并启用新会话；相同请求不重复调用平台。请求撤销时停用一次。

WidgetHost 在新布局完成后发布位置。在同一焦点的新一帧尚未完成时保留上次提交的位置，不为查询 IME 强制重建 View；多个模型更新仍合并到一帧。焦点切换不继续保留旧所有者的请求。布局或协调失败清除缓存，窗口驱动在更新、绘制或呈现失败时停用 IME。

失焦、不可绘制窗口、退出请求和断开连接会停用 IME。驱动还独立记录窗口焦点，即使自定义 Application 持续返回请求，也不能在失焦后的重绘中重新启用。

TextInput 的候选位置与绘制使用同一个文字原点，包含 padding、垂直居中和横向滚动，并限制在内容区域内。预编辑期间使用临时文本的光标位置。普通光标移动、预编辑更新和应用确认回写不改变 generation；外部替换、revision/编辑预算变化、禁用/只读切换、动作接收方变化和失焦推进 generation。ImeDisabled 只取消临时显示，不推进 generation，避免平台关闭通知触发重复重启。

WidgetHost 在派发组合数据前完成待处理的 View 更新，比较当前能力与上次发布的会话。若所有者或 generation 已改变，或已无文本输入能力，就消费并丢弃该次 Preedit/Commit，随后发布新请求。此检查覆盖“外部更新已排队、下一帧前到达旧提交”的情况；不强行重建每个普通模型事件。


<a id="ime--验证与后续工作"></a>

### 验证与后续工作

自动化测试覆盖嵌套位置与 padding、裁剪、非法矩形、焦点切换、会话代次、位置更新、DPI 不重复缩放、View 合并、失焦、隐藏、绘制/呈现失败及断开连接。

TextInput 已实现独立预编辑快照、选区与下划线，以及提交/取消/外部覆盖的合成事件路径，详见 [单行输入框](text.md#text-input)。默认文字服务已支持保留原文的缺字显示。会话代次用于请求同步，当前原生输入事件不携带此标识：新请求发布后才到达的旧平台事件无法仅靠此检查区分，不能宣称已解决所有迟到事件。真实桌面候选窗口位置、平台回调顺序和中文组合输入仍待验证。

<a id="clipboard"></a>


<a id="clipboard--文本剪贴板"></a>

## 文本剪贴板

已实现公共文本剪贴板契约、事件服务注入、输入框复制/剪切/粘贴和桌面原生适配。自动化测试使用替身验证，不读取或覆盖开发机器的系统剪贴板；跨应用复制粘贴仍需实机验收。


<a id="clipboard--模块与所有权"></a>

### 模块与所有权

原 `zui-clipboard` crate 已合入 `zui-app`，不再保留兼容包装。`zui-app` 默认不启用原生剪贴板；工作区应用需要显式配置：

```toml
zui-app = { workspace = true, features = ["clipboard"] }
```

原 `zui-clipboard/wayland` 改为 `zui-app/clipboard-wayland`，会自动启用 `clipboard`。仅使用 CPU 或 GPU feature 不会引入 arboard。原生连接仍为懒加载，单元测试不访问系统剪贴板。

`zui-api::clipboard::Clipboard` 定义 read_text(max_bytes)、write_text 和框架错误类型，不包含平台句柄。`EventServices` 在单次事件路由中借用该服务；核心只转交服务，不解释剪贴板内容，也不识别输入框。既有 Widget::event 实现仍可使用，需服务的控件实现 event_with_services。直接焦点/捕获通知不携带服务。

`Tree::dispatch_input_with_services` 支持显式注入；普通 dispatch_input 使用 UnavailableClipboard，未配置时报告 Unavailable，不伪装成成功。WidgetHost 通过 with_clipboard 持有注入实例。控件不保存服务引用，不通过全局单例查找平台能力。

`zui_app::clipboard::NativeClipboard` 是 zui-app 中由 `clipboard` feature 启用的桌面适配器，实现仅使用公共 API 与锁定的 arboard 3.6.1；核心、控件、平台契约与渲染器均不依赖它。首次使用时连接，随后保留实例，构造本身不访问剪贴板。桌面窗口示例显式注入：

```rust
let host = WidgetHost::new(model, layout, text, limits)?
    .with_clipboard(zui_app::clipboard::NativeClipboard::default());
```

arboard 默认图像功能已关闭，本阶段只处理 UTF-8 文本。Linux 默认使用 X11，原生 Wayland 数据控制可通过 zui-app 的 `clipboard-wayland` feature 启用，仍取决于合成器支持；本轮仅验证 macOS 编译。Linux 的剪贴板内容所有权可能随实例销毁而丢失，因此不能每次操作后立即丢弃连接。详见 [arboard 3.6.1 文档](https://docs.rs/arboard/3.6.1/arboard/struct.Clipboard.html) 与 [功能配置](https://docs.rs/crate/arboard/3.6.1/features)。无桌面服务的嵌入式配置可不链接此适配器，按应用需要注入替代实现。


<a id="clipboard--编辑语义"></a>

### 编辑语义

- Control 或 Super 加 C/X/V 发起复制、剪切和粘贴；按键重复不重复执行剪贴板操作。
- 复制只写入选中的原始文本；空选区不覆盖剪贴板，不改变编辑历史。
- 剪切先写入剪贴板，成功后才删除选区；失败返回 Rejected，原文、选择区和撤销历史保留。平台写入失败不保证操作系统剪贴板本身能回滚。
- 粘贴先读取，再按单行规则和剩余文本预算校验，最后一次性替换选区；不自动删除换行。空文本不删除已有选区。失败不改变编辑状态，成功属于一个撤销单元。
- 只读允许复制与选择，禁止剪切和粘贴；禁用或非空预编辑期间不执行剪贴板操作。

返回文本字节限制由适配器检查，控件的编辑缓冲区再次检查最终文本预算。该限制不代表操作系统读取过程的内存分配上限。当前为同步调用，系统剪贴板阻塞可能影响 UI 响应；后续若引入异步服务，必须加上节点身份、编辑版本与选择区有效性检查，不能将晚到文本直接写入新的焦点控件。


<a id="clipboard--验证范围"></a>

### 验证范围

控件测试覆盖 Unicode 文本复制、剪切失败原子性、粘贴、撤销/重做、读取失败、不可用服务、预算、单行拒绝、只读、重复键与预编辑隔离。Host 集成测试验证注入服务确实接入核心路由，并将剪切/粘贴动作回写应用模型。

图片、HTML、文件列表、Linux primary selection、跨应用实机互操作和异步剪贴板不在当前实现范围。公共契约继续保持 no_std + alloc，桌面适配器需要 std；不据此宣称 MCU 支持已完成。

## 系统字体专项验证

常规测试继续显式注册仓库字体，即使 workspace 功能合并启用了 `system-fonts`，也不读取系统字体。以下测试需要已安装拉丁文和中文字体，默认忽略，显式执行：

```sh
cargo test -p zui-text --features system-fonts --locked -- --ignored
cargo test -p zui-render-check --no-default-features --features cpu,system-fonts --test system_fonts --locked -- --ignored
```

系统字体随操作系统版本和安装环境变化，不能用来替代固定字体的逐像素基线。
