//! The console shell: cart menu, settings, warnings, errors, and running a cart.

use crate::api::{self, Console};
use crate::editor::{Action, Editor};
use crate::gfx::{Gfx, W};
use crate::input::{EKey, FrameInput};
use crate::settings::{Settings, FPS_CAPS, SCALES};
use mlua::{Function, Lua, LuaOptions, StdLib};
use std::{cell::RefCell, fs, path::{Path, PathBuf}, rc::Rc};

/// Carts have no size limit. Past this many bytes we only show a warning.
pub const BIG_CART_WARN_BYTES: usize = 128 * 1024;

const ROWS: usize = 11; // visible cart rows in the menu
const SETTING_ROWS: usize = 4;

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Menu,
    Settings,
    Warn,
    Running,
    Error,
    Editor,
    NewCart,
}

struct Entry {
    name: String,
    path: PathBuf,
    bytes: usize,
}

pub struct App {
    pub settings: Settings,
    con: Rc<RefCell<Console>>,
    screen: Screen,
    carts_dir: String,
    persist: bool,
    quit_on_exit: bool, // launched with a cart/--edit on the command line: Esc quits
    editor: Option<Editor>,
    from_editor: bool, // the running cart was started from the editor
    err_line: Option<usize>,
    new_name: String,
    new_msg: String,
    entries: Vec<Entry>,
    sel: usize,
    top: usize,
    set_sel: usize,
    hold: [u32; 4],
    lua: Option<Lua>,
    current: Option<PathBuf>,
    warn_path: PathBuf,
    warn_kb: usize,
    err_lines: Vec<String>,
}

impl App {
    pub fn new(
        settings: Settings,
        con: Rc<RefCell<Console>>,
        direct_cart: Option<String>,
        edit_cart: Option<String>,
        carts_override: Option<String>,
        persist: bool,
    ) -> App {
        let carts_dir = carts_override.unwrap_or_else(|| settings.carts_dir.clone());
        let mut app = App {
            settings,
            con,
            screen: Screen::Menu,
            carts_dir,
            persist,
            quit_on_exit: direct_cart.is_some() || edit_cart.is_some(),
            editor: None,
            from_editor: false,
            err_line: None,
            new_name: String::new(),
            new_msg: String::new(),
            entries: Vec::new(),
            sel: 0,
            top: 0,
            set_sel: 0,
            hold: [0; 4],
            lua: None,
            current: None,
            warn_path: PathBuf::new(),
            warn_kb: 0,
            err_lines: Vec::new(),
        };
        app.rescan();
        if let Some(c) = edit_cart {
            app.open_editor(PathBuf::from(c));
        } else if let Some(c) = direct_cart {
            app.launch(PathBuf::from(c), false);
        }
        app
    }

    /// True while a cart is running (the frame cap only applies then).
    pub fn is_running(&self) -> bool {
        self.screen == Screen::Running
    }

    /// Advance one frame. Returns true to quit.
    pub fn tick(&mut self, inp: &FrameInput) -> bool {
        let esc = inp.esc;
        match self.screen {
            Screen::Menu => self.tick_menu(inp),
            Screen::Settings => self.tick_settings(esc),
            Screen::Warn => self.tick_warn(esc),
            Screen::Error => self.tick_error(esc),
            Screen::Running => self.tick_running(esc, inp.reload),
            Screen::Editor => self.tick_editor(inp),
            Screen::NewCart => self.tick_new_cart(inp),
        }
    }

    // ---- input helpers -------------------------------------------------

    fn pressed(&self, b: usize) -> bool {
        let c = self.con.borrow();
        c.down[b] && !c.prev[b]
    }

    /// Arrow-key press with hold-to-repeat.
    fn nav(&mut self, b: usize) -> bool {
        if !self.con.borrow().down[b] {
            self.hold[b] = 0;
            return false;
        }
        self.hold[b] += 1;
        let h = self.hold[b];
        h == 1 || (h > 18 && (h - 18) % 4 == 0)
    }

    fn changed(&mut self) {
        if self.persist {
            let _ = self.settings.save();
        }
    }

    // ---- cart handling -------------------------------------------------

