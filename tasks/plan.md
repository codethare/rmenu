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
# Plan: perf-hidpi-stream（1/3/9）

依赖顺序：clippy-clean → stdin-stream → hidpi-scale（先清零告警，后两件改动才有 `-D warnings` 可用的门禁）。

1. clippy-clean：desktop `sort_by_cached_key`、font/main 四处 let-chain、stdin `map_while(Result::ok)`、`rect`/`draw_text` 加 too_many_arguments allow → 8 error → 0
2. stdin-stream：`ItemFeed`（Mutex 队列 + AtomicBool EOF）后台线程按 256 行分批；主循环每 tick + 每次按键 `sync_items()`（无需新事件源，现有 16ms 轮询足够）；池容量改为 `row_capacity(lines)`；`no_items` 保持 exit 1 契约；`--run` 仍同步
3. hidpi-scale：`OutputInfo.scale_factor` → `apply_scale()`（重载字号 `FONT_SIZE*scale`）；`draw()` 缓冲=逻辑×scale、`set_buffer_scale`、damage 用缓冲像素；`render::draw` 增 scale 参数，内部几何常量×scale；`font.rs` 行内距改为按字号比例（0.25em），使 1.5× 行高在任意 scale 成立

风险：

- 流式下输入前期无项目：Enter 回显输入（dmenu 契约）保持不变
- 空 stdin 会闪现一帧面板再退出（退出码/stderr 不变）
- 分数缩放无 fractional-scale 绑定 → 按整数 scale，已写入 README 已知限制
- 行内距改比例式：自定义字号（如 20px）行高 28 → 30，属 1.5× 规则的一致化

验证检查点：`cargo test` 57 passed；`cargo clippy -- -D warnings` exit 0（存量 8 条清零）。
# Plan: product-round（1/2/4/5/6）

1. output-select：Opts.output + `-o` 解析；开局先建 OutputState → `conn.roundtrip()` → 按 `info.name` 查找 → 传入 `create_layer_surface(..., Some(&out))`；未知名 exit 1；该 OutputState 移入 App（避免双份）
2. man-page：`docs/rmenu.1`（纯 roff，本地 `groff -man` 可校验）+ README 安装说明
3. caret-blink：App.blink + next_blink(Instant)；主循环 `tick()` 500ms 切换；`on_key` → `wake_caret()` 保持常亮；`render::draw` 增 `caret_visible`
4. scroll-indicator：`render::scroll_thumb` 纯函数（长度∝visible/total，位置∝top/max_top）；由 main 在 render::draw 后用 `rect` 绘制（避免再增 draw 参数）
5. scrolloff：draw() 滚动逻辑改为上下各留 `SCROLLOFF=1` 行；仅在 `total > visible` 时弹性滚动

风险：

- `-o` 需要一次额外 roundtrip 才能拿到 output 名（合成器才发 name）
- 闪烁引入 0.5s 周期重绘（每 500ms 一帧，可忽略）
- 指示条用 label_accent，与选中蓝色块有重叠区域（位于右侧内边距内，文字不重叠）

验证检查点：`cargo test` 60 passed；`cargo clippy -- -D warnings` exit 0；`groff -man -Tutf8 docs/rmenu.1` 无 stderr。
# Plan: launcher-quality（1/2/3/5/6）

1. stdin-badline：`ItemFeed::spawn` 拆出 `read_into(impl BufRead)`（同步可测），`for line in reader.lines()` + `let Ok(line) = line else { continue }`；测试直接喂 `&b"good\n\xff bad\nlast\n"[..]`
2. exec-fieldcodes：`clean_exec(exec, name, desktop_file)` 按字符扫描替换/丢弃字段码（旧实现只 `rposition` 剥尾部）
3. path-filter：`path_commands_in(path)` 可测；过滤条件 = `fs::metadata`（**跟随符号链接**，`DirEntry::metadata` 是 lstat，会把 `/usr/bin/sh → bash` 误判为非普通文件）+ `is_file` + `mode & 0o111`
4. name-locale：`locale_tags()`（`LC_ALL`→`LC_MESSAGES`→`LANG`，剥 `.UTF-8`，精度递减）+ `pick_name(tags, localized, plain)`
5. entry-visibility：`shown_on(only, not, current)`；`XDG_CURRENT_DESKTOP` 为空则不过滤；`Terminal=true` 直接跳过

风险：

- PATH 过滤会减少条目数：符号链接必须跟随，否则 `/usr/bin/sh` 之类全被滤掉（已踩到，测试抳住）
- `Terminal=true` 跳过意味着 htop 类条目不再可用——待定 `$TERMINAL` 支持
- `sh -c` 与字节保真未动（属契约变更，见 spec Open Questions）

验证检查点：64 passed；clippy 0；groff 无 stderr。
# Plan: render-cache（1+4）

1. glyph-cache：`font.rs` 新增 `GlyphBitmap`/`GlyphKey` + `MenuFont.glyphs: RefCell<HashMap<..>>`（缓存随字体重载失效）；`render.rs::draw_text` 改为「按 (面索引, gid, 亚像素 x/y 4 桶) 取缓存，miss 时在**规范笔位**（只留亚像素）栅格化，再按整像素位移 blit」；上限 8192 条，超限清空
2. cli-bounds：`valid_width`(1..=16384) / `valid_lines`(<=1000，0=自动) 纯函数 + 解析处校验；断言覆盖边界
3. test-hygiene：删两处无效 `#[allow(clippy::assertions_on_constants)]`；删 `renders_frame_with_cjk_text` 的 `target/` 写盘与恒真断言；新增缓存复用断言

