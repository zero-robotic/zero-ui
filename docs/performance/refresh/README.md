# 刷新链路优化与性能对比

这是第一轮历史结果。后续包含 [第二轮整帧复用](../retained-cpu/README.md) 和 [第三轮子树缓存与区域重绘](../regions/README.md)；当前基准程序输出对应最新实现。

测量日期：2026-11-07。基线为 `b67cdf9` 加上计时和同一基准程序，不包含本轮运行时优化。Release 模式、相同工作量下，局部文本与弹出层合成场景的整帧 P50 耗时降低约 39%～40%，主要收益来自避免未变化文本的重复排版。颜色更新没有获得收益；当前仍然全量生成 Scene、全量 CPU 光栅化。

## 架构与改动范围

- **zui-app**：可选的 `RefreshMetrics` / `RefreshPhase`，使用系统时钟；不向 core/api/widgets 引入 std 或日志依赖。原生驱动记录 renderer + presentation submission 的 CPU 墙钟耗时。
- **WidgetHost**：新增显式 `update_batch`。连续 Wake 保留所有模型回调，将动画推进与 IME 收尾合并一次；输入、焦点及生命周期事件作为边界。不会丢弃移动事件，不改变命中、捕获或组合输入的顺序。Winit 当前不自动调用批量 API。
- **zui-core**：仅当失效为 None、视口和字体 epoch 均未变时，跳过托管子节点工厂与协调；仍执行焦点作用域同步和待处理焦点显露。Paint 仍检查托管子节点，以保留虚拟列表滚动的更新语义。
- **Text**：内容/样式变化清空缓存；颜色变化只影响绘制；相同宽度和字体 epoch 复用已准备结果，测量与最终准备可交接同宽结果。最多保存两个 TextLayout，通常准备后只保留一个，不新增全局缓存。布局引擎仍可计算整树，但未变化文本不再重复整形。

依赖方向未改变。布局与文字仍经公共 trait 注入，计时留在装配层，缓存由控件自己管理。仅开发工具 `zui-window-check` 增加直接 CPU 渲染器依赖，以测量真实光栅化；生产层不新增跨层依赖。

**字体服务迁移注意**：已有树更换 TextService 时必须改变 `font_epoch`，或重建树。仅调用 `invalidate(Layout)` 不再强制清除 Text 的有效排版缓存；字体/字体策略变化也应改变 epoch。这一要求已同步到 API 和文字文档。缓存命中不影响最终宽度校验；失败仍阻止新场景发布。

## 计时口径

```rust,ignore
use zui_app::{Application, RefreshPhase};
host.set_profiling(true);
// 原有更新与绘制流程，或主动消费已有事件队列：
let action = host.update_batch(events, metrics, elapsed)?;
let stats = host.take_refresh_metrics();
let layout = stats.phase(RefreshPhase::Layout);
// layout.calls / total / max；stats.layouts 为成功重算几何的次数。
```

阶段为 View、Reconcile、Layout、Input、Animation、Scene、Present。Layout 包含布局、控件准备、托管子节点和焦点收尾，并非仅 Taffy 求解。View/Reconcile 单独计时；Input 包含树内派发，不包含后续应用 message 回调。阶段统计不能直接相加当作完整帧耗时，因为模型 update、IME 等工作不全在这些阶段内。构造 Host 的初始 view 不计入启用后的统计。

基准额外用外层计时测量 update_batch、draw 和 CPU render_into，三者合成整帧耗时。基准的 Present 仅记录 CPU 光栅化；原生驱动的 Present 包含呈现提交，二者不能混为同一指标。GPU 提交的 CPU 耗时也不等于 GPU 执行耗时。默认禁用 profiling 时不读取系统时钟；本次前后都启用计时。

## 环境与场景

- macOS 27.0.1（26A434），arm64；Rust 1.98.1（48a229cea，LLVM 22.1.8）。沙箱未提供 CPU 型号，因此不注明芯片型号。
- 800×480、scale=1，仓库固定 NotoSansSC 字体；Taffy + Parley + Vello CPU，复用像素缓冲。
- 每场景先绘制初帧，再预热 20 帧、采样 120 帧。三轮交替：before/after、after/before、before/after；计时期间不并行编译或运行测试。
- 表格使用各轮 P50/P95 的中位数，阶段均值也按轮取中位数。耗时降低 = `(before - after) / before`，负数表示回退，不是吞吐率提升。
- 每个采样帧在计时区间之外计算像素哈希并累积。全部场景、全部轮次的对应校验和一致；这覆盖绘制输出，输入/模型语义另由回归测试覆盖。

