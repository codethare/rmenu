//! rmenu — a wmenu/dmenu-style menu for Wayland (wlr-layer-shell).
//!
//! Plain text or `--run` launcher mode. `--run` scans .desktop files;
//! otherwise lines are read from stdin.

mod desktop;
mod font;
mod items;
mod render;

use std::io::{self, BufRead};
use std::process::exit;
use std::time::Duration;

use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;

use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState, FrameCallbackData},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
    shm::{slot::SlotPool, Shm, ShmHandler},
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_seat, wl_shm, wl_surface},
    Connection, QueueHandle,
};

const DEFAULT_WIDTH: u32 = 640;
const FONT_SIZE: f32 = 16.0;
/// Panel content inset: must be >= corner radius so text never grazes the curve.
const PAD: u32 = 12;
/// Away-from-edge float for the upper-center panel (bottom mode keeps 8px).
const TOP_MARGIN: i32 = 32;
/// ponytail: stdin can be huge; cap drawn rows and scroll instead of mapping a giant buffer.
const MAX_VISIBLE: usize = 64;

struct Opts {
    prompt: String,
    width: u32,
    lines: usize,
    ci: bool,
    font: Option<String>,
    run: bool,
    bottom: bool,
    password: bool,
    colors: render::Colors,
}

fn usage() -> ! {
    eprintln!(
        "usage: rmenu [-biPv] [-f font.ttf|FAMILY [style] [pt|Npx]] [-l lines] [-W width] [-p prompt] [--run]\n\
               [-N color] [-n color] [-M color] [-m color] [-S color] [-s color]\n\
         \n\
         Reads lines from stdin and prints the selected line to stdout.\n\
         `--run` ignores stdin, lists .desktop applications, and launches the selection.\n\
         `-b` shows the menu at the bottom of the screen; `-P` masks typed input as asterisks;\n\
         `-i` matches case-insensitively.\n\
         Colors are wmenu-style `RRGGBB[AA]`: `-N`/`-n` normal bg/fg, `-M`/`-m` prompt bg/fg,\n\
         `-S`/`-s` selection bg/fg."
    );
    exit(1);
}

fn parse_opts_from(args: impl Iterator<Item = String>) -> Opts {
    let mut o = Opts {
        prompt: String::new(),
        width: DEFAULT_WIDTH,
        lines: 0,
        ci: false,
        font: None,
        run: false,
        bottom: false,
        password: false,
        colors: render::Colors::default(),
    };
    let mut args = args;
    while let Some(a) = args.next() {
        let color = |s: String| render::parse_color(&s).unwrap_or_else(|| usage());
        match a.as_str() {
            "-i" => o.ci = true,
            "-b" => o.bottom = true,
            "-P" => o.password = true,
            "-p" => o.prompt = args.next().unwrap_or_else(|| usage()),
            "-l" => o.lines = args.next().unwrap_or_else(|| usage()).parse().unwrap_or_else(|_| usage()),
            "-W" => o.width = args.next().unwrap_or_else(|| usage()).parse().unwrap_or_else(|_| usage()),
            "-f" => o.font = Some(args.next().unwrap_or_else(|| usage())),
            "-N" => o.colors.bg_normal = color(args.next().unwrap_or_else(|| usage())),
            "-n" => o.colors.fg_normal = color(args.next().unwrap_or_else(|| usage())),
            "-M" => o.colors.bg_prompt = color(args.next().unwrap_or_else(|| usage())),
            "-m" => o.colors.fg_prompt = color(args.next().unwrap_or_else(|| usage())),
            "-S" => o.colors.bg_sel = color(args.next().unwrap_or_else(|| usage())),
            "-s" => o.colors.fg_sel = color(args.next().unwrap_or_else(|| usage())),
            "--run" => o.run = true,
            "-h" | "--help" => usage(),
            _ => {
                eprintln!("rmenu: unknown option {a}");
                usage();
            }
        }
    }
    o
}

fn parse_opts() -> Opts {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-v" || a == "--version") {
        println!("rmenu {}", env!("CARGO_PKG_VERSION"));
        exit(0);
    }
    parse_opts_from(args.into_iter())
}

