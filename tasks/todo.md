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
# Todo: contrast-viewport-tryexec（1/3/4 + 5 部分）

- [x] 1 选中行 `bg_sel #0b6285` + 白字（6.77:1）；WCAG 回归测试
- [x] 3 视口纯函数 `viewport_top` + 7 条断言；修掉陈旧 offset 越界风险
- [x] 4 `TryExec` 缺失即隐藏（绝对路径/`$PATH`/符号链接）
- [x] 5a 抽出 `anim.rs`、`feed.rs`（`pub(crate)` + 测试随行）
- [ ] 5b 抽出 `opts.rs`、`menu.rs`（需字段放权 + 拆测试辅助，下一轮）
- [x] 回归：73 passed；clippy 0；fmt clean
# Todo: idle-wakeups（1/2/5）

- [x] 1 按需睡眠：空闲唤醒 62.5 → ≤2 次/秒；`loop_timeout` 断言（busy/空闲/过期）
- [x] 2 stdin 为 tty 时提示（行为与退出码不变）
- [x] 5 `draw_text` 裁剪边界断言（不越界写）
- [x] 回归：75 passed；clippy 0；fmt clean
# Todo: run-item-stream（1/2/5）

- [x] T1 `feed.rs`：`spawn_with(producer)` + `spawn_worker` 共用线程外壳（`spawn()` 语义不变）+ 2 条断言
- [x] T2 `main.rs`：`--run` 分支 → `ItemFeed::spawn_with(|| desktop::merged(...))`；删首帧前同步 `no_items` 退出
- [x] T3 回归：81 passed；clippy `-D warnings` 0；fmt clean；README 补流式说明
     实测（PATH 复制 500 份 ≈ 扫描 0.84 s）：首帧仍在 0.001 s，条目帧 0.840 s——扫描已不在关键路径
# Todo: single-instance（1/2）

- [x] T1 `control.rs`：`claim()`（Owner/Dismissed/Disabled）+ `socket_path_from` 纯函数 + 4 条 std 单测
- [x] T2 `main.rs`：`parse_opts` 后 claim（Dismissed → exit 1，早于字体加载与 Wayland 连接）；`WaylandSource` 后 `insert_source(Generic)`
- [x] T3 验收：A→1 实例 / B exit 1 且 `WAYLAND_DEBUG` 0 行 / 陈旧 socket 回收 / 无 XDG_RUNTIME_DIR 静默降级（2 实例并存）；
     回归 81 passed + clippy 0 + fmt clean；README 补单实例一行
# Todo: UI-001 panel polish

- [x] 圆角 12 px、内容边距 16 px
- [x] 选中项 8 px 内缩 / 8 px 圆角，输入区加入 1 px 分隔线
- [x] 补充分隔线、选中项形状和 HiDPI 像素断言
- [x] 回归：82 passed；clippy 0；fmt clean
# Todo: UI-002 full-row selection

- [x] 选中背景恢复整行覆盖，删除内缩/圆角常量
- [x] 更新全宽覆盖与外层圆角裁剪断言
- [x] 回归：82 passed；clippy 0；fmt clean
# Todo: startup-font-latency

- [x] P1 serve-stale 字体链 + 后台重扫（`cache_use` 断言 + `read_chain` 断言）
- [x] P2 `-f FAMILY` 解析结果复用 font-chain 缓存（`family_file_name` 断言）
- [x] P3 字体加载线程化 + 精确临时 row_h 先提交 + 首帧 join
      SPEC：`openspec/specs/SPEC-startup-font-overlap.md`；`row_h_for`/`spec_size` 纯函数 + 不变量断言
      实测：默认 21–32 ms（同批 `-f FILE` 18–28 ms，改前两者差 ~10–20 ms）；`-f FAMILY` 22–32 ms；
      「提交 → 首次 attach」间隙 4.4–11.5 ms（改前 2.1–2.7 ms）→ 证明提交先于 join；
      `-f "Noto Sans Mono 20px"` 首帧 set_size 与提交一致（640,30），默认 24/24，无补正；
      scale 2：`set_buffer_scale(2)` 生效、无 panic、set_size 24→12（缓出起点，与父提交逐行相同）；
      `-f "No Such Font"` / `/nonexistent` → exit 1 + 原文案；空 stdin → `no items` exit 1；单实例仍 dismiss
- [x] 附带发现（未修，见 SPEC）：`-o NAME` 完全不可用 —— `Connection::roundtrip()` 不 dispatch
      `registry_queue_init` 的队列，`OutputState::info()` 恒为 None；属独立改动
- [x] P4 链路改 mono 主字体 + CJK/图标惰性
      SPEC：`openspec/specs/SPEC-font-chain-lazy.md`；`FaceState` + `face(i)` 按需读 + 失败不重试
      实测：默认 17–31 ms（中位 20）与 `-f FILE` 15–20（中位 19）齐平；改前中位差 ~3 ms；
      `-p 中文` 首帧 +4–8 ms（惰性读 27MB TTC）；CJK/Nerd 渲染断言全绿
      发现并修：缓存缺「策略号」→ 旧链（CJK 在前）会被当 fresh 而静默失效，新增 `p\t2` 行参与键校验
- [x] `-o NAME` 修复（附带发现）
      SPEC：`openspec/specs/SPEC-output-select-fix.md`；App 先构造（`layer: None`）→ `event_queue.roundtrip(&mut app)`
      → 查名 → 再建 layer surface + 装配 loop source
      实测：`-o HEADLESS-1` → `enter(wl_output@8)`（= HEADLESS-1）、`-o HEADLESS-2` → `enter(wl_output@7)`；
      `-o nope` → exit 1 原文案；不带 `-o` 仅 0 次额外 sync，带 `-o` 恰 1 次
- [x] 回归：87 passed（+3 断言）；fmt clean；clippy 无新增（font.rs/main.rs 0 条；render.rs 19 条为存量）
      实测（headless sway）：人为把 font-chain 键改脏 → 首帧 36–62 ms（改前同条件 2927 ms），
      长活实例 1–2 s 后缓存被后台重扫写回正确键；
      `-f FAMILY` 62–76 ms → 35–45 ms（与默认路径持平）；默认路径 26–50 ms 不变
