# 单行输入框基础

输入框已接通原生 IME 请求、候选位置与会话代次，详见 [输入法会话与候选位置桥接](ime.md)。真实系统输入法的候选显示和回调时序仍待验收。

状态：`TextInput`/`TextInputView` 已接入持久控件树、Unicode 编辑状态与交互排版。支持单行键盘文本、鼠标拖选、删除、撤销/重做、外部同步、横向滚动、光标闪烁与注入的文本剪贴板。完整 IME 验收及无障碍尚未完成。

## 使用与数据所有权

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

## 回收后的编辑草稿

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

## 草稿容量统计和空闲维护

`TextInputDraft::memory_usage()` 报告空闲草稿的 TextInputDraftMemory，包括可用编辑缓冲的 EditMemoryUsage 与保存的外部文本快照容量。allocated_bytes 合计这些自有载荷容量；空草稿为 0，非法编辑缓冲的 edit 为 None，活动草稿返回 DraftInUse，不能把活动占用误计为零。活动控件可用 TextInput::editing_memory_usage 查询相同范围。

`TextInputDraft::release_history()` 仅对空闲草稿生效，释放撤销/重做载荷和记录数组，保留提交文本、选区及其他恢复状态，返回释放的容量。活动时拒绝且不修改状态；空草稿或已释放历史返回 0。clear 会连文本和外部快照一起丢弃。

统计不含 Rc/slot 固定分配、分配器开销、模型其他副本、排版和渲染缓存；同一共享句柄只应计算一次。历史遍历为 O(H)，适合明确的缓存维护时机。应用可以据此先清理空闲历史、再按自己的策略删除草稿，下文缓存可统一执行按访问顺序的显式维护，但不自动调度，也不强制限制活动编辑器的总分配。

底层与控件测试验证载荷和容量差异、两个历史栈的释放、重复维护、新编辑可撤销、活动拒绝，以及维护后重挂载文本/选区保持且旧撤销历史不可用。

## 有界业务草稿缓存

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

## 编辑行为

Control 或 Super 加 C/X/V 通过注入服务复制、剪切和粘贴。剪切写入成功后才删除选区；粘贴失败保留编辑状态，只读仅允许复制。未注入服务时返回 Rejected(Unavailable)，详见 [剪贴板契约与装配](clipboard.md)。

文字来自 `KeyInput.text`，不根据物理按键猜测字符。Backspace/Delete 删除选区或相邻完整字素；左右键使用排版视觉顺序，Shift 扩展选区。Home/End 定位逻辑文本首尾。Control 或 Super 加 A 全选，Z 撤销，Shift+Z/Y 重做。Tab 留给核心切换焦点；Enter 提交但不插入换行。

Control 加左右方向键按逻辑顺序移动到 Unicode 词级位置，Shift 扩展选区；Control+Backspace/Delete 删除已有选区或前后一个分段。词边界由独立编辑模块计算，规则见 [编辑状态](editing.md)。只读允许词级选择但不删除，预编辑期间不执行这些命令。这是统一的 Control 绑定，尚未实现 macOS Option 词级操作、可配置按键映射或双向文字的视觉词级移动。

鼠标主键按下定位光标并捕获，移动和松开扩展选择；上下拖出单行区域时仍按该行横向位置命中。拖动取消、失焦、删除和禁用遵守公共捕获生命周期。尚不支持 Shift 点击、双击选词或三击选行。

单行输入拒绝 Unicode 控制字符和 U+2028/U+2029，因此换行、Tab 和 NUL 不写入缓冲区；组合字符、连接符及其他非控制字符保持原文。文本超出 EditLimits 时发出 Rejected，原有文本/选择/历史不变。初始或外部替换文本非法时，布局准备失败，不发布场景。

read_only 保留聚焦、导航和选择，禁止修改；disabled 不可聚焦或编辑。首期无占位文字、密码掩码或输入校验回调。默认 ParleyText 对缺字保留原文并输出字体自带的缺字字形，普通输入和预编辑均可继续选择、删除、提交或撤销。显式选用严格策略的文字服务仍会返回 MissingGlyph；该策略由装配层选择，控件不依赖具体文字实现。详见 [文字服务的缺字策略](text-design.md)。

## 排版与滚动

光标通过公共动画契约按需闪烁，默认亮/暗各 500 ms；caret_blink_interval 为 None 时常亮。输入和选择变化重置可见阶段，预编辑与选区期间停止闪烁请求。闪烁不改变 IME 候选矩形或触发布局，详见 [动画调度](animation.md)。

单行文本采用 Unbounded 交互排版，不随输入框宽度换行。测量/准备复用缓存，字体 epoch 变化或文本/文字样式变化会重新排版。选择变化复用几何结果，保证绘制选区、光标与字形来自同一布局。

内容按外层 padding 裁剪，光标越过可见区域时调整横向滚动。事件命中转换为文字局部坐标：减去内容原点、加上横向滚动偏移。普通指针移动不复制整段文本；提交编辑及通知目前仍使用 String，尚未优化为大型文档的增量通知。

`TextInputStyle` 提供背景、文字、边框、焦点边框、选区、光标、禁用文字色及自然宽度。光标在聚焦且选择折叠时按 caret_blink_interval 闪烁，None 时常亮。公开错误来自 ZUI 契约，控件不依赖核心、Taffy 或 Parley 具体实现。

## 输入法边界

`ImePreedit` 使用独立显示快照替换当前选择区域，绘制预编辑文字、下划线及候选选区，不修改已提交文本或撤销历史，也不发出 Changed。cursor 为 None 时隐藏预编辑光标；字节位置必须是合法 UTF-8 边界，绘制位置再吸附到完整字素边界。快照服从文本预算，非法输入保留之前的有效状态。

`ImeCommit` 将最终文字作为一次编辑提交，一次撤销恢复原文。空预编辑、ImeDisabled、失焦、禁用、只读切换或外部替换撤销临时显示；原来的已提交选择区仍可用于紧随空预编辑到达的提交。非空预编辑期间编辑按键和鼠标选择不会修改底层选择区，按键也不触发窗口快捷键。ImeEnabled 本身不抑制普通键盘文本。

聚焦且可编辑时请求启用原生 IME，候选矩形随光标和横向滚动更新。外部替换和只读等会话变化会重置平台请求。Host 会拒绝与待处理 View 更新前的会话不一致的组合数据；原生事件自身没有会话标识，因此尚不能完整隔离所有迟到事件。真实平台输入验证完成前，不宣称完整中文输入法支持已验收。

## 验证与示例

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
