# Todo: spotlight-ui

- [x] T1 render.rs：圆角绘制（`rounded_span` + span 感知填充）+ 默认 `bg_prompt` 改 `#2e2e2e`
- [x] T2 main.rs：查询为空 → 单行输入栏；非空 → 展开列表；清空 → 收起；非 -b 模式水平居中 + 顶边距 24；空查询 Enter 取消
- [x] T3 回归：README 补一行行为说明；全量 `cargo test` 通过（25 passed）、clippy 干净
# Todo: spacing-comfort

- [x] T1 font.rs + main.rs：行高 16→24px（上下各 4px，Fitts 目标扩大）、PAD 4→12（≥ 圆角半径 10，文字不蹭弧线）、顶边距 24→32（不与屏边贴死）
- [x] T2 render.rs：修复 caret 被行高隐藏的潜在 bug（ab_glyph descent 为负，`baseline+descent` 算出错位/不可见；改为文本块轴对齐 `baseline−descent`），新增 caret 几何单测
- [x] T3 回归：新增 `spacing_follows_eye_comfort_rules` 编译期守卫；测试用 `crate::PAD` 替代字面量 4；全量 `cargo test` 27 passed、clippy 无新增告警
