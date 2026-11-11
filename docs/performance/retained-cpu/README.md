# 第二轮：复用完全未变化的 CPU 帧

这是整帧复用阶段的历史报告。当前实现已进一步支持子树命令缓存和 CPU/GPU 区域重绘，见 [第三轮测量](../regions/README.md) 与 [当前机制](../../region-rendering.md)。

日期：2026-11-07。本轮基线是上一轮刷新计时、更新合并和文本缓存优化完成后的版本，**不是**最初的 `b67cdf9`。本轮关注 CPU 光栅化，保留上一轮优化及现有模块依赖。

## 实现和边界

`zui-render-cpu::RetainedCpuRenderer` 组合原有 CpuRenderer、一个 PixelBuffer 和上一帧成功渲染的 Scene。它只在以下内容全部相等时返回已有像素：

- 完整绘制命令，包括顺序、路径、裁剪、变换、颜色、字形、字体和图片资源身份。
- 物理尺寸、scale、背景色。

使用完整相等比较，不用哈希代替正确性检查。字体和图片资源不可变，新的资源身份自然导致重绘。缓存只保存最近一帧，A/B 交替切换不会命中。失败前清除旧缓存标记，失败后重试必须重绘；`invalidate()` 可显式强制重绘。

输出只以 `&PixelBuffer` 借用，调用方不能修改内部像素。窗口曝光或恢复时仍执行 surface.present，因此不会以“内容未变”为理由跳过显示提交。原生 CPU Presenter 已使用该适配器，不只是在基准中启用。

**内存和动态开销**：相较原生 Presenter 原本的缓冲方案，没有增加第二份像素缓冲，但多保存一个 Scene。路径、字形等命令数据需要克隆；字体和图片底层资源共享。保留的场景受现有 RenderLimits 限制，这不等于严格的总内存字节上限。动态帧有比较/克隆成本，需结合业务决定是否采用；自行装配的嵌入式显示驱动仍可直接使用原有 CpuRenderer::render_into，不保留场景副本。

core、widgets、paint 协议及 GPU 渲染器没有本轮改动；渲染器不感知窗口或控件。原生 CPU Presenter 的缓冲所有权下移到 CPU 适配器，单向依赖保持不变。这里是整帧复用，**不是脏矩形或局部重绘**；只要场景改变，仍全量光栅化。

```rust,ignore
let mut renderer = zui_render_cpu::RetainedCpuRenderer::new(limits);
let pixels = renderer.render(&scene, frame)?;
display.present(pixels)?; // 仍提交给显示设备
let reused = renderer.reused_frame();
```

## 测量

环境与上一轮一致：macOS 27.0.1 arm64，Rust 1.98.1，Release，800×480、scale=1、固定仓库字体，Taffy/Parley/Vello CPU。每场景预热 20 帧、采样 120 帧；交替执行三轮 before/after、after/before、before/after，测量期间不编译或测试。

基线使用原有 CpuRenderer::render_into 和复用的调用方 PixelBuffer；优化后使用原生 Presenter 同款 RetainedCpuRenderer。业务更新、输入、View、布局和 Scene 生成工作量不变，首次分配均在预热期。仍对每个采样帧计算完整像素哈希，六个场景、三轮的累计校验和全部一致。

[完整 P50/P95 与阶段结果](results.md)，原始 CSV 保存在本目录。P50 为各轮 P50 的中位数，降幅按耗时计算：

| 场景 | 优化前 µs | 优化后 µs | 耗时降低 |
|---|---:|---:|---:|
| pointer32 | 1426.5 | 287.0 | 79.9% |
| virtual_idle | 166.9 | 6.8 | 95.9% |
| local_text | 1325.3 | 1337.6 | -0.9% |
| color | 1211.6 | 1210.8 | 0.1% |
| popup | 1321.0 | 1336.5 | -1.2% |
| burst32 | 1303.8 | 1313.7 | -0.8% |

pointer32 每帧派发 32 次位于标签区域之外的指针移动，画面不变，全部输入仍处理；virtual_idle 是 10,000 行虚拟列表的静态重复绘制。它们不是悬停高亮、拖动或列表滚动场景。动态文本、颜色及弹出层变化会导致重绘，不能将静态场景收益推广到这些交互。负降幅如实保留；未做统计显著性推断。

基准主动请求静态重复绘制，不意味着窗口空闲时持续刷新。数字包含更新、Scene 生成与 CPU 像素输出，不含系统显示提交、垂直同步或显示器延迟；也不是 GPU 或嵌入式实机测量。刷新准备阶段的小幅波动不是本轮新增算法的收益。

## 复现

当前版本：

```sh
cargo build -p zui-window-check --example refresh-bench --release --locked --offline
target/release/examples/refresh-bench 120 > after.csv
```

`baseline.patch` 对 `b67cdf9` 加上上一轮所有相关运行时优化、计时与旧版基准，恢复本轮优化前状态。已验证补丁可应用。仓库根目录执行：

```sh
benchmark_root="$PWD"
benchmark_baseline_dir="$(mktemp -d /tmp/zui-retained-baseline.XXXXXX)"
git archive b67cdf9 | tar -x -C "$benchmark_baseline_dir"
(cd "$benchmark_baseline_dir" && git apply "$benchmark_root/docs/performance/retained-cpu/baseline.patch")
cargo build --manifest-path "$benchmark_baseline_dir/Cargo.toml" \
  -p zui-window-check --example refresh-bench --release --locked --offline
"$benchmark_baseline_dir/target/release/examples/refresh-bench" 120 > before.csv
```

完成三轮交替运行后，用同一比较脚本验证校验和并生成表格：

```sh
python3 tools/compare-refresh.py \
  --before docs/performance/retained-cpu/before-{1,2,3}.csv \
  --after docs/performance/retained-cpu/after-{1,2,3}.csv
```

## 验证

新增 CPU 测试比较复用路径与直接渲染的像素，覆盖独立构建的相同命令、透明混合与裁剪、变换/颜色/资源替换、清空场景、显式失效、尺寸/缩放/背景变化、非法输入与预算错误后的恢复。动态字形变化由六场景基准的逐帧像素对比覆盖。

本轮不改 portable crate；MCU 的能力边界仍以 [可移植性文档](../../mcu-portability.md) 为准。

本轮执行通过：

```sh
cargo test -p zui-render-check --test retained_cpu --test rendering --locked --offline
cargo test -p zui-app -p zui-window-check --locked --offline
cargo clippy -p zui-render-cpu -p zui-app -p zui-window-check -p zui-render-check \
  --all-targets --all-features --locked --offline -- -D warnings
python3 tools/check-dependencies.py
cargo fmt --all --check
git diff --check
```

共 54 项针对性测试通过，26 个包的依赖方向及 CPU/GPU 特性隔离检查通过。本轮未进行真实窗口交互或 GPU 性能测量。