风险与踩坑：

- `px_bounds()` 是 **f32** 矩形（原代码也 `as i32`）；`Outline::draw` 按 `width()×height()` 迭代 → 宽高与 min 必须同源单位（已在注释里标住）
- 新增常量插到了 `#[allow(too_many_arguments)]` 与函数之间 → 属性变成修饰 const，clippy 报 9/7（已修；注释/属性必须紧跟函数）
- 亚像素桶让位图可缓存但保留亚像素定位；桶数 4 是精度/缓存体积的折中

验证检查点：66 passed；clippy 0；基准对比 64 行 9.71 → 2.32 ms、HiDPI 18.15 → 7.75 ms。
# Plan: paint-aa-anim-keywords（1/4/5/6）

1. one-pass-paint（render.rs）：删 `fill()`（全屏写）与冗余的第二遍，改按行分三段写（左透明/背景/右透明）；slot 可能换，角外像素必须显式写透明
2. subpixel-aa（font.rs + render.rs）：`GlyphBitmap.planes: Vec<GlyphPlane>`（1 面=灰度，3 面=子像素）；miss 时按 0/⅓/⅔ px 位移栅格化三面；`blit_plane(chan)` 逐通道混合；`Subpixel{Gray,Rgb,Bgr}` + `GlyphKey` 加模式维度；几何桶常量改名 `BUCKETS_PER_PX` 避开同名
3. height-anim（main.rs）：`Anim{from,to,start}` + ease-out cubic；目标高度变化时从当前显示高度重启动画；`tick()` 每 16 ms 推进
4. keywords（items.rs + desktop.rs）：`Item.extra`（仅匹配、不显示）+ `filter` 大小写不敏感关键词检查；排序仍看可见文本

风险与取舍：

- 单遍绘制实测只拿到 20%（4.84→3.88），低于预期 2×：消除了一整遍全缓冲写，但逐行分段写不如整块 fill 高效
- 子像素 AA 实测 +35%（scale1）/ +55%（scale2）→ 定策略为仅 scale 1 启用
- “属性/注释紧跟函数”又踩两次（allow 被新常量挤开；删 fill 连带删了 rect 的 allow）——已修

验证检查点：70 passed；clippy 0；配对基准（gray vs rgb）记录在 SPEC。
# Plan: prompt-badge-r2

确认范围：不做「固定槽位/坐标一致」（否决）；只做三件：

1. 颜色 0x004560 → 0x2e4a5c（灰蓝，与灰栏调和、不撞选中蓝）
2. 胶囊两侧半圆收口（`cap_inset` 双侧复用，左缘同样内收）
3. 栏身灰同步不变（0x2e，已达成）

测试：左缘顶行内收断言新增；caret 差值断言不受影响（文字起点/胶囊右缘未变）。
验证检查点：全量 `cargo test` 55 passed；clippy lint 类型与父提交一致；ASCII 拓扑确认胶囊对称。
# Plan: prompt-outline (E) + 保留 F

确认：形状菜单（B/C/D/E/F/G）→ 选 E 描边胶囊；F 左侧强调条完整存档不实现。

1. `BG_LABEL`/字段 → `LABEL_ACCENT`/`label_accent`（0x5c8494 亮灰蓝描边色，纯内部重命名）
2. draw()：填充循环 → 轮廓循环（顶/底整行描边 + 中间行左右边列，内部留 `bg_prompt`）
3. 测试改写（描边/内部/帽缘断言）

风险：

- 描边颜色必须与 0x2e 栏身有对比——选亮灰蓝而非原 0x2e4a5c（同色相太暗）
- 顶帽 inset=7 为确定性值，但扫描断言（而非精确坐标）更抗字体度量变化

验证检查点：全量 `cargo test` 55 passed；clippy lint 类型一致；ASCII 确认 1px 轮廓。
# Plan: prompt-rail (F，取代 E)

用户对 E 不满意 → 改 F 左侧强调条（SPEC-prompt-outline.md 的保留档直接启用）。

1. `LABEL_ACCENT` 回 0x2e4a5c（深灰蓝，用户早前确认的强调色）；`PILL_HPAD` 删 → `LABEL_RAIL_W=4`
2. draw()：描边循环 → `rect` 4px 竖条（高 20 居中）+ 标签文字自 16+8=24 起 + 文字后 8px 接输入
3. 清理：删 `cap_inset`（无引用）；`measure` 转 `#[cfg(test)]`（draw 不再用，防 dead_code）
4. 测试改写：rail 竖条/栏身统一断言；caret 差值基数 = 4+16=20

风险：

- 竖条色若用 E 的亮 0x5c8494 填充会显脏——回深 0x2e4a5c
- 文字起点改为硬编码推导，测试与实现必须同源

验证检查点：全量 `cargo test` 55 passed；clippy 类型一致；ASCII 确认竖条/间距。
