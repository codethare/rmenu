# Plan: spotlight-ui

依赖顺序与并行度：

1. render.rs（渲染器）——圆角填充 + `rounded_span` 辅助 + 默认色微调。无依赖，可先行。
2. main.rs（布局/交互）——初始单行高度、查询驱动展开/收起、基于输出逻辑宽度的水平居中（`OutputState::info().logical_size`）、空查询 Enter 取消。依赖 1（默认色）但不阻塞。
3. 回归验证——全量 `cargo test`；README 补一行；可选 clippy。

风险：

- 首个 configure 时 `surface_enter` 可能未到，输出宽度未知 → 回落左边距 0，`surface_enter` 后再设边距重排（可能一帧跳变，可接受）。
- 圆角无抗锯齿（逐像素阶梯），v1 接受，需要时再做覆盖度 AA。

验证检查点：T1 后 render 单测绿；T2 后 main 单测绿 + 手工在合成器上看观感。
# Plan: spacing-comfort

依据（人眼舒适 / 心理学要点）：

1. 行高 ≈1.5× 字号（16→24px）：文字不拥挤、选中条成易瞄准目标（Fitts 定律；WCAG/排版经验行距 1.4–1.6×）。
2. 内容内边距 ≥ 圆角半径（4→12px）：文字不贴近圆角弧线；输入栏与列表文字共用同一左对齐轴线（格式塔连续性）。
3. 屏边距（24→32px）：面板不“贴死”屏边；间距统一 8px 网格节奏（8/12/24/32）。
4. caret 修复：ab_glyph `descent_px` 为负，旧代码 `baseline+descent` 算出的 caret 顶 < 行高被守卫吞掉（row_h=32 时恰好不触发）；改为文本块轴对齐。
