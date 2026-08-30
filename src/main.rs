//! rmenu — a wmenu/dmenu-style menu for Wayland (wlr-layer-shell) with program icons.
//!
//! Plain text or `--run` launcher mode. `--run` scans .desktop files and shows their
//! icons; otherwise lines are read from stdin, optionally as `<icon>\t<label>`.

mod desktop;
mod font;
mod icon;
mod items;
mod render;

use std::collections::HashMap;
use std::io::{self, BufRead};
use std::process::exit;

use image::RgbaImage;
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
const PAD: u32 = 4;
/// ponytail: stdin can be huge; cap drawn rows and scroll instead of mapping a giant buffer.
const MAX_VISIBLE: usize = 64;

struct Opts {
    prompt: String,
    width: u32,
    lines: usize,
    ci: bool,
    font: Option<String>,
    run: bool,
}

fn usage() -> ! {
    eprintln!(
        "usage: rmenu [-i] [-p prompt] [-l lines] [-W width] [-f font.ttf] [--run]\n\
         \n\
         Reads lines from stdin; each line may be `icon<TAB>label` where icon is a\n\
         path or a freedesktop icon theme name. Prints the selected label to stdout.\n\
         `--run` ignores stdin, lists .desktop applications with their icons, and\n\
         launches the selection. `-i` matches case-insensitively."
    );
    exit(1);
}

fn parse_opts() -> Opts {
    let mut o = Opts { prompt: String::new(), width: DEFAULT_WIDTH, lines: 0, ci: false, font: None, run: false };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-i" => o.ci = true,
            "-p" => o.prompt = args.next().unwrap_or_else(|| usage()),
            "-l" => o.lines = args.next().unwrap_or_else(|| usage()).parse().unwrap_or_else(|_| usage()),
            "-W" => o.width = args.next().unwrap_or_else(|| usage()).parse().unwrap_or_else(|_| usage()),
            "-f" => o.font = Some(args.next().unwrap_or_else(|| usage())),
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

fn main() {
    let opts = parse_opts();

    let items: Vec<items::Item> = if opts.run {
        desktop::load_apps()
            .into_iter()
            .map(|a| items::Item { icon: a.icon, text: a.name, value: a.exec })
            .collect()
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
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap_or_else(|e| {
        eprintln!("rmenu: cannot init globals: {e}");
        exit(1);
    });
    let qh = event_queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor missing");
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr-layer-shell unsupported");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm missing");

    let (_visible, height) = visible_rows(&items, opts.lines, &font);
    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("rmenu"), None);
    layer.set_anchor(Anchor::TOP);
    layer.set_margin(0, 0, 8, 0);
    layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
    layer.set_size(opts.width, height);
    layer.commit();

    let pool = SlotPool::new((opts.width * height * 4) as usize, &shm)
        .expect("failed to allocate shm pool");

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
        matches: Vec::new(),
        sel: 0,
        top: 0,
        query: String::new(),
        icons: HashMap::new(),
        font,
        prompt: opts.prompt.clone(),
        width: opts.width,
        lines: opts.lines,
        ci: opts.ci,
        exit: None,
        dirty: false,
        frame_pending: false,
        first_configure: true,
        run: opts.run,
    };
    app.matches = (0..app.items.len()).collect();

    while app.exit.is_none() && event_queue.blocking_dispatch(&mut app).is_ok() {}
    app.finish();
}

/// Number of item rows to draw and the resulting surface height (incl. prompt row).
fn visible_rows(items: &[items::Item], lines: usize, font: &font::MenuFont) -> (usize, u32) {
    let visible = if lines > 0 { lines.min(items.len()) } else { items.len().min(MAX_VISIBLE) };
    (visible, font.row_h * (visible as u32 + 1))
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
    items: Vec<items::Item>,
    matches: Vec<usize>,
    sel: usize,
    top: usize,
    query: String,
    icons: HashMap<String, RgbaImage>,
    font: font::MenuFont,
    prompt: String,
    width: u32,
    lines: usize,
    ci: bool,
    /// `None` = canceled (Escape), `Some(value)` = selected.
    exit: Option<String>,
    dirty: bool,
    frame_pending: bool,
    first_configure: bool,
    run: bool,
}

