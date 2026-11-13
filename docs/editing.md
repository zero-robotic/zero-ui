# Unicode 编辑状态

状态：`zui_widgets::edit` 已实现与界面无关的文本、选择区、逻辑字素移动及有限撤销历史。已由 TextInput 接入可见编辑；编辑模块自身不处理系统输入法、剪贴板或可视光标。

## 模块边界

原 `zui-edit` crate 已合入 `zui-widgets`，公开入口改为 `zui_widgets::edit`，原有算法测试迁至 `crates/zui-widgets/tests/editing.rs`。无需额外 feature；该算法也是文本输入控件的基础能力。

`zui_widgets::edit` 使用 `no_std + alloc`，只依赖固定版本 `unicode-segmentation 1.13.3`，算法模块不引用输入设备、字体、具体控件或渲染后端；其宿主 crate 为 zui-widgets。它负责已提交文本的编辑规则；上层负责将键盘/IME/剪贴板行为转换成编辑操作。第三方分段类型不出现在公共接口中。

布局服务负责坐标命中、字形簇与字节偏移映射、双向文字的视觉顺序和选择区域几何。编辑模块只处理逻辑文本顺序，不得反向查询布局或控件树。后续输入框和多行编辑器可依赖此模块，但不能把窗口事件和控件状态移入其中。

## 公共入口

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

## 撤销和资源限制

一次文本替换是一个撤销单元，记录起点、删除/插入的 UTF-8 字符串，以及操作前后的选择区，不保存完整文档快照。连续输入尚未合并为词或时间分组。移动光标和选择区本身不增加历史；替换成相同文本只调整选择区，不打断重做链。

EditLimits 分别限制当前文本字节数、撤销和重做合计的字符串载荷字节数、历史条数。默认文本与历史载荷各 1 MiB，最多 256 条历史。历史字节数不包括集合和记录结构的分配开销；条数另行限制。边界扫描目前是线性算法，尚未提供 Rope 或大文档索引。

超出文本预算的操作返回 BudgetExceeded，原文、选择和历史均保持不变。历史预算不足时淘汰最旧撤销记录；单次编辑本身无法纳入历史或历史条数为零时，编辑照常提交并清空旧历史，避免跨越未记录操作后错误应用旧差量。撤销/重做仅在两个队列间移动记录，不扩大历史载荷。

## 已分配容量与显式历史释放

`TextBuffer::memory_usage()` 返回 EditMemoryUsage，分别报告当前文本长度、文本缓冲容量、历史字符串载荷长度及历史总缓冲容量。历史容量包括 undo/redo 的记录数组，以及每条记录插入/删除字符串的容量；clear_history 清空载荷后保留的数组容量仍会计入。allocated_bytes 合计文本与历史容量，不重复合计长度字段。

统计为 O(H)，H 为历史条数，不放入每帧绘制；不包含对象自身、分配器元数据、排版、模型副本或进程其他资源，因此不是 RSS 或整机内存上限。EditLimits 原有载荷限制语义不变，不能据此声称它是严格分配容量限制。

`release_history()` 丢弃撤销/重做并释放两个记录数组，保留文本、当前选择区和文本缓冲容量，返回释放的历史缓冲容量。与保留容器容量的 clear_history 区别明确。重复释放返回 0；之后可以继续编辑并建立新历史。释放内存由分配器接收，不保证操作系统 RSS 立即下降。

## 验证与后续

```sh
cargo test -p zui-widgets --test editing --locked
cargo clippy -p zui-widgets --all-targets --locked
```

六项回归测试覆盖中文、组合重音、带肤色的 ZWJ emoji、旗帜、CRLF 与印度文字字素；反向选择、折叠、替换、字符合并、无效偏移、预算失败原子性、历史淘汰和分支重做。边界组合测试遍历多个文本的选择端点与插入片段，逐次验证光标合法性及撤销/重做恢复。

词级回归另覆盖中英文、标点、组合重音、emoji 和纯空白，遍历多个文本的全部字素边界验证双向删除、撤销与重做。词级操作的端点同样保证是完整字素边界；非扩展移动遇到已有选区时先折叠到相应逻辑边缘。

文字服务现已提供可选交互排版快照，支持光标、命中、选择区域与视觉左右移动，详见 [文字服务与字形绘制](text-design.md)。[单行输入框基础](text-input.md) 已组合编辑状态与这些几何契约，控件层负责只读和单行过滤。IME 预编辑保持为独立临时状态，提交后才写入缓冲区；逻辑字素/词级移动不能冒充双向文字的视觉左右移动。文本剪贴板已通过上层服务注入接入，编辑模块不执行系统 I/O，详见 [文本剪贴板](clipboard.md)。视觉词级与行导航、撤销合并、密码策略、业务输入校验与无障碍仍待后续实现。
