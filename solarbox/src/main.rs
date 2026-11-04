mod api;
mod app;
mod editor;
mod gfx;
mod input;
mod settings;

use api::Console;
use app::App;
use gfx::{H, W};
use input::{EKey, FrameInput};
use minifb::{InputCallback, Key, KeyRepeat, Scale, Window, WindowOptions};
use settings::Settings;
use std::{cell::RefCell, io::Write, process::exit, rc::Rc, time::Instant};

fn usage() -> ! {
    eprintln!(
        "usage: solarbox [cart.lua] [--carts <dir>] [--edit <cart.lua>]\n\
         \x20      solarbox ... --headless <out.ppm> [--frames N] [--keys UDLRZXE...] [--script file]\n\
         \n\
         With no cart, opens the cart menu. In the menu: E edits, N makes a new cart.\n\
         In a cart: Esc returns (to the editor if it was started from there), F5 restarts."
    );
    exit(2)
}

// ---- window and input ------------------------------------------------------

type CharQueue = Rc<RefCell<Vec<char>>>;

struct CharSink(CharQueue);

impl InputCallback for CharSink {
    fn add_char(&mut self, c: u32) {
        if let Some(ch) = char::from_u32(c) {
            self.0.borrow_mut().push(ch);
        }
    }
}

/// Target fps for the window. The cap only applies while a cart runs: menus,
/// settings and the editor always run at 60, because their key repeat is counted
/// in frames and an uncapped loop would make every key press fire hundreds of times.
fn target_fps(running: bool, cap: u8) -> usize {
    if running {
        cap as usize // 0 = no limit
    } else {
        60
    }
}

fn make_window(scale: u8, sink: &CharQueue) -> Window {
    let scale = match scale {
        1 => Scale::X1,
        2 => Scale::X2,
        8 => Scale::X8,
        0 => Scale::FitScreen,
        _ => Scale::X4,
    };
    let mut w = Window::new("SolarBox", W, H, WindowOptions { scale, ..WindowOptions::default() })
        .unwrap_or_else(|e| {
            eprintln!("can't open window: {e}");
            exit(1)
        });
    w.set_target_fps(60);
    w.set_input_callback(Box::new(CharSink(sink.clone())));
    w
}

fn ekey(k: Key) -> Option<EKey> {
    Some(match k {
        Key::Left => EKey::Left,
        Key::Right => EKey::Right,
        Key::Up => EKey::Up,
        Key::Down => EKey::Down,
        Key::Home => EKey::Home,
        Key::End => EKey::End,
        Key::PageUp => EKey::PageUp,
        Key::PageDown => EKey::PageDown,
        Key::Backspace => EKey::Backspace,
        Key::Delete => EKey::Delete,
        Key::Enter | Key::NumPadEnter => EKey::Enter,
        Key::Tab => EKey::Tab,
        _ => return None,
    })
}

fn letter(k: Key) -> Option<char> {
    use Key::*;
    Some(match k {
        A => 'a', B => 'b', C => 'c', D => 'd', E => 'e', F => 'f', G => 'g', H => 'h',
        I => 'i', J => 'j', K => 'k', L => 'l', M => 'm', N => 'n', O => 'o', P => 'p',
        Q => 'q', R => 'r', S => 's', T => 't', U => 'u', V => 'v', W => 'w', X => 'x',
        Y => 'y', Z => 'z',
        _ => return None,
    })
}

/// US-layout fallback, used only if the window backend delivers no characters.
fn key_to_char(k: Key, shift: bool) -> Option<char> {
    use Key::*;
    if let Some(l) = letter(k) {
        return Some(if shift { l.to_ascii_uppercase() } else { l });
    }
    let pair = |a: char, b: char| Some(if shift { b } else { a });
    match k {
        Key0 => pair('0', ')'), Key1 => pair('1', '!'), Key2 => pair('2', '@'),
        Key3 => pair('3', '#'), Key4 => pair('4', '$'), Key5 => pair('5', '%'),
        Key6 => pair('6', '^'), Key7 => pair('7', '&'), Key8 => pair('8', '*'),
        Key9 => pair('9', '('),
        Space => Some(' '),
        Apostrophe => pair('\'', '"'), Comma => pair(',', '<'), Minus => pair('-', '_'),
        Period => pair('.', '>'), Slash => pair('/', '?'), Semicolon => pair(';', ':'),
        Equal => pair('=', '+'), LeftBracket => pair('[', '{'), RightBracket => pair(']', '}'),
        Backslash => pair('\\', '|'), Backquote => pair('`', '~'),
        _ => None,
    }
}

