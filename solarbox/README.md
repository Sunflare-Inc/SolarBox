# SolarBox
A small Lua fantasy console. 128x128, 16 colours, 60 fps.

    cargo run --release                          # opens the cart menu (reads ./carts)
    cargo run --release -- carts/hello.lua       # run one cart directly (Esc quits)
    cargo run --release -- --carts ~/solarbox    # use another carts folder for this run

## Menu and settings
- Menu: Up/Down choose, Z play, X settings, Esc quit. Lists every `.lua` file in the carts folder.
- In a cart: Esc returns to the menu, F5 restarts the cart.
- Carts over 128 KB show a "big program" warning first. There is no size limit.
- Settings: window scale (1x/2x/4x/8x/fit), cart frame cap (60, 30 or none), FPS counter, reset to defaults.
  The cap only applies while a cart runs; menus, settings and the editor always run at 60.
  At 30 fps a cart that uses `_update60` runs at half speed.
  Saved to `~/.config/solarbox/settings.cfg` (or `$XDG_CONFIG_HOME/solarbox/settings.cfg`).
  The carts folder is changed with `carts_dir=` in that file or `--carts` for one run.
- Errors in a cart show on screen instead of closing the console.

## Code editor
Menu: E edits the selected cart, N makes a new one (from a template) and opens it.
`solarbox --edit carts/foo.lua` opens the editor directly.
- Lua syntax highlighting, line numbers, auto-indent after then/do/else/function.
- Ctrl+S save, Ctrl+R save and run (Esc in the cart returns to the editor; if the cart
  crashed, the cursor jumps to the error line), Esc leaves (asks twice if unsaved).
- Shift+arrows select, Ctrl+C/X/V copy/cut/paste (internal clipboard; with no selection
  they copy/cut the line), Ctrl+Z / Ctrl+Y undo/redo, Ctrl+D duplicate line, Ctrl+A select all,
  Ctrl+Left/Right jump words, Home/End/PageUp/PageDown, Tab / Shift+Tab indent.
- Tabs in a file are turned into two spaces when it is opened.

## Carts
Keys: arrows, Z (button 4), X (button 5). Callbacks: _init, _update60 (or _update), _draw.
API: cls color pset pget line rect rectfill circ circfill print btn btnp time t
Maths follows PICO-8 (sin/cos take turns, sin is inverted). Plain Lua 5.4 syntax only for now (no +=).
Carts run without io/os, so they can't touch your files.

## Testing without a window
    solarbox --carts carts --headless out.ppm --keys ddz --frames 30
    solarbox --carts carts --headless out.ppm --script steps.txt
`--keys`: U D L R Z X, E = Esc (one frame each). Writes the final 128x128 frame as a PPM.
Script lines: `key <udlrzx>`, `hold <udlrzx> <frames>`, `esc`, `f5`, `type <text>`,
`ek [shift] [ctrl] <left|right|up|down|home|end|pgup|pgdn|bksp|del|enter|tab>`,
`ctrl [shift] <letter>`, `frames <n>`.

## Planned cart format
Single text file with sections (like `.p8`): `__lua__`, `__gfx__`, `__map__`, `__sfx__`, ...
Not built yet; carts are plain `.lua` until sprites exist.