impl App {
    fn visible(&self) -> usize {
        if self.lines > 0 {
            self.lines.min(self.matches.len())
        } else {
            self.matches.len().min(MAX_VISIBLE)
        }
    }

    fn select(&mut self) {
        self.exit = self.matches.get(self.sel).map(|&i| self.items[i].value.clone());
    }

    fn refilter(&mut self) {
        self.matches = items::filter(&self.items, &self.query, self.ci());
        self.sel = 0;
        self.top = 0;
        self.request_draw();
    }

    fn ci(&self) -> bool {
        self.ci
    }

    fn on_key(&mut self, keysym: Keysym, utf8: Option<String>) {
        match keysym {
            Keysym::Escape => self.exit = None,
            Keysym::Return | Keysym::KP_Enter => self.select(),
            Keysym::BackSpace => {
                self.query.pop();
                self.refilter();
            }
            Keysym::Up => {
                if self.sel > 0 {
                    self.sel -= 1;
                    self.request_draw();
                }
            }
            Keysym::Down => {
                if self.sel + 1 < self.matches.len() {
                    self.sel += 1;
                    self.request_draw();
                }
            }
            _ => {
                if let Some(t) = utf8 {
                    if !t.is_empty() {
                        self.query.push_str(&t);
                        self.refilter();
                    }
                }
            }
        }
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

        // Keep the selection in view.
        let visible = self.visible();
        let total = self.matches.len();
        if self.sel >= self.top + visible && visible > 0 {
            self.top = self.sel + 1 - visible;
        } else if self.sel < self.top {
            self.top = self.sel;
        }
        let top = self.top.min(total.saturating_sub(1));

        // Resolve icons for the visible rows.
        let icon_sz = self.font.row_h - 2 * PAD;
        for &mi in &self.matches[top..(top + visible).min(total)] {
            if let Some(spec) = &self.items[mi].icon {
                if !self.icons.contains_key(spec) {
                    if let Some(img) = icon::load(spec, icon_sz) {
                        self.icons.insert(spec.clone(), img);
                    }
                }
            }
        }

        let mut rows: Vec<render::Row> = Vec::with_capacity(visible);
        for (i, &mi) in self.matches[top..(top + visible).min(total)].iter().enumerate() {
            let it = &self.items[mi];
            rows.push(render::Row {
                icon: it.icon.as_ref().and_then(|s| self.icons.get(s)),
                text: &it.text,
                selected: top + i == self.sel,
            });
        }

        let w = self.width;
        let h = self.font.row_h * (visible as u32 + 1);
        layer.set_size(w, h);

        let (buffer, canvas) = match self
            .pool
            .create_buffer(w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Argb8888)
        {
            Ok(b) => b,
            Err(e) => {
                eprintln!("rmenu: buffer error: {e}");
                self.exit = None;
                return;
            }
        };
        render::draw(canvas, w, h, &self.font, &self.prompt, &self.query, &rows, PAD);

        layer.wl_surface().damage_buffer(0, 0, w as i32, h as i32);
        let surface = layer.wl_surface().clone();
        surface.frame(&self.qh, FrameCallbackData(surface.clone()));
        if buffer.attach_to(layer.wl_surface()).is_err() {
            self.exit = None;
            return;
        }
        layer.commit();
        self.frame_pending = true;
        self.dirty = false;
    }

    fn finish(&mut self) {
        match self.exit.take() {
            Some(value) => {
                if self.run {
                    let _ = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(&value)
                        .stdin(std::process::Stdio::null())
                        .spawn();
                    exit(0);
                }
                println!("{value}");
                exit(0);
            }
            None => exit(1),
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
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl LayerShellHandler for App {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = None;
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
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
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
        event: KeyEvent,
    ) {
        self.on_key(event.keysym, event.utf8);
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
        _: Modifiers,
        _: RawModifiers,
        _: u32,
    ) {
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