fn collect_input(window: &Window, sink: &CharQueue) -> FrameInput {
    let down = |a: Key, b: Key| window.is_key_down(a) || window.is_key_down(b);
    let ctrl = down(Key::LeftCtrl, Key::RightCtrl);
    let shift = down(Key::LeftShift, Key::RightShift);
    let mut inp = FrameInput {
        esc: window.is_key_pressed(Key::Escape, KeyRepeat::No),
        reload: window.is_key_pressed(Key::F5, KeyRepeat::No),
        ctrl,
        shift,
        ..FrameInput::default()
    };
    let typed: Vec<char> = sink.borrow_mut().drain(..).collect();
    let repeated = window.get_keys_pressed(KeyRepeat::Yes);
    inp.keys = repeated.iter().filter_map(|k| ekey(*k)).collect();
    if ctrl {
        inp.shortcuts =
            window.get_keys_pressed(KeyRepeat::No).into_iter().filter_map(letter).collect();
    } else if !typed.is_empty() {
        inp.chars = typed.into_iter().filter(|c| *c >= ' ' && *c != '\u{7f}').collect();
    } else {
        inp.chars = repeated.iter().filter_map(|k| key_to_char(*k, shift)).collect();
    }
    inp
}

// ---- headless scripting ----------------------------------------------------

fn feed(con: &Rc<RefCell<Console>>, down: [bool; 6]) {
    let mut s = con.borrow_mut();
    s.prev = s.down;
    s.down = down;
}

fn tick_with(app: &mut App, con: &Rc<RefCell<Console>>, inp: FrameInput) {
    feed(con, [false; 6]);
    app.tick(&inp);
}

fn parse_ekey(name: &str) -> Option<EKey> {
    Some(match name {
        "left" => EKey::Left,
        "right" => EKey::Right,
        "up" => EKey::Up,
        "down" => EKey::Down,
        "home" => EKey::Home,
        "end" => EKey::End,
        "pgup" => EKey::PageUp,
        "pgdn" => EKey::PageDown,
        "bksp" => EKey::Backspace,
        "del" => EKey::Delete,
        "enter" => EKey::Enter,
        "tab" => EKey::Tab,
        _ => return None,
    })
}

/// One scripted step. Script lines: `key <udlrzx>...`, `esc`, `f5`, `type <text>`,
/// `ek [shift] [ctrl] <name>`, `ctrl [shift] <letter>`, `frames <n>`,
/// `hold <udlrzx> <frames>` (hold buttons down for that many frames).
fn run_script_line(app: &mut App, con: &Rc<RefCell<Console>>, line: &str) {
    let (cmd, rest) = line.split_once(' ').unwrap_or((line, ""));
    let button = |c: char| match c.to_ascii_lowercase() {
        'l' => Some(0),
        'r' => Some(1),
        'u' => Some(2),
        'd' => Some(3),
        'z' => Some(4),
        'x' => Some(5),
        _ => None,
    };
    match cmd {
        "key" => {
            for c in rest.chars() {
                let Some(b) = button(c) else { continue };
                let mut down = [false; 6];
                down[b] = true;
                feed(con, down);
                app.tick(&FrameInput::default());
                tick_with(app, con, FrameInput::default());
            }
        }
        "hold" => {
            let (keys, n) = rest.split_once(' ').unwrap_or((rest, "1"));
            let mut down = [false; 6];
            for c in keys.chars() {
                if let Some(b) = button(c) {
                    down[b] = true;
                }
            }
            for _ in 0..n.trim().parse::<u32>().unwrap_or(1) {
                feed(con, down);
                app.tick(&FrameInput::default());
            }
            tick_with(app, con, FrameInput::default());
        }
        "esc" => tick_with(app, con, FrameInput { esc: true, ..Default::default() }),
        "f5" => tick_with(app, con, FrameInput { reload: true, ..Default::default() }),
        "type" => {
            tick_with(app, con, FrameInput { chars: rest.chars().collect(), ..Default::default() })
        }
        "ek" => {
            let mut inp = FrameInput::default();
            for tok in rest.split_whitespace() {
                match tok {
                    "shift" => inp.shift = true,
                    "ctrl" => inp.ctrl = true,
                    name => match parse_ekey(name) {
                        Some(k) => inp.keys.push(k),
                        None => eprintln!("script: unknown key {name}"),
                    },
                }
            }
            tick_with(app, con, inp);
        }
        "ctrl" => {
            let mut inp = FrameInput { ctrl: true, ..Default::default() };
            for tok in rest.split_whitespace() {
                if tok == "shift" {
                    inp.shift = true;
                } else if let Some(c) = tok.chars().next() {
                    inp.shortcuts.push(c);
                }
            }
            tick_with(app, con, inp);
        }
        "frames" => {
            for _ in 0..rest.trim().parse::<u32>().unwrap_or(1) {
                tick_with(app, con, FrameInput::default());
            }
        }
        other => eprintln!("script: unknown command {other}"),
    }
}

