# Todo: spotlight-ui

- [x] T1 render.rs：圆角绘制（`rounded_span` + span 感知填充）+ 默认 `bg_prompt` 改 `#2e2e2e`
- [x] T2 main.rs：查询为空 → 单行输入栏；非空 → 展开列表；清空 → 收起；非 -b 模式水平居中 + 顶边距 24；空查询 Enter 取消
- [x] T3 回归：README 补一行行为说明；全量 `cargo test` 通过（25 passed）、clippy 干净
# Todo: spacing-comfort

- [x] T1 font.rs + main.rs：行高 16→24px（上下各 4px，Fitts 目标扩大）、PAD 4→12（≥ 圆角半径 10，文字不蹭弧线）、顶边距 24→32（不与屏边贴死）
- [x] T2 render.rs：修复 caret 被行高隐藏的潜在 bug（ab_glyph descent 为负，`baseline+descent` 算出错位/不可见；改为文本块轴对齐 `baseline−descent`），新增 caret 几何单测
- [x] T3 回归：新增 `spacing_follows_eye_comfort_rules` 编译期守卫；测试用 `crate::PAD` 替代字面量 4；全量 `cargo test` 27 passed、clippy 无新增告警
# Todo: prompt-input-gap

- [x] T1 render.rs：`PROMPT_GAP` 常量 + draw() 内 prompt 后补间隙
- [x] T2 caret 差值断言测试；全量 `cargo test`（55 passed）+ clippy 新增告警数为零
# Todo: prompt-badge

- [x] T1 render.rs：`PILL_*` 常量 + `right_cap_inset`；draw() 条带 → 胶囊
- [x] T2 改写两条既有测试（胶囊形状/栏身统一 + 间距差值基数）
- [x] T3 回归：全量 `cargo test`（55 passed）+ clippy 逐 lint diff 无新增
# Todo: prompt-badge-r2

- [x] 颜色 → 0x2e4a5c；胶囊两侧圆角；左缘收口断言；全量测试 55 passed + clippy 一致
# Todo: prompt-outline (E) + 保留 F

- [x] E：`LABEL_ACCENT` 0x5c8494 描边胶囊（内部留灰）；测试改写 55 passed + clippy 一致
- [x] F：左侧强调条方案完整写入 SPEC（未实现）
# Todo: prompt-rail (F)

- [x] E 回滚，F 落地：竖条 4px/高 20/色 0x2e4a5c/文字 24 起；删 cap_inset、measure 转 cfg(test)；测试改写；55 passed + clippy 一致；ASCII 确认
# Todo: perf-hidpi-stream（1/3/9）

- [x] 9 clippy-clean：8 error → 0（`cargo clippy -- -D warnings` exit 0）
- [x] 3 stdin-stream：ItemFeed 后台分批 + sync_items（tick/按键）+ row_capacity；空 stdin 仍 exit 1；`--run` 同步不变
- [x] 1 hidpi-scale：apply_scale（字号随 scale 重载）+ 缓冲×scale + set_buffer_scale + render 几何×scale；行内距按 0.25em 比例
- [x] 回归：57 passed；clippy 0；README 补流式/HiDPI 说明
# Todo: product-round（1/2/4/5/6）

- [x] 1 `-o output`：选屏 + 未知名报错；解析单测
- [x] 2 man page：docs/rmenu.1（groff 渲染通过）+ README 安装说明
- [x] 4 光标闪烁：500ms 切换，输入常亮；caret_visible 参数 + “隐藏时无光标”断言
- [x] 5 滚动指示：scroll_thumb 纯函数 + 位置/长度断言；仅溢出时绘制
- [x] 6 scrolloff：上下各 1 行上下文
- [x] 回归：60 passed；clippy 0
# Todo: launcher-quality（1/2/3/5/6）

- [x] 1 非法 UTF-8 行不再截断（read_into 可测 + 字节流断言）
- [x] 2 Exec 字段码：中段/粘连/`%c`/`%k`/`%%` 全覆盖
- [x] 3 PATH 仅列可执行普通文件（跟随符号链接；临时目录单测）
- [x] 5 `Name[locale]` 本地化优先
- [x] 6 `OnlyShowIn`/`NotShowIn`/`Terminal=true` 过滤
- [x] 回归：64 passed；clippy 0；README/man 同步
# Todo: render-cache（1+4）

- [x] 1 字形缓存：64 行 9.71 → 2.32 ms；HiDPI 64 行 18.15 → 7.75 ms；新缓存复用断言
- [x] 4a 删无效 `#[allow(clippy::assertions_on_constants)]`（测试构建告警消除）
- [x] 4b 删测试 `target/` 写盘与恒真断言
- [x] 4c `-W`/`-l` 边界校验 + 断言
- [x] 回归：66 passed；clippy 0；release 构建通过
# Todo: paint-aa-anim-keywords（1/4/5/6）

- [x] 1 单遍绘制（paint 1.26→0.51 / 4.84→3.88 ms）
- [x] 4 子像素 AA（3 平面逐通道；BGR 镜像断言；Gray 中性断言；1× 启用、HiDPI 灰度）
- [x] 5 高度过渡动画（120 ms 缓出；起始/终点/单调断言）
- [x] 6 `Keywords=` 参与匹配（不影响排序）
- [x] 回归：70 passed；clippy 0；release 通过；README 说明子像素与 BGR 逃生门