fn main() {
    let opts = parse_opts();

    let items: Vec<items::Item> = if opts.run {
        desktop::merged(desktop::load_apps(), desktop::path_commands())
    } else {
        let stdin = io::stdin();
        stdin.lock().lines().filter_map(|l| l.ok()).map(|l| items::parse(&l)).collect()
    };
    if items.is_empty() {
        eprintln!("rmenu: no items");
        exit(1);
    }

    let font = font::MenuFont::load(opts.font.as_deref(), FONT_SIZE)
        .unwrap_or_else(|e| { eprintln!("rmenu: {e}"); exit(1) });

    let conn = Connection::connect_to_env().unwrap_or_else(|e| {
        eprintln!("rmenu: cannot connect to Wayland: {e}");
        exit(1);
    });
    let (globals, event_queue) = registry_queue_init(&conn).unwrap_or_else(|e| {
        eprintln!("rmenu: cannot init globals: {e}");
        exit(1);
    });
    let qh = event_queue.handle();
    // calloop drives both the Wayland socket and sctk's key-repeat timer, so
    // holding a key (C-p/C-n, arrows) auto-repeats at the compositor's rate.
    let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap_or_else(|e| {
        eprintln!("rmenu: {e}");
        exit(1);
    });
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue).insert(loop_handle.clone()).unwrap_or_else(|e| {
        eprintln!("rmenu: {e}");
        exit(1);
    });
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor missing");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell unsupported");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm missing");

    let (_visible, height) = visible_rows(&items, opts.lines, &font);
    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("rmenu"), None);
    layer.set_anchor(if opts.bottom { Anchor::BOTTOM } else { Anchor::TOP });
    layer.set_margin(if opts.bottom { 8 } else { TOP_MARGIN }, 0, 0, 0);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    // Start collapsed to the single input bar (Spotlight); the list appears
    // once the query has content. The pool keeps the max height for rows.
    layer.set_size(opts.width, font.row_h);
    layer.commit();

    let pool = SlotPool::new((opts.width * height * 4) as usize, &shm)
        .expect("failed to allocate shm pool");

    let item_count = items.len();
    let mut app = App {
        registry_state: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        shm,
        compositor,
        layer_shell,
        layer: Some(layer),
        pool,
        qh: qh.clone(),
        keyboard: None,
        items,
        menu: MenuState::new(item_count),
        top: 0,
        font,
        prompt: opts.prompt.clone(),
        width: opts.width,
        lines: opts.lines,
        ci: opts.ci,
        colors: opts.colors,
        password: opts.password,
        bottom: opts.bottom,
        output_w: None,
        dirty: false,
        frame_pending: false,
        first_configure: true,
        run: opts.run,
        mods: Modifiers::default(),
        loop_handle,
    };

    loop {
        if event_loop.dispatch(Duration::from_millis(16), &mut app).is_err() {
            break;
        }
        if app.menu.done.is_some() {
            break;
        }
    }
    app.finish();
}

/// Number of item rows to draw and the resulting surface height (incl. prompt row).
fn visible_rows(items: &[items::Item], lines: usize, font: &font::MenuFont) -> (usize, u32) {
    let visible = if lines > 0 { lines.min(items.len()) } else { items.len().min(MAX_VISIBLE) };
    (visible, font.row_h * (visible as u32 + 1))
}

/// Menu outcome: `None` while running, `Some` once the user (or an error) closed it.
#[derive(Debug, PartialEq)]
enum Done {
    Cancel,
    Select(String),
}

/// Keyboard/filter state, Wayland-free so it is unit-testable.
struct MenuState {
    query: String,
    matches: Vec<usize>,
    sel: usize,
    done: Option<Done>,
    /// Values picked by Ctrl-Return while the menu keeps running (multi-select).
    picked: Vec<String>,
}

impl MenuState {
    fn new(count: usize) -> Self {
        Self {
            query: String::new(),
            matches: (0..count).collect(),
            sel: 0,
            done: None,
            picked: Vec::new(),
        }
    }

    fn refilter(&mut self, items: &[items::Item], ci: bool) {
        self.matches = items::filter(items, &self.query, ci);
        self.sel = 0;
    }

    /// The value Return picks: the highlighted item, or (dmenu/wmenu contract)
    /// the typed line when there is no match — that's how `echo "" | rmenu -p …`
    /// prompt-style scripts work.
    fn pick_value(&self, items: &[items::Item]) -> String {
        match self.matches.get(self.sel) {
            Some(&i) => items[i].value.clone(),
            None => self.query.clone(),
        }
    }