fn main() {
    let mut cart: Option<String> = None;
    let mut edit: Option<String> = None;
    let mut carts: Option<String> = None;
    let mut headless: Option<String> = None;
    let mut frames = 1u32;
    let mut keys = String::new();
    let mut script: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--carts" => carts = Some(it.next().unwrap_or_else(|| usage())),
            "--edit" => edit = Some(it.next().unwrap_or_else(|| usage())),
            "--headless" => headless = Some(it.next().unwrap_or_else(|| usage())),
            "--frames" => frames = it.next().and_then(|v| v.parse().ok()).unwrap_or_else(|| usage()),
            "--keys" => keys = it.next().unwrap_or_else(|| usage()),
            "--script" => script = Some(it.next().unwrap_or_else(|| usage())),
            s if s.starts_with("--") => usage(),
            _ if cart.is_none() => cart = Some(a),
            _ => usage(),
        }
    }

    let con = Rc::new(RefCell::new(Console::new()));
    let is_headless = headless.is_some();
    let settings = if is_headless { Settings::default() } else { Settings::load() };
    let mut app = App::new(settings, con.clone(), cart, edit, carts, !is_headless);
    let mut buf = vec![0u32; W * H];

    if let Some(out) = headless {
        for k in keys.chars() {
            if k.eq_ignore_ascii_case(&'e') {
                run_script_line(&mut app, &con, "esc");
            } else {
                run_script_line(&mut app, &con, &format!("key {k}"));
            }
        }
        if let Some(path) = script {
            let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("can't read script {path}: {e}");
                exit(1)
            });
            for line in text.lines() {
                if !line.trim().is_empty() && !line.starts_with('#') {
                    run_script_line(&mut app, &con, line);
                }
            }
        }
        for _ in 0..frames {
            tick_with(&mut app, &con, FrameInput::default());
        }
        con.borrow().gfx.to_rgb(&mut buf);
        let mut f = std::fs::File::create(&out).expect("create output");
        write!(f, "P6\n{W} {H}\n255\n").unwrap();
        for p in &buf {
            f.write_all(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]).unwrap();
        }
        println!("wrote {out}");
        return;
    }

    let sink: CharQueue = Rc::new(RefCell::new(Vec::new()));
    let mut cur_scale = app.settings.scale;
    let mut cur_fps = 60usize;
    let mut window = make_window(cur_scale, &sink);
    let keymap = [Key::Left, Key::Right, Key::Up, Key::Down, Key::Z, Key::X];
    let (mut last, mut fps) = (Instant::now(), 60.0f64);

    while window.is_open() {
        let mut down = [false; 6];
        for (i, k) in keymap.iter().enumerate() {
            down[i] = window.is_key_down(*k);
        }
        feed(&con, down);
        let inp = collect_input(&window, &sink);
        if app.tick(&inp) {
            break;
        }

        if app.settings.scale != cur_scale {
            cur_scale = app.settings.scale;
            window = make_window(cur_scale, &sink);
            cur_fps = 60;
        }
        let want = target_fps(app.is_running(), app.settings.fps_cap);
        if want != cur_fps {
            cur_fps = want;
            window.set_target_fps(want);
        }

        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64().max(1e-6);
        last = now;
        fps = fps * 0.95 + (1.0 / dt) * 0.05;

        if app.settings.show_fps {
            let mut overlay = con.borrow().gfx.clone();
            let label = format!("{:.0}", fps);
            overlay.rectfill(0, 0, label.len() as i32 * 4, 6, 0);
            overlay.print(&label, 1, 1, 11);
            overlay.to_rgb(&mut buf);
        } else {
            con.borrow().gfx.to_rgb(&mut buf);
        }
        window.update_with_buffer(&buf, W, H).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::target_fps;

    #[test]
    fn cap_applies_only_to_running_carts() {
        assert_eq!(target_fps(true, 30), 30);
        assert_eq!(target_fps(true, 60), 60);
        assert_eq!(target_fps(true, 0), 0); // uncapped cart
        // Menus never go uncapped or slow, whatever the cap is.
        assert_eq!(target_fps(false, 0), 60);
        assert_eq!(target_fps(false, 30), 60);
    }
}
