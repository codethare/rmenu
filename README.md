# rmenu

A [wmenu](https://codeberg.org/adnano/wmenu)-style dynamic menu for Wayland,
written in Rust.

`rmenu` is a layer-shell dropdown menu: type to filter, arrows to move,
Enter to select, Escape to cancel. It renders text entirely in software
(wl_shm + its own rasterizer), so there are no cairo/pango dependencies —
the whole UI is drawn by `ab_glyph`.

## Features

- wmenu/dmenu-style filtering: every space-separated query token must be a
  substring; ranking is exact > prefix > substring
- `Ctrl`-free text input, `Up`/`Down`/`Ctrl-n`/`Ctrl-p`/`PgUp`/`PgDn`/`Home`/`End` navigation, key
  repeat, long-list scrolling, case-insensitive matching (`-i`), `Tab` completes
  the highlighted entry into the input; `Ctrl-c`/`Ctrl-g` cancel, `Ctrl-h`
  backspace, `Ctrl-j`/`Ctrl-m` accept, `Ctrl-u` clears the line, `Ctrl-w` kills
  the last word, `Shift-Return` submits exactly what was typed, and
  `Ctrl-Return` multi-selects (prints each pick, keeps running)
- Spotlight-style UI: rounded corners; launch shows only the input bar in the
  upper-center of the screen, results appear as you type, and the menu
  collapses back to the bar when the query is cleared
- `--run` launcher mode: scans `.desktop` files and launches the selection
  via `sh -c` — and merges in everything on `$PATH`, deduped with the desktop
  entries winning (like wmenu-run, files are listed by name — no per-entry
  exec check, so non-executables may appear)
- CJK text renders (auto-picks a CJK-capable system font)

## Build / run

```sh
cargo build --release
```

Modes:

```sh
# stdin → menu → selection printed to stdout
printf 'firefox\nFirefox\nalacritty\n' | rmenu

# launcher (desktop entries + PATH scripts/commands)
rmenu --run

# with filters
printf 'alacritty\nfirefox\n' | rmenu -i -p 'run: '
```

Options (wmenu-compatible subset): `-b` (menu at screen bottom), `-P` (mask typed
input as asterisks), `-i` (case-insensitive), `-l lines` (visible rows), `-W width`,
`-p prompt`, `-f font.ttf|"FAMILY [style] [pt|Npx]"` (bare size is points, `Npx` is
pixels — wmenu/Pango convention), `-v` (print version), `--run`, `-h`.

Colors are wmenu-style `RRGGBB[AA]`: `-N` normal bg, `-n` normal fg, `-M` prompt bg,
`-m` prompt fg, `-S` selection bg, `-s` selection fg. Example:

```sh
printf 'alacritty\nfirefox\n' | rmenu -N 111111 -n cccccc -S 005577 -s ffffff
```

### Sway

```conf
set $menu rmenu --run
bindsym $mod+d exec $menu
```

## Known limits

- No IME: typing CJK into the filter field needs a Wayland input-method
  protocol client, same as wmenu. Filtering CJK *labels* works fine.
- Long stdin lists are capped at 64 visible rows and scroll (keeps the shm
  buffer bounded); pass `-l N` for a fixed height.
- No xdg-activation token on launch (`--run`); windows may not grab focus on
  some compositors.