    fn rescan(&mut self) {
        self.entries.clear();
        if let Ok(rd) = fs::read_dir(&self.carts_dir) {
            for e in rd.flatten() {
                let p = e.path();
                let is_lua = p
                    .extension()
                    .and_then(|x| x.to_str())
                    .map_or(false, |x| x.eq_ignore_ascii_case("lua"));
                if is_lua && p.is_file() {
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let bytes = e.metadata().map(|m| m.len() as usize).unwrap_or(0);
                    self.entries.push(Entry { name, path: p, bytes });
                }
            }
        }
        self.entries.sort_by_key(|e| e.name.to_lowercase());
        self.sel = self.sel.min(self.entries.len().saturating_sub(1));
    }

    fn launch(&mut self, path: PathBuf, skip_warn: bool) {
        let bytes = fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
        if !skip_warn && bytes > BIG_CART_WARN_BYTES {
            self.warn_kb = bytes / 1024;
            self.warn_path = path;
            self.screen = Screen::Warn;
            return;
        }
        match self.load(&path) {
            Ok(lua) => {
                self.lua = Some(lua);
                self.current = Some(path);
                self.screen = Screen::Running;
            }
            Err(msg) => self.fail(&msg),
        }
    }

    fn load(&self, path: &Path) -> Result<Lua, String> {
        let src = fs::read_to_string(path)
            .map_err(|e| format!("can't read {}: {e}", path.display()))?;
        self.con.borrow_mut().reset();
        // No io/os/package/debug: carts can't touch the host.
        let libs = StdLib::MATH | StdLib::STRING | StdLib::TABLE | StdLib::UTF8 | StdLib::COROUTINE;
        let lua = Lua::new_with(libs, LuaOptions::default()).map_err(|e| e.to_string())?;
        api::install(&lua, &self.con).map_err(|e| e.to_string())?;
        let name = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        lua.load(&src).set_name(format!("@{name}")).exec().map_err(|e| e.to_string())?;
        call_if_defined(&lua, "_init").map_err(|e| format!("_init: {e}"))?;
        Ok(lua)
    }

    fn fail(&mut self, msg: &str) {
        eprintln!("cart error: {msg}");
        self.err_line = error_line(msg);
        let msg = msg.split("stack traceback:").next().unwrap_or(msg);
        self.err_lines = wrap(msg.trim(), 30);
        self.lua = None;
        self.screen = Screen::Error;
    }

    /// Leave a cart/warning/error screen: back to the editor it was started
    /// from, else quit (direct launch), else the menu.
    fn back_to_menu(&mut self) -> bool {
        self.lua = None;
        if self.from_editor && self.editor.is_some() {
            self.from_editor = false;
            if let (Some(line), Some(ed)) = (self.err_line.take(), self.editor.as_mut()) {
                ed.show_error(line);
            }
            self.screen = Screen::Editor;
            return false;
        }
        if self.quit_on_exit {
            return true;
        }
        self.rescan();
        self.screen = Screen::Menu;
        false
    }

    fn open_editor(&mut self, path: PathBuf) {
        match Editor::open(path) {
            Ok(ed) => {
                self.editor = Some(ed);
                self.from_editor = false;
                self.screen = Screen::Editor;
            }
            Err(msg) => self.fail(&msg),
        }
    }

    fn tick_editor(&mut self, inp: &FrameInput) -> bool {
        let Some(mut ed) = self.editor.take() else {
            self.screen = Screen::Menu;
            return false;
        };
        let act = {
            let mut c = self.con.borrow_mut();
            ed.tick(inp, &mut c.gfx)
        };
        let path = ed.path.clone();
        self.editor = Some(ed);
        match act {
            Action::None => false,
            Action::Exit => {
                self.editor = None;
                if self.quit_on_exit {
                    return true;
                }
                self.rescan();
                self.screen = Screen::Menu;
                false
            }
            Action::Run => {
                self.from_editor = true;
                self.err_line = None;
                self.launch(path, false);
                false
            }
        }
    }