    /// Plain Return: select the highlighted item, or cancel on an empty query
    /// (with the list hidden there is nothing visible to pick, so Enter
    /// cancels instead of selecting blind).
    fn enter(&mut self, items: &[items::Item]) {
        self.done = if self.query.is_empty() {
            Some(Done::Cancel)
        } else {
            Some(Done::Select(self.pick_value(items)))
        };
    }

    fn on_key(
        &mut self,
        keysym: Keysym,
        utf8: Option<String>,
        mods: Modifiers,
        items: &[items::Item],
        ci: bool,
        visible: usize,
    ) {
        match keysym {
            Keysym::Escape => self.done = Some(Done::Cancel),
            // Ctrl-Return: pick the highlighted entry, print it, and keep the
            // menu open so scripts can multi-select (App emits `picked` after
            // this call); advance so repeated presses walk the list.
            Keysym::Return | Keysym::KP_Enter if mods.ctrl => {
                self.picked.push(self.pick_value(items));
                if self.sel + 1 < self.matches.len() {
                    self.sel += 1;
                }
            }
            // Shift-Return: submit exactly what's typed, matches or not.
            Keysym::Return | Keysym::KP_Enter if mods.shift => {
                self.done = Some(Done::Select(self.query.clone()));
            }
            Keysym::Return | Keysym::KP_Enter => self.enter(items),
            Keysym::BackSpace => {
                if self.query.pop().is_some() {
                    self.refilter(items, ci);
                }
            }
            Keysym::Up => {
                if self.sel > 0 {
                    self.sel -= 1;
                }
            }
            Keysym::Down => {
                if self.sel + 1 < self.matches.len() {
                    self.sel += 1;
                }
            }
            Keysym::Page_Up => {
                self.sel = self.sel.saturating_sub(visible.max(1));
            }
            Keysym::Page_Down => {
                self.sel = (self.sel + visible.max(1)).min(self.matches.len().saturating_sub(1));
            }
            Keysym::Home => self.sel = 0,
            Keysym::End => self.sel = self.matches.len().saturating_sub(1),
            Keysym::Tab => {
                // dmenu habit: complete the highlighted item into the query.
                if let Some(&i) = self.matches.get(self.sel) {
                    let text = items[i].text.clone();
                    if text != self.query {
                        self.query = text;
                        self.refilter(items, ci);
                        // refilter resets sel to 0; anchor back on the completed item
                        // (prefix ties like "libreoffice" vs "libreoffice-stable").
                        if let Some(p) = self.matches.iter().position(|&m| m == i) {
                            self.sel = p;
                        }
                    }
                }
            }
            _ => {
                // xkb encodes ctrl+key as a single control char (ctrl+a -> U+0001).
                // Bind the chords we support; everything else must not become
                // query text (the old guard dropped them silently).
                if let Some(t) = utf8 {
                    match t.chars().next() {
                        Some('\u{3}') | Some('\u{7}') => self.done = Some(Done::Cancel), // C-c / C-g
                        Some('\u{8}') => {
                            // C-h = backspace
                            if self.query.pop().is_some() {
                                self.refilter(items, ci);
                            }
                        }
                        Some('\u{a}') | Some('\u{d}') => self.enter(items), // C-j / C-m = Return
                        Some('\u{15}') => {
                            // C-u: delete everything left of the caret (end of line)
                            if !self.query.is_empty() {
                                self.query.clear();
                                self.refilter(items, ci);
                            }
                        }
                        Some('\u{e}') => {
                            // C-n: next row
                            if self.sel + 1 < self.matches.len() {
                                self.sel += 1;
                            }
                        }
                        Some('\u{10}') => {
                            // C-p: previous row
                            if self.sel > 0 {
                                self.sel -= 1;
                            }
                        }
                        Some('\u{17}') => {
                            // C-w: kill the last word and its separator (readline
                            // style). Caret is at the end — no cursor support yet.
                            let end = self.query.trim_end_matches(' ').len();
                            let start = self.query[..end].rfind(' ').unwrap_or(0);
                            self.query.truncate(start);
                            self.refilter(items, ci);
                        }
                        _ => {
                            if !t.is_empty() && !t.chars().any(char::is_control) {
                                self.query.push_str(&t);
                                self.refilter(items, ci);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[allow(dead_code)] // state objects kept alive for their proxy bindings
struct App {
    registry_state: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    shm: Shm,
    compositor: CompositorState,
    layer_shell: LayerShell,
    layer: Option<LayerSurface>,
    pool: SlotPool,
    qh: QueueHandle<App>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    /// Latest modifier state; sctk drops modifiers from KeyEvent, so
    /// `update_modifiers` is the only place they arrive.
    mods: Modifiers,
    items: Vec<items::Item>,
    menu: MenuState,
    top: usize,
    font: font::MenuFont,
    prompt: String,
    width: u32,
    lines: usize,
    ci: bool,
    colors: render::Colors,
    password: bool,
    bottom: bool,
    /// Logical width of the output the panel is mapped on; `None` until
    /// surface_enter, used to center the panel horizontally (Spotlight).
    output_w: Option<u32>,
    dirty: bool,
    frame_pending: bool,
    first_configure: bool,
    run: bool,
    loop_handle: smithay_client_toolkit::reexports::calloop::LoopHandle<'static, App>,
}

impl App {
    fn visible(&self) -> usize {
        if self.lines > 0 {
            self.lines.min(self.menu.matches.len())
        } else {
            self.menu.matches.len().min(MAX_VISIBLE)
        }
    }

    fn on_key(&mut self, keysym: Keysym, utf8: Option<String>) {
        let visible = self.visible();
        self.menu.on_key(keysym, utf8, self.mods, &self.items, self.ci, visible);
        // Ctrl-Return multi-select: emit each pick while the menu keeps running.
        let picked = std::mem::take(&mut self.menu.picked);
        for v in picked {
            if self.run {
                self.launch(&v);
            } else {
                println!("{v}");
            }
        }
        self.request_draw();
    }

    /// Spawn a `--run` selection via sh (selections are shell command lines).
    fn launch(&self, cmd: &str) {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdin(std::process::Stdio::null())
            .spawn();
    }

    fn request_draw(&mut self) {
        if self.frame_pending {
            self.dirty = true;
        } else {
            self.draw();
        }
    }

    fn draw(&mut self) {
        if self.frame_pending {
            return;
        }
        let Some(layer) = self.layer.clone() else { return };

        // Spotlight: no list until the query has content; collapse back to the
        // single input bar when the query is cleared.
        let visible = if self.menu.query.is_empty() { 0 } else { self.visible() };
        let total = self.menu.matches.len();

        // Keep the selection in view (only meaningful while the list is shown).
        let mut rows: Vec<render::Row> = Vec::with_capacity(visible);
        if visible > 0 {
            if self.menu.sel >= self.top + visible && visible > 0 {
                self.top = self.menu.sel + 1 - visible;
            } else if self.menu.sel < self.top {
                self.top = self.menu.sel;
            }
            let top = self.top.min(total.saturating_sub(1));
            for (i, &mi) in self.menu.matches[top..(top + visible).min(total)].iter().enumerate() {
                let it = &self.items[mi];
                rows.push(render::Row { text: &it.text, selected: top + i == self.menu.sel });
            }
        }

        let w = self.width;
        let h = self.font.row_h * (visible as u32 + 1);
        if self.bottom {
            layer.set_margin(8, 0, 0, 0);
        } else {
            // Upper-center: 24px top breathing room, horizontally centered.
            let left = self.output_w.map(|ow| ((ow as i64 - w as i64) / 2).max(0) as i32).unwrap_or(0);
            layer.set_margin(TOP_MARGIN, 0, 0, left);
        }
        layer.set_size(w, h);

        let (buffer, canvas) = match self
            .pool
            .create_buffer(w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Argb8888)
        {
            Ok(b) => b,
            Err(e) => {
                eprintln!("rmenu: buffer error: {e}");
                self.menu.done = Some(Done::Cancel);
                return;
            }
        };
        render::draw(canvas, w, h, &self.font, &self.prompt, &self.menu.query, self.password, &rows, PAD, &self.colors);

        layer.wl_surface().damage_buffer(0, 0, w as i32, h as i32);
        let surface = layer.wl_surface().clone();
        surface.frame(&self.qh, FrameCallbackData(surface.clone()));
        if buffer.attach_to(layer.wl_surface()).is_err() {
            self.menu.done = Some(Done::Cancel);
            return;
        }
        layer.commit();
        self.frame_pending = true;
        self.dirty = false;
    }

    fn finish(&mut self) {
        match self.menu.done.take() {
            Some(Done::Select(value)) => {
                if self.run {
                    self.launch(&value);
                    exit(0);
                }
                println!("{value}");
                exit(0);
            }
            _ => exit(1),
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.frame_pending = false;
        if self.dirty {
            self.draw();
        }
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if let Some(w) = self
            .output_state
            .info(output)
            .and_then(|i| i.logical_size)
            .map(|(w, _)| w.max(0) as u32)
        {
            if self.output_w != Some(w) {
                self.output_w = Some(w); // center the panel on this output
                self.request_draw();
            }
        }
    }
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.menu.done = Some(Done::Cancel);
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &LayerSurface,
        _: LayerSurfaceConfigure,
        _: u32,
    ) {
        if self.first_configure {
            self.first_configure = false;
            self.draw();
        } else {
            self.request_draw();
        }
    }
}

impl SeatHandler for App {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            // Route repeats through the calloop loop (the sctk repeat timer);
            // with plain get_keyboard the repeat callback is never armed.
            self.keyboard = self
                .seat_state
                .get_keyboard_with_repeat(
                    qh,
                    &seat,
                    None,
                    self.loop_handle.clone(),
                    Box::new(|app, _kbd, event| app.on_key(event.keysym, event.utf8)),
                )
                .ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(kb) = self.keyboard.take() {
                kb.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for App {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.on_key(event.keysym, event.utf8);
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
        // Repeats are delivered via the get_keyboard_with_repeat callback, not here.
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
        self.mods = modifiers;
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for App {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_registry!(App);

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(App);

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_items() -> Vec<items::Item> {
        ["firefox", "alacritty", "libreoffice"]
            .iter()
            .map(|s| items::Item { lc: s.to_lowercase(), text: s.to_string(), value: s.to_string() })
            .collect()
    }

    fn opts(args: &[&str]) -> Opts {
        parse_opts_from(args.iter().map(|s| s.to_string()))
    }

    fn mods(ctrl: bool, shift: bool) -> Modifiers {
        Modifiers { ctrl, shift, ..Default::default() }
    }

    /// Key press with default modifiers; `visible=3` mirrors the 3-item sample list.
    fn key(m: &mut MenuState, items: &[items::Item], ks: Keysym, u: Option<&str>) {
        m.on_key(ks, u.map(String::from), Modifiers::default(), items, false, 3);
    }

    /// xkeysym for any printable 'F' key; the `_` arm only uses the utf8 text.
    fn type_key(m: &mut MenuState, items: &[items::Item], c: char) {
        key(m, items, Keysym::from(0x66u32), Some(&c.to_string()));
    }

    #[test]
    fn color_flags_match_wmenu_fields() {
        // `-M`/`-m` are the prompt rows, `-S`/`-s` the selection (wmenu semantics).
        let o = opts(&["-N", "112233", "-n", "445566", "-M", "778899", "-m", "aabbcc", "-S", "ddeeff", "-s", "010203"]);
        assert_eq!(o.colors.bg_normal, [0x33, 0x22, 0x11, 0xff]);
        assert_eq!(o.colors.fg_normal, [0x66, 0x55, 0x44, 0xff]);
        assert_eq!(o.colors.bg_prompt, [0x99, 0x88, 0x77, 0xff]);
        assert_eq!(o.colors.fg_prompt, [0xcc, 0xbb, 0xaa, 0xff]);
        assert_eq!(o.colors.bg_sel, [0xff, 0xee, 0xdd, 0xff]);
        assert_eq!(o.colors.fg_sel, [0x03, 0x02, 0x01, 0xff]);
    }

    #[test]
    fn bottom_and_password_flags() {
        let o = opts(&["-b", "-P"]);
        assert!(o.bottom && o.password);
        let d = opts(&[]);
        assert!(!d.bottom && !d.password);
    }

    #[test]
    fn escape_cancels() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        assert!(m.done.is_none());
        key(&mut m, &items, Keysym::Escape, None);
        assert_eq!(m.done, Some(Done::Cancel));
    }

    #[test]
    fn enter_selects_highlighted_value() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'f'); // narrows to firefox + libreoffice, firefox first
        key(&mut m, &items, Keysym::Return, None);
        assert_eq!(m.done, Some(Done::Select("firefox".into())));
    }

    #[test]
    fn enter_with_empty_query_cancels() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        key(&mut m, &items, Keysym::Return, None);
        assert_eq!(m.done, Some(Done::Cancel));
    }

    #[test]
    fn typing_filters_and_enter_picks_only_match() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'l');
        type_key(&mut m, &items, 'i');
        assert_eq!(m.matches.len(), 1);
        key(&mut m, &items, Keysym::Return, None);
        assert_eq!(m.done, Some(Done::Select("libreoffice".into())));
    }

    #[test]
    fn enter_with_no_match_echoes_query() {
        // dmenu/wmenu contract: with nothing to pick, Enter prints the typed
        // line — that's how `echo "" | rmenu -p …` prompt scripts work.
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'z');
        assert!(m.matches.is_empty());
        key(&mut m, &items, Keysym::Return, None);
        assert_eq!(m.done, Some(Done::Select("z".into())));
    }

    #[test]
    fn unbound_ctrl_chords_do_not_garbble_query() {
        // Wayland hands ctrl+a .. ctrl+z to us as U+0001..U+001A control chars;
        // unbound chords (C-a, C-v) must still never become query text.
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        m.on_key(Keysym::from(0x61u32), Some("\u{1}".into()), mods(true, false), &items, false, 3);
        m.on_key(Keysym::from(0x76u32), Some("\u{16}".into()), mods(true, false), &items, false, 3);
        assert_eq!(m.query, "");
        assert_eq!(m.matches.len(), 3);
    }

    #[test]
    fn backspace_restores_matches() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'a'); // only "alacritty"
        assert_eq!(m.matches.len(), 1);
        key(&mut m, &items, Keysym::BackSpace, None);
        assert_eq!(m.matches.len(), 3);
    }

    #[test]
    fn arrows_move_selection_within_bounds() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'i'); // matches all three, original order (no prefix hit)
        key(&mut m, &items, Keysym::Up, None); // clamp at top
        assert_eq!(m.sel, 0);
        key(&mut m, &items, Keysym::Down, None);
        assert_eq!(m.sel, 1);
        key(&mut m, &items, Keysym::Down, None);
        key(&mut m, &items, Keysym::Down, None); // clamp at bottom
        assert_eq!(m.sel, 2);
        key(&mut m, &items, Keysym::Return, None);
        assert_eq!(m.done, Some(Done::Select("libreoffice".into())));
    }