| 场景 | 工作量 |
|---|---|
| local_text | 120 个固定尺寸标签，每帧修改一个标签的文本 |
| color | 同一标签网格，每帧只改变一个标签颜色 |
| popup | 同一网格，每帧交替添加/删除额外弹出层；不是完整菜单交互测试 |
| burst32 | 每帧连续 32 次 Wake，各自更新模型文本，再绘制一帧 |
| pointer32 | 每帧 32 次指针移动，保留每条输入，随后绘制 |
| virtual_idle | 10,000 行虚拟列表、420 高视口，每帧 Wake、绘制，无滚动 |

这些是受控的离屏工作负载。基准主动逐帧绘制，包括静态场景，不意味着原生窗口在空闲时持续刷新。结果不含系统合成、屏幕传输、垂直同步或真实菜单点击到显示的延迟，也不代表无 GPU 嵌入式设备或 MCU 的实测性能。没有测量 GPU 完成时间和端到端 GPU 加速比。

## 结果

完整 [P50/P95 和刷新准备耗时对比](results.md)，原始数据为本目录 `before-1/2/3.csv`、`after-1/2/3.csv`。

| 行为计数（每轮 120 帧） | 优化前 | 优化后 |
|---|---:|---:|
| 局部文本：排版调用 | 28,800 | 120 |
| 弹出层：排版调用 | 29,100 | 60 |
| burst32：动画阶段调用 | 3,960 | 240 |
| burst32：View 重建次数 | 120 | 120 |
| burst32：已合并重建请求 | 3,720 | 3,720 |
| pointer32：输入事件数 | 3,840 | 3,840 |

burst32 的 240 次动画包含批处理收尾 120 次及 draw 120 次。View 重建在基线中已经合并，这部分没有新增收益。颜色场景整帧 P50 回退约 0.5%，刷新准备均值回退约 4.2%（约 4µs）；不宣称该场景提升，也不据此推断统计显著性。光栅化仍约为标签场景每帧 1.16ms，因此准备阶段约 81% 的降幅不会转化为整帧相同降幅。

## 复现

当前版本：

```sh
cargo build -p zui-window-check --example refresh-bench --release --locked --offline
target/release/examples/refresh-bench 120 > after.csv
```

基线重建：`baseline.patch` 仅在 `b67cdf9` 添加计时、默认逐条 update_batch 和同一基准，不包含运行时优化。已验证补丁可应用；可在临时目录导出，避免影响当前工作区（命令在仓库根目录执行）：

```sh
benchmark_root="$PWD"
benchmark_baseline_dir="$(mktemp -d /tmp/zui-refresh-baseline.XXXXXX)"
git archive b67cdf9 | tar -x -C "$benchmark_baseline_dir"
(cd "$benchmark_baseline_dir" && git apply "$benchmark_root/docs/performance/refresh/baseline.patch")
cargo build --manifest-path "$benchmark_baseline_dir/Cargo.toml" \
  -p zui-window-check --example refresh-bench --release --locked --offline
"$benchmark_baseline_dir/target/release/examples/refresh-bench" 120 > before.csv
```

交替重复三轮并保存 CSV 后：

```sh
python3 tools/compare-refresh.py \
  --before docs/performance/refresh/before-{1,2,3}.csv \
  --after docs/performance/refresh/after-{1,2,3}.csv
```

比较脚本先检查场景集合、样本数和输出校验和，再生成 Markdown 表格。复测结果受硬件、电源和系统负载影响，不应要求绝对数值相同。

## 验证与后续边界

已覆盖：批量 Wake 不丢模型更新、最早唤醒、退出截断、输入边界提交新几何、不丢输入、空批次、计时开关与重置；干净树跳过工厂且 Paint/视口变化仍刷新；文本宽度、内容、样式、字体 epoch 失效、排版失败后恢复和 CPU 像素一致性。

本轮保持全树 Layout 失效标记、完整 Scene 重建、CPU/GPU 全量渲染。下一阶段若继续优化，应单独测量布局子树失效、Scene 缓存及 CPU 脏矩形，并先定义透明叠加、裁剪和旧区域恢复规则。本轮未添加通用全局缓存、跨层渲染状态或后台线程协议。

本轮验证命令全部通过：

```sh
cargo test -p zui-core -p zui-window-check -p zui-render-check --locked --offline
cargo clippy -p zui-app -p zui-core -p zui-widgets -p zui-window-check \
  -p zui-render-check --all-targets --all-features --locked --offline -- -D warnings
python3 tools/check-dependencies.py
python3 tools/check-mcu.py
```

依赖检查覆盖 26 个包及 CPU/GPU 特性隔离；MCU 检查编译 12 个可移植库到 `thumbv7em-none-eabihf`，不代表固件链接、设备渲染或内存预算已验证。全特性 Clippy 验证 GPU 路径编译，本轮未运行真实 GPU 性能测试。
