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
# Plan: prompt-input-gap

依赖：单文件 render.rs，无依赖、无并行任务。

1. render.rs：`PROMPT_GAP = 8.0` 常量（8px 网格）+ `draw()` 内 prompt 绘制后 `x += PROMPT_GAP`；query/caret 自然右移，query `max_w` 相应收窄。
2. caret 差值断言（无/有 prompt 两帧，差值 = prompt 宽 + 8±1，f32 舍入容差）+ 全量回归。

风险：

- f32 求和舍入 → 断言区间 8..=9
- 既有 label strip 边界测试用空 query，不受间隙影响

验证检查点：T2 后 `cargo test` 全绿 + clippy 干净。
# Plan: prompt-badge

依赖：render.rs 单文件，无依赖、无并行任务。

1. 常量：`PILL_VPAD = 2`、`PILL_HPAD = 8`；新增 `right_cap_inset`（右缘圆帽，镜像 `rounded_span` 角落数学）。
2. draw()：全高平直条带 → 紧凑胶囊（高 20、垂直居中、左贴 PAD、右端半径=高/2 半圆收口）；prompt 文字内移 `PILL_HPAD`；输入首字符 = 胶囊右缘 + `PROMPT_GAP`(8)。
3. 测试改写：`prompt_label_has_distinct_background_from_input_area` → 胶囊形状 + 栏身统一 `bg_prompt` 断言；`prompt_gap_separates_label_from_input` → 差值基数改为 `2×PILL_HPAD + PROMPT_GAP`。

风险：

- 像素量化：圆帽 inset 与 `rounded_span` 同公式（py+0.5）逐行取整
- 两处测试像素位随胶囊几何变化，同步改写

验证检查点：T3 全量 `cargo test` + clippy 逐 lint diff 无新增。
# Plan: prompt-badge-r2

确认范围：不做「固定槽位/坐标一致」（否决）；只做三件：

1. 颜色 0x004560 → 0x2e4a5c（灰蓝，与灰栏调和、不撞选中蓝）
2. 胶囊两侧半圆收口（`cap_inset` 双侧复用，左缘同样内收）
3. 栏身灰同步不变（0x2e，已达成）

测试：左缘顶行内收断言新增；caret 差值断言不受影响（文字起点/胶囊右缘未变）。
验证检查点：全量 `cargo test` 55 passed；clippy lint 类型与父提交一致；ASCII 拓扑确认胶囊对称。