    #[test]
    fn ctrl_n_p_move_selection() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'i'); // all three matches
        m.on_key(Keysym::from(0x70u32), Some("\u{10}".into()), mods(true, false), &items, false, 3); // C-p clamp at top
        assert_eq!(m.sel, 0);
        m.on_key(Keysym::from(0x6eu32), Some("\u{e}".into()), mods(true, false), &items, false, 3); // C-n
        assert_eq!(m.sel, 1);
        m.on_key(Keysym::from(0x70u32), Some("\u{10}".into()), mods(true, false), &items, false, 3); // C-p
        assert_eq!(m.sel, 0);
        m.on_key(Keysym::from(0x6eu32), Some("\u{e}".into()), mods(true, false), &items, false, 3);
        m.on_key(Keysym::from(0x6eu32), Some("\u{e}".into()), mods(true, false), &items, false, 3); // C-n clamp at bottom
        assert_eq!(m.sel, 2);
    }

    #[test]
    fn page_keys_jump_by_visible_rows() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        m.on_key(Keysym::Page_Down, None, mods(false, false), &items, false, 2);
        assert_eq!(m.sel, 2); // clamped to last (3 items, page 2)
        m.on_key(Keysym::Page_Up, None, mods(false, false), &items, false, 2);
        assert_eq!(m.sel, 0);
        m.on_key(Keysym::Page_Up, None, mods(false, false), &items, false, 2); // clamp at top
        assert_eq!(m.sel, 0);
    }

    #[test]
    fn home_end_jump_selection() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        key(&mut m, &items, Keysym::Down, None);
        key(&mut m, &items, Keysym::Home, None);
        assert_eq!(m.sel, 0);
        key(&mut m, &items, Keysym::End, None);
        assert_eq!(m.sel, 2);
        key(&mut m, &items, Keysym::End, None); // clamp
        assert_eq!(m.sel, 2);
    }

    #[test]
    fn ctrl_c_cancels() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'a');
        m.on_key(Keysym::from(0x63u32), Some("\u{3}".into()), mods(true, false), &items, false, 3);
        assert_eq!(m.done, Some(Done::Cancel));
    }

    #[test]
    fn ctrl_u_clears_query() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'f');
        m.on_key(Keysym::from(0x75u32), Some("\u{15}".into()), mods(true, false), &items, false, 3);
        assert_eq!(m.query, "");
        assert_eq!(m.matches.len(), 3);
    }

    #[test]
    fn ctrl_w_kills_last_word_readline_style() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        for c in ['a', 'b', ' ', 'c', 'd'] {
            type_key(&mut m, &items, c); // "ab cd"
        }
        m.on_key(Keysym::from(0x77u32), Some("\u{17}".into()), mods(true, false), &items, false, 3);
        assert_eq!(m.query, "ab");
    }

    #[test]
    fn ctrl_w_on_trailing_space_kills_word_and_separator() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        for c in ['a', 'b', ' ', 'c', 'd', ' '] {
            type_key(&mut m, &items, c); // "ab cd "
        }
        m.on_key(Keysym::from(0x77u32), Some("\u{17}".into()), mods(true, false), &items, false, 3);
        assert_eq!(m.query, "ab");
    }

    #[test]
    fn ctrl_h_backspaces() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'l');
        m.on_key(Keysym::from(0x68u32), Some("\u{8}".into()), mods(true, false), &items, false, 3);
        assert_eq!(m.query, "");
        assert_eq!(m.matches.len(), 3);
    }

    #[test]
    fn ctrl_j_enters_like_return() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'l'); // libreoffice
        m.on_key(Keysym::from(0x6au32), Some("\u{a}".into()), mods(true, false), &items, false, 3);
        assert_eq!(m.done, Some(Done::Select("libreoffice".into())));
    }

    #[test]
    fn shift_return_submits_typed_text() {
        // Even with matches available, Shift-Return submits exactly the typed line.
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'i'); // matches all three, sel=0 (firefox)
        m.on_key(Keysym::Return, None, mods(false, true), &items, false, 3);
        assert_eq!(m.done, Some(Done::Select("i".into())));
    }

    #[test]
    fn ctrl_return_picks_and_continues() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'i'); // all three match, sel=0 -> firefox
        let ctrl = mods(true, false);
        m.on_key(Keysym::Return, None, ctrl, &items, false, 3);
        assert_eq!(m.picked, vec!["firefox".to_string()]);
        assert!(m.done.is_none());
        assert_eq!(m.sel, 1); // advanced so repeated Ctrl-Return walks the list
        m.on_key(Keysym::Return, None, ctrl, &items, false, 3);
        assert_eq!(m.sel, 2);
        m.on_key(Keysym::Return, None, ctrl, &items, false, 3);
        assert_eq!(m.sel, 2); // clamps at last
        assert_eq!(m.picked, vec!["firefox".to_string(), "alacritty".to_string(), "libreoffice".to_string()]);
    }

    #[test]
    fn tab_completes_highlighted_item_into_query() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'o'); // matches: firefox, libreoffice
        key(&mut m, &items, Keysym::Down, None); // highlight libreoffice (2nd match)
        key(&mut m, &items, Keysym::Tab, None);
        assert_eq!(m.query, "libreoffice");
        assert_eq!(m.matches.len(), 1); // refiltered to the completed item
        assert_eq!(m.sel, 0);
        key(&mut m, &items, Keysym::Return, None);
        assert_eq!(m.done, Some(Done::Select("libreoffice".into())));
    }

    #[test]
    fn tab_with_hidden_list_fills_first_item() {
        // Spotlight collapses the list on empty query, but the internal
        // selection still exists — Tab fills it like any other state.
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        key(&mut m, &items, Keysym::Tab, None);
        assert_eq!(m.query, "firefox");
        key(&mut m, &items, Keysym::Return, None);
        assert_eq!(m.done, Some(Done::Select("firefox".into())));
    }

    #[test]
    fn tab_with_no_match_is_noop() {
        let items = sample_items();
        let mut m = MenuState::new(items.len());
        type_key(&mut m, &items, 'z'); // no matches
        key(&mut m, &items, Keysym::Tab, None);
        assert_eq!(m.query, "z");
        assert!(m.matches.is_empty());
    }
}