    fn tick_new_cart(&mut self, inp: &FrameInput) -> bool {
        if inp.esc {
            self.screen = Screen::Menu;
            return false;
        }
        for c in &inp.chars {
            if (c.is_ascii_alphanumeric() || *c == '_' || *c == '-') && self.new_name.len() < 24 {
                self.new_name.push(*c);
                self.new_msg.clear();
            }
        }
        if inp.keys.contains(&EKey::Backspace) {
            self.new_name.pop();
            self.new_msg.clear();
        }
        if inp.keys.contains(&EKey::Enter) && !self.new_name.is_empty() {
            let path = Path::new(&self.carts_dir).join(format!("{}.lua", self.new_name));
            if path.exists() {
                self.new_msg = "THAT NAME ALREADY EXISTS".into();
            } else {
                let template = format!(
                    "-- {n}\n\nfunction _init()\nend\n\nfunction _update60()\nend\n\n\
                     function _draw()\n  cls(1)\n  print(\"{n}\", 4, 4, 7)\nend\n",
                    n = self.new_name
                );
                let made = fs::create_dir_all(&self.carts_dir).and_then(|_| fs::write(&path, template));
                match made {
                    Ok(()) => {
                        self.rescan();
                        if let Some(i) = self.entries.iter().position(|e| e.path == path) {
                            self.sel = i;
                        }
                        self.open_editor(path);
                        return false;
                    }
                    Err(e) => self.new_msg = format!("CAN'T CREATE: {e}"),
                }
            }
        }
        self.draw_new_cart();
        false
    }

    // ---- per-screen ticks ---------------------------------------------

    fn tick_menu(&mut self, inp: &FrameInput) -> bool {
        if inp.esc {
            return true;
        }
        if inp.chars.iter().any(|c| matches!(c, 'n' | 'N')) {
            self.new_name.clear();
            self.new_msg.clear();
            self.screen = Screen::NewCart;
            return false;
        }
        if inp.chars.iter().any(|c| matches!(c, 'e' | 'E')) && !self.entries.is_empty() {
            let path = self.entries[self.sel].path.clone();
            self.open_editor(path);
            return false;
        }
        let len = self.entries.len();
        if len > 0 {
            if self.nav(3) {
                self.sel = (self.sel + 1) % len;
            }
            if self.nav(2) {
                self.sel = (self.sel + len - 1) % len;
            }
            if self.sel < self.top {
                self.top = self.sel;
            } else if self.sel >= self.top + ROWS {
                self.top = self.sel + 1 - ROWS;
            }
            if self.pressed(4) {
                let path = self.entries[self.sel].path.clone();
                self.launch(path, false);
                if self.screen != Screen::Menu {
                    return false;
                }
            }
        }
        if self.pressed(5) {
            self.screen = Screen::Settings;
            return false;
        }
        self.draw_menu();
        false
    }

    fn tick_settings(&mut self, esc: bool) -> bool {
        if esc || self.pressed(5) {
            self.screen = Screen::Menu;
            return false;
        }
        if self.nav(3) {
            self.set_sel = (self.set_sel + 1) % SETTING_ROWS;
        }
        if self.nav(2) {
            self.set_sel = (self.set_sel + SETTING_ROWS - 1) % SETTING_ROWS;
        }
        let dir: i32 = if self.nav(1) { 1 } else if self.nav(0) { -1 } else { 0 };
        let mut changed = false;
        match self.set_sel {
            0 if dir != 0 => {
                let i = SCALES.iter().position(|s| *s == self.settings.scale).unwrap_or(2) as i32;
                let n = SCALES.len() as i32;
                self.settings.scale = SCALES[((i + dir + n) % n) as usize];
                changed = true;
            }
            1 if dir != 0 => {
                let i = FPS_CAPS.iter().position(|c| *c == self.settings.fps_cap).unwrap_or(0) as i32;
                let n = FPS_CAPS.len() as i32;
                self.settings.fps_cap = FPS_CAPS[((i + dir + n) % n) as usize];
                changed = true;
            }
            2 if dir != 0 => {
                self.settings.show_fps = !self.settings.show_fps;
                changed = true;
            }
            3 if self.pressed(4) => {
                let dir = std::mem::take(&mut self.settings.carts_dir);
                self.settings = Settings { carts_dir: dir, ..Settings::default() };
                changed = true;
            }
            _ => {}
        }
        if changed {
            self.changed();
        }
        self.draw_settings();
        false
    }

    fn tick_warn(&mut self, esc: bool) -> bool {
        if self.pressed(4) {
            let p = self.warn_path.clone();
            self.launch(p, true);
            return false;
        }
        if esc || self.pressed(5) {
            return self.back_to_menu();
        }
        self.draw_warn();
        false
    }

    fn tick_error(&mut self, esc: bool) -> bool {
        if esc || self.pressed(4) || self.pressed(5) {
            return self.back_to_menu();
        }
        self.draw_error();
        false
    }

    fn tick_running(&mut self, esc: bool, reload: bool) -> bool {
        if esc {
            return self.back_to_menu();
        }
        if reload {
            if let Some(p) = self.current.clone() {
                self.launch(p, true);
                return false;
            }
        }
        let res = self.lua.as_ref().map(step);
        if let Some(Err(e)) = res {
            self.fail(&e.to_string());
        }
        false
    }

    // ---- drawing -------------------------------------------------------

    fn draw_menu(&mut self) {
        let mut c = self.con.borrow_mut();
        let g = &mut c.gfx;
        g.cls(1);
        center(g, "SOLARBOX", 3, 10);
        g.rectfill(0, 10, 127, 10, 13);
        if self.entries.is_empty() {
            center(g, "NO CARTS FOUND", 40, 8);
            center(g, "PUT .LUA FILES IN:", 52, 6);
            for (i, l) in wrap(&self.carts_dir, 30).iter().take(4).enumerate() {
                center(g, l, 62 + i as i32 * 7, 7);
            }
        }
        for i in 0..ROWS {
            let idx = self.top + i;
            let Some(e) = self.entries.get(idx) else { break };
            let y = 14 + i as i32 * 8;
            let (fg, size_fg) = if idx == self.sel {
                g.rectfill(0, y - 1, 127, y + 6, 9);
                (0, 0)
            } else {
                (7, 13)
            };
            g.print(&trunc(&e.name, 24), 4, y, fg);
            let size = if e.bytes < 1024 { format!("{}B", e.bytes) } else { format!("{}K", e.bytes / 1024) };
            g.print(&size, 118 - size.len() as i32 * 4, y, size_fg);
        }
        if self.top > 0 {
            g.print("^", 122, 11, 6);
        }
        if self.top + ROWS < self.entries.len() {
            g.print("V", 122, 104, 6);
        }
        g.rectfill(0, 111, 127, 127, 0);
        center(g, "Z PLAY  X CONFIG  ESC QUIT", 113, 13);
        center(g, "E EDIT  N NEW CART", 121, 13);
    }

    fn draw_new_cart(&mut self) {
        let mut c = self.con.borrow_mut();
        let g = &mut c.gfx;
        g.cls(1);
        center(g, "NEW CART", 3, 10);
        g.rectfill(0, 10, 127, 10, 13);
        g.print("NAME:", 8, 30, 6);
        g.rectfill(8, 40, 119, 50, 0);
        g.rect(8, 40, 119, 50, 13);
        let mut x = 11;
        for ch in self.new_name.chars() {
            g.put_char(ch, x, 43, 7);
            x += 4;
        }
        g.rectfill(x, 42, x + 2, 48, 10);
        g.print(".LUA", 8, 54, 13);
        if !self.new_msg.is_empty() {
            for (i, l) in wrap(&self.new_msg, 30).iter().take(3).enumerate() {
                g.print(l, 8, 66 + i as i32 * 7, 8);
            }
        }
        let dir = format!("IN {}", self.carts_dir);
        for (i, l) in wrap(&dir, 30).iter().take(3).enumerate() {
            g.print(l, 8, 92 + i as i32 * 7, 13);
        }
        g.rectfill(0, 117, 127, 127, 0);
        center(g, "ENTER CREATE  ESC CANCEL", 121, 13);
    }

    fn draw_settings(&mut self) {
        let s = &self.settings;
        let rows: [(&str, String); SETTING_ROWS] = [
            ("WINDOW SCALE", if s.scale == 0 { "FIT".into() } else { format!("{}X", s.scale) }),
            ("CART FRAME CAP", if s.fps_cap == 0 { "NONE".into() } else { format!("{} FPS", s.fps_cap) }),
            ("SHOW FPS", if s.show_fps { "ON".into() } else { "OFF".into() }),
            ("RESET DEFAULTS", String::new()),
        ];
        let cfg = if self.persist {
            Settings::path().map(|p| p.display().to_string()).unwrap_or_else(|| "NOT SAVED".into())
        } else {
            "NOT SAVED".into()
        };
        let mut c = self.con.borrow_mut();
        let g = &mut c.gfx;
        g.cls(1);
        center(g, "SETTINGS", 3, 10);
        g.rectfill(0, 10, 127, 10, 13);
        for (i, (label, value)) in rows.iter().enumerate() {
            let y = 18 + i as i32 * 10;
            let sel = i == self.set_sel;
            if sel {
                g.rectfill(2, y - 2, 125, y + 6, 9);
            }
            let fg = if sel { 0 } else { 7 };
            g.print(label, 6, y, fg);
            if !value.is_empty() {
                let v = if sel { format!("<{value}>") } else { value.clone() };
                g.print(&v, 122 - v.len() as i32 * 4, y, fg);
            }
        }
        let mut y = 62;
        for (title, text) in [("CARTS FOLDER", &self.carts_dir), ("CONFIG FILE", &cfg)] {
            g.print(title, 4, y, 13);
            y += 7;
            for l in wrap(text, 30).iter().take(3) {
                g.print(l, 4, y, 6);
                y += 7;
            }
            y += 3;
        }
        g.rectfill(0, 117, 127, 127, 0);
        center(g, "UP DOWN LEFT RIGHT  X BACK", 121, 13);
    }

    fn draw_warn(&mut self) {
        let mut c = self.con.borrow_mut();
        let g = &mut c.gfx;
        g.cls(1);
        center(g, "BIG PROGRAM", 28, 9);
        center(g, &format!("{} KB", self.warn_kb), 40, 7);
        center(g, "THIS CART IS LARGE AND", 54, 6);
        center(g, "MAY LOAD OR RUN SLOWLY.", 62, 6);
        center(g, "Z RUN ANYWAY", 80, 11);
        center(g, "X BACK", 90, 8);
    }

    fn draw_error(&mut self) {
        let mut c = self.con.borrow_mut();
        let g = &mut c.gfx;
        g.cls(1);
        center(g, "CART ERROR", 3, 8);
        g.rectfill(0, 10, 127, 10, 13);
        for (i, l) in self.err_lines.iter().take(14).enumerate() {
            g.print(l, 4, 15 + i as i32 * 7, 7);
        }
        g.rectfill(0, 117, 127, 127, 0);
        center(g, "Z OR X OR ESC: BACK", 121, 13);
    }
}

// ---- free helpers ------------------------------------------------------

fn call_if_defined(lua: &Lua, name: &str) -> mlua::Result<bool> {
    if let Some(f) = lua.globals().get::<Option<Function>>(name)? {
        f.call::<()>(())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn step(lua: &Lua) -> mlua::Result<()> {
    if !call_if_defined(lua, "_update60")? {
        call_if_defined(lua, "_update")?;
    }
    call_if_defined(lua, "_draw")?;
    Ok(())
}

fn center(g: &mut Gfx, text: &str, y: i32, col: u8) {
    let w = text.chars().count() as i32 * 4 - 1;
    g.print(text, (W as i32 - w) / 2, y, col);
}

fn trunc(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('.');
        t
    }
}

fn wrap(s: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for raw in s.lines() {
        let mut line = String::new();
        for word in raw.split_whitespace() {
            let mut word = word.to_string();
            while word.chars().count() > width {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                out.push(word.chars().take(width).collect());
                word = word.chars().skip(width).collect();
            }
            let extra = if line.is_empty() { 0 } else { 1 };
            if !line.is_empty() && line.chars().count() + extra + word.chars().count() > width {
                out.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(&word);
        }
        out.push(line);
    }
    out
}

/// Pull the line number out of a Lua error like "cart.lua:12: attempt to ...".
fn error_line(msg: &str) -> Option<usize> {
    let i = msg.find(".lua:")? + 5;
    let digits: String = msg[i..].chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}
