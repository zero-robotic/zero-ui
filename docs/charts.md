# 图表模块与 Sparkline

zui-extras::charts 是 zui-extras 中由 charts feature 按需启用的 no_std + alloc 模块，依赖 zui-api、zui-types、zui-paint 和 zui-theme；不依赖核心、标准控件、窗口、文字引擎或 Vello。应用将图表作为普通 View 放入容器；CPU/GPU 使用相同 Painter 路径描述。no_std 编译不代表已经完成 MCU 内存、显示驱动或硬件验收。

## 迷你趋势图

```rust,ignore
let series = SparklineSeries::new(vec![Some(12.0), Some(18.0), None, Some(15.0)])?;
let mut chart = SparklineView::themed(&theme, series);
chart.range = Some(ChartRange::new(0.0, 20.0)?);
chart.style.natural_size = Size::new(240.0, 80.0);
let view = View::new(chart);
```

SparklineSeries 拥有共享不可变样本与缓存的极值；Clone 不复制样本数组。样本等间隔排列，None 保留其位置并断开路径，不跨缺测区连线或填充。构造拒绝 NaN/无穷值，并返回出错索引。最多 65,536 个槽位，超出返回 TooManySamples；大数据应在应用或后续聚合层显式降采样，不静默丢点。

range=None 使用所有有效样本的极值。常量序列垂直居中；空序列或全缺测不绘图。单个样本水平居中，连续段中的孤立样本绘制线宽大小的方形标记。固定 ChartRange 要求有限且严格递增，超出范围的样本饱和到上下边界。归一化兼容跨度溢出的极大端点与可表示的极小跨度，不将非有限坐标交给渲染器。

SparklineStyle 配置线色、线宽、可选面积色和自然尺寸。默认 160×48、2 像素逻辑线宽及主题强调色；面积填至绘图区底部，并非数值零基线。折线各缺测段分别填充。所有绘制裁剪到内容框，正常线宽在四周留半线宽，过大线宽仍受裁剪。自然尺寸和线宽必须有限、为正且不大于 f32::MAX，非法样式在测量/准备阶段报错，修正后可重新布局恢复。

数据、范围和配色更新只请求 Paint；自然尺寸变化或非法样式进入/恢复请求 Layout。绘图时间与样本数线性相关，每次 paint 构建路径；尚未缓存几何或增量更新。图表为被动展示，不参与命中或键盘焦点。

## 连续采样与有界历史

SparklineHistory 在应用层维护固定容量的等间隔历史，容量范围为 1..=65,536。构造时预留环形存储，后续 push/extend 在该存储内写入；满容量先淘汰最旧槽位，None 同样占一个采样位置。

```rust,ignore
let mut history = SparklineHistory::new(240)?;
// 数据到达时写入，不必每个样本都重建界面。
history.extend(&[Some(12.0), None, Some(15.0)])?;
// 应用决定刷新时机，然后更新 SparklineView.series。
let series = history.snapshot();
```

push 返回被淘汰槽位：None 表示尚未满，Some(None) 表示淘汰缺测，Some(Some(v)) 表示淘汰有效值。extend 先验证整个切片，NaN/无穷值返回批内索引并保持数据及缓存不变；返回淘汰数量，包含同一批中被后续样本挤出的槽位。不会自动补齐时间、排序或插值，真实缺测应由应用显式写入 None。

snapshot 首次或数据变更后复制当前有效历史并重算极值，时间/额外空间为 O(N)；未变更时返回缓存的共享快照，时间 O(1)。旧快照不可变，继续可用。clear 清空历史并保留环形容量，旧快照不受影响。

容量限制针对当前历史槽位，不是整个应用内存硬上限。应用保留的旧快照、当前缓存和构建期间的临时数组都需要额外内存；构造的 AllocationFailed 仅覆盖环形存储预留，快照分配仍使用全局分配器。每个样本 push 为 O(1)，批量写入为 O(M)，绘图仍是全量路径构建。模块不创建线程、定时器或帧循环；采样与显示刷新频率由应用控制。

新增三项历史单元测试覆盖不同容量下的重复环绕、槽位淘汰、存储容量不增长、批量错误原子性、快照缓存/不可变和极值淘汰重算。CPU 集成测试验证历史更新只触发 Paint、图像随新快照改变且旧快照可以重现原图。

## 预览和验证

```sh
cargo run -p zui-render-check --locked --offline -- cpu artifacts/sparkline-cpu.png sparkline
cargo test -p zui-extras --no-default-features --features charts --locked --offline
cargo test -p zui-render-check --test sparkline --locked --offline
# 需要可用 GPU
cargo test -p zui-render-check --features gpu --test sparkline --locked --offline -- --ignored
```

两项数据单元测试覆盖极端值、极小跨度、常量、缺测和输入校验。CPU/GPU 像素测试均实际运行通过，验证 1×/2× 断线与填充、空数据清屏、单点、Paint 更新，以及非法样式修正。CPU 预览已目视确认三种趋势状态。图表依赖策略纳入 22 包检查，关闭默认特性编译通过。

## 后续范围

当前 Sparkline 是等间隔、单序列、无坐标轴的基础趋势控件。非等间隔横轴、多序列、坐标轴/刻度、图例、提示、选择/缩放、数据聚合、增量几何更新、动画与无障碍描述仍待实现；不将当前版本视为完整折线图或整个图表体系已完成。
