# rmenu

A [wmenu](https://codeberg.org/adnano/wmenu)-style dynamic menu for Wayland,
written in Rust — with program icons.

`rmenu` is a layer-shell dropdown menu: type to filter, arrows to move,
Enter to select, Escape to cancel. It renders text and icons entirely in
software (wl_shm + its own rasterizer), so there are no cairo/pango
dependencies — the whole UI is drawn by `ab_glyph` + `image`/`resvg`.

## Features

- wmenu/dmenu-style filtering: every space-separated query token must be a
  substring; ranking is exact > prefix > substring
- `Ctrl`-free text input, `Up`/`Down` navigation, key repeat, long-list
  scrolling, case-insensitive matching (`-i`)
- **Icons**: each line may carry an icon — a file path or a freedesktop
  icon-theme name (png/jpg/webp via `image`, svg via `resvg`, themes looked
  up in `hicolor`/`Adwaita`/`XDG_CURRENT_DESKTOP` and `~/.local/share/icons`)
- `--run` launcher mode: scans `.desktop` files, shows each program's icon,
  launches the selection via `sh -c`
- CJK text renders (auto-picks a CJK-capable system font)

## Build / run

```sh
cargo build --release
```

Modes:

```sh
# stdin → menu → selection printed to stdout
printf 'firefox\tFirefox\norg.gnome.Terminal\tTerminal\n' | rmenu

# launcher (programs + icons)
rmenu --run

# with a plain dmenu-style pipe
printf 'alacritty\nfirefox\n' | rmenu -i -p 'run: '
```

Options (wmenu-compatible subset): `-p prompt`, `-i` (case-insensitive),
`-l lines` (visible rows), `-W width`, `-f font.ttf`, `--run`, `-h`.

### Sway

```conf
set $menu rmenu --run
bindsym $mod+d exec $menu
```

### Input format

Lines from stdin are `icon<TAB>label`. The icon field is either a path
(`/usr/share/icons/hicolor/48x48/apps/firefox.png`) or a theme icon name
(`firefox`). On selection, the label (text after the tab) is printed to
stdout; plain lines are printed as-is, like dmenu.

## Known limits

- No IME: typing CJK into the filter field needs a Wayland input-method
  protocol client, same as wmenu. Filtering CJK *labels* works fine.
- Long stdin lists are capped at 64 visible rows and scroll (keeps the shm
  buffer bounded); pass `-l N` for a fixed height.
- No xdg-activation token on launch (`--run`); windows may not grab focus on
  some compositors.