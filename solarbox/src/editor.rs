//! The built-in code editor: Lua syntax highlighting, selection, undo/redo,
//! line numbers, auto-indent. Draws into the console's 128x128 screen.

use crate::gfx::{Gfx, W};
use crate::input::{EKey, FrameInput};
use std::{fs, path::PathBuf, rc::Rc};

type Line = Rc<Vec<char>>;

const ROW_H: i32 = 7;
const VIS_ROWS: usize = 17;
const MAX_UNDO: usize = 500;

// Syntax colours (palette indices).
const C_TEXT: u8 = 7;
const C_KEYWORD: u8 = 14;
const C_API: u8 = 12;
const C_NUMBER: u8 = 9;
const C_STRING: u8 = 11;
const C_COMMENT: u8 = 13;
const C_PUNCT: u8 = 6;

const KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];
const API_NAMES: &[&str] = &[
    "cls", "color", "pset", "pget", "line", "rect", "rectfill", "circ", "circfill", "print", "btn",
    "btnp", "time", "t", "flr", "ceil", "sin", "cos", "atan2", "sqrt", "abs", "min", "max", "sgn",
    "mid", "rnd", "srand", "add", "del", "count", "foreach", "all",
];

#[derive(Clone)]
struct Snap {
    lines: Vec<Line>,
    cx: usize,
    cy: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Idle,
    Typing,
    Deleting,
    Other,
}

pub enum Action {
    None,
    Exit,
    Run,
}

pub struct Editor {
    pub path: PathBuf,
    lines: Vec<Line>,
    cx: usize,
    cy: usize,
    want: usize,
    anchor: Option<(usize, usize)>,
    top: usize,
    left: usize,
    dirty: bool,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    kind: Kind,
    clip: String,
    msg: String,
    msg_t: u32,
    armed: bool,
    states: Vec<u16>,
    states_ok: bool,
    err_line: Option<usize>,
    blink: u32,
}

impl Editor {
    pub fn open(path: PathBuf) -> Result<Editor, String> {
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("can't read {}: {e}", path.display()))?;
        let text = text.replace("\r\n", "\n").replace('\r', "\n").replace('\t', "  ");
        let mut lines: Vec<Line> =
            text.split('\n').map(|l| Rc::new(l.chars().collect())).collect();
        if lines.len() > 1 && lines.last().map_or(false, |l| l.is_empty()) {
            lines.pop();
        }
        Ok(Editor {
            path,
            lines,
            cx: 0,
            cy: 0,
            want: 0,
            anchor: None,
            top: 0,
            left: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            kind: Kind::Idle,
            clip: String::new(),
            msg: String::new(),
            msg_t: 0,
            armed: false,
            states: Vec::new(),
            states_ok: false,
            err_line: None,
            blink: 0,
        })
    }

    pub fn show_error(&mut self, line: usize) {
        self.goto_line(line);
        self.err_line = Some(line);
        self.say(&format!("ERROR ON LINE {line}"), 240);
    }

    fn say(&mut self, m: &str, frames: u32) {
        self.msg = m.to_string();
        self.msg_t = frames;
    }

    fn goto_line(&mut self, line: usize) {
        self.cy = line.saturating_sub(1).min(self.lines.len() - 1);
        self.cx = self.lines[self.cy].iter().take_while(|c| **c == ' ').count();
        self.want = self.cx;
        self.anchor = None;
        self.top = self.cy.saturating_sub(VIS_ROWS / 2);
        self.ensure_visible();
    }

    // ---- text model --------------------------------------------------------

    fn line_mut(&mut self, y: usize) -> &mut Vec<char> {
        Rc::make_mut(&mut self.lines[y])
    }

    fn sel(&self) -> Option<((usize, usize), (usize, usize))> {
        let a = self.anchor?;
        let b = (self.cx, self.cy);
        if a == b {
            None
        } else if (a.1, a.0) <= (b.1, b.0) {
            Some((a, b))
        } else {
            Some((b, a))
        }
    }

    fn snapshot(&mut self) {
        self.undo.push(Snap { lines: self.lines.clone(), cx: self.cx, cy: self.cy });
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Called before every edit: records an undo step (typing/backspace runs share one).
    fn begin(&mut self, k: Kind) {
        if k == Kind::Other || k != self.kind {
            self.snapshot();
        }
        self.kind = if k == Kind::Other { Kind::Idle } else { k };
        self.dirty = true;
        self.states_ok = false;
        self.err_line = None;
        self.blink = 0;
    }

    fn delete_range(&mut self, a: (usize, usize), b: (usize, usize)) {
        if a.1 == b.1 {
            self.line_mut(a.1).drain(a.0..b.0);
        } else {
            let tail: Vec<char> = self.lines[b.1][b.0..].to_vec();
            let l = self.line_mut(a.1);
            l.truncate(a.0);
            l.extend(tail);
            self.lines.drain(a.1 + 1..=b.1);
        }
        self.cx = a.0;
        self.cy = a.1;
        self.want = a.0;
        self.anchor = None;
    }

    fn delete_selection(&mut self) -> bool {
        if let Some((a, b)) = self.sel() {
            self.delete_range(a, b);
            true
        } else {
            false
        }
    }

    fn selected_text(&self, a: (usize, usize), b: (usize, usize)) -> String {
        let mut s = String::new();
        for y in a.1..=b.1 {
            let l = &self.lines[y];
            let x0 = if y == a.1 { a.0 } else { 0 };
            let x1 = if y == b.1 { b.0 } else { l.len() };
            s.extend(l[x0..x1].iter());
            if y != b.1 {
                s.push('\n');
            }
        }
        s
    }

    fn newline_raw(&mut self, indent: usize) {
        let cx = self.cx;
        let tail = self.line_mut(self.cy).split_off(cx);
        let mut nl = vec![' '; indent];
        nl.extend(tail);
        self.lines.insert(self.cy + 1, Rc::new(nl));
        self.cy += 1;
        self.cx = indent;
    }

    fn insert_text(&mut self, s: &str) {
        for ch in s.chars() {
            match ch {
                '\n' => self.newline_raw(0),
                '\t' => self.insert_raw(' ', 2),
                c if c >= ' ' && c != '\u{7f}' => self.insert_raw(c, 1),
                _ => {}
            }
        }
        self.want = self.cx;
    }

    fn insert_raw(&mut self, c: char, n: usize) {
        let (cx, cy) = (self.cx, self.cy);
        let l = self.line_mut(cy);
        for _ in 0..n {
            l.insert(cx, c);
        }
        self.cx += n;
    }

    fn insert_char(&mut self, c: char) {
        if self.sel().is_some() {
            self.begin(Kind::Other);
            self.delete_selection();
        } else {
            self.begin(Kind::Typing);
        }
        self.insert_raw(c, 1);
        self.want = self.cx;
        self.ensure_visible();
    }

    fn enter(&mut self) {
        self.begin(Kind::Other);
        self.delete_selection();
        let before: String = self.lines[self.cy][..self.cx].iter().collect();
        let indent = before.chars().take_while(|c| *c == ' ').count();
        let extra = if opens_block(before.trim_end()) { 2 } else { 0 };
        self.newline_raw(indent + extra);
        self.want = self.cx;
        self.ensure_visible();
    }

    fn backspace(&mut self) {
        if self.sel().is_some() {
            self.begin(Kind::Other);
            self.delete_selection();
        } else if self.cx > 0 {
            self.begin(Kind::Deleting);
            let (cx, cy) = (self.cx, self.cy);
            self.line_mut(cy).remove(cx - 1);
            self.cx -= 1;
        } else if self.cy > 0 {
            self.begin(Kind::Other);
            let cur = self.lines.remove(self.cy);
            self.cy -= 1;
            self.cx = self.lines[self.cy].len();
            let cy = self.cy;
            self.line_mut(cy).extend(cur.iter());
        }
        self.want = self.cx;
        self.ensure_visible();
    }

    fn delete_key(&mut self) {
        if self.sel().is_some() {
            self.begin(Kind::Other);
            self.delete_selection();
        } else if self.cx < self.lines[self.cy].len() {
            self.begin(Kind::Deleting);
            let (cx, cy) = (self.cx, self.cy);
            self.line_mut(cy).remove(cx);
        } else if self.cy + 1 < self.lines.len() {
            self.begin(Kind::Other);
            let next = self.lines.remove(self.cy + 1);
            let cy = self.cy;
            self.line_mut(cy).extend(next.iter());
        }
        self.ensure_visible();
    }

    fn tab(&mut self, shift: bool) {
        if shift {
            let lead = self.lines[self.cy].iter().take_while(|c| **c == ' ').count().min(2);
            if lead > 0 {
                self.begin(Kind::Other);
                let cy = self.cy;
                self.line_mut(cy).drain(0..lead);
                self.cx = self.cx.saturating_sub(lead);
            }
        } else {
            self.begin(Kind::Other);
            self.delete_selection();
            self.insert_raw(' ', 2);
        }
        self.want = self.cx;
        self.ensure_visible();
    }

    // ---- cursor ------------------------------------------------------------

    fn set(&mut self, x: usize, y: usize, extend: bool, keep_want: bool) {
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some((self.cx, self.cy));
            }
        } else {
            self.anchor = None;
        }
        self.cy = y.min(self.lines.len() - 1);
        self.cx = x.min(self.lines[self.cy].len());
        if !keep_want {
            self.want = self.cx;
        }
        self.kind = Kind::Idle;
        self.blink = 0;
        self.ensure_visible();
    }

    fn word_left(&self) -> (usize, usize) {
        let (mut x, mut y) = (self.cx, self.cy);
        if x == 0 {
            return if y > 0 { (self.lines[y - 1].len(), y - 1) } else { (0, 0) };
        }
        let l = &self.lines[y];
        while x > 0 && !is_word(l[x - 1]) {
            x -= 1;
        }
        while x > 0 && is_word(l[x - 1]) {
            x -= 1;
        }
        y = self.cy;
        (x, y)
    }

    fn word_right(&self) -> (usize, usize) {
        let (mut x, y) = (self.cx, self.cy);
        let l = &self.lines[y];
        if x >= l.len() {
            return if y + 1 < self.lines.len() { (0, y + 1) } else { (x, y) };
        }
        while x < l.len() && !is_word(l[x]) {
            x += 1;
        }
        while x < l.len() && is_word(l[x]) {
            x += 1;
        }
        (x, y)
    }

    fn gutter(&self) -> i32 {
        self.lines.len().to_string().len().max(2) as i32 * 4 + 3
    }

    fn cols(&self) -> usize {
        ((W as i32 - self.gutter()) / 4).max(1) as usize
    }

    fn ensure_visible(&mut self) {
        if self.cy < self.top {
            self.top = self.cy;
        } else if self.cy >= self.top + VIS_ROWS {
            self.top = self.cy + 1 - VIS_ROWS;
        }
        let cols = self.cols();
        if self.cx < self.left {
            self.left = self.cx;
        } else if self.cx >= self.left + cols {
            self.left = self.cx + 1 - cols;
        }
    }

    fn key(&mut self, k: EKey, shift: bool, ctrl: bool) {
        let (cx, cy) = (self.cx, self.cy);
        match k {
            EKey::Left => {
                if let (false, Some((a, _))) = (shift, self.sel()) {
                    self.set(a.0, a.1, false, false);
                } else if ctrl {
                    let (x, y) = self.word_left();
                    self.set(x, y, shift, false);
                } else if cx > 0 {
                    self.set(cx - 1, cy, shift, false);
                } else if cy > 0 {
                    self.set(self.lines[cy - 1].len(), cy - 1, shift, false);
                }
            }
            EKey::Right => {
                if let (false, Some((_, b))) = (shift, self.sel()) {
                    self.set(b.0, b.1, false, false);
                } else if ctrl {
                    let (x, y) = self.word_right();
                    self.set(x, y, shift, false);
                } else if cx < self.lines[cy].len() {
                    self.set(cx + 1, cy, shift, false);
                } else if cy + 1 < self.lines.len() {
                    self.set(0, cy + 1, shift, false);
                }
            }
            EKey::Up => {
                if cy > 0 {
                    self.set(self.want, cy - 1, shift, true);
                } else {
                    self.set(0, 0, shift, false);
                }
            }
            EKey::Down => {
                if cy + 1 < self.lines.len() {
                    self.set(self.want, cy + 1, shift, true);
                } else {
                    self.set(usize::MAX, cy, shift, false);
                }
            }
            EKey::PageUp => self.set(self.want, cy.saturating_sub(VIS_ROWS - 1), shift, true),
            EKey::PageDown => self.set(self.want, cy + VIS_ROWS - 1, shift, true),
            EKey::Home => {
                if ctrl {
                    self.set(0, 0, shift, false);
                } else {
                    let first = self.lines[cy].iter().take_while(|c| **c == ' ').count();
                    self.set(if cx == first { 0 } else { first }, cy, shift, false);
                }
            }
            EKey::End => {
                if ctrl {
                    let last = self.lines.len() - 1;
                    self.set(usize::MAX, last, shift, false);
                } else {
                    self.set(usize::MAX, cy, shift, false);
                }
            }
            EKey::Backspace => self.backspace(),
            EKey::Delete => self.delete_key(),
            EKey::Enter => self.enter(),
            EKey::Tab => self.tab(shift),
        }
    }

    // ---- commands ----------------------------------------------------------

    fn restore(&mut self, s: Snap) {
        self.lines = s.lines;
        self.cx = s.cx;
        self.cy = s.cy.min(self.lines.len() - 1);
        self.cx = self.cx.min(self.lines[self.cy].len());
        self.want = self.cx;
        self.anchor = None;
        self.dirty = true;
        self.states_ok = false;
        self.kind = Kind::Idle;
        self.ensure_visible();
    }

    fn do_undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snap { lines: self.lines.clone(), cx: self.cx, cy: self.cy });
            self.restore(s);
        } else {
            self.say("NOTHING TO UNDO", 60);
        }
    }

    fn do_redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snap { lines: self.lines.clone(), cx: self.cx, cy: self.cy });
            self.restore(s);
        } else {
            self.say("NOTHING TO REDO", 60);
        }
    }

    fn copy(&mut self, cut: bool) {
        if let Some((a, b)) = self.sel() {
            self.clip = self.selected_text(a, b);
            if cut {
                self.begin(Kind::Other);
                self.delete_selection();
            }
        } else {
            // No selection: copy / cut the whole line.
            self.clip = format!("{}\n", self.lines[self.cy].iter().collect::<String>());
            if cut {
                self.begin(Kind::Other);
                if self.lines.len() > 1 {
                    self.lines.remove(self.cy);
                    self.cy = self.cy.min(self.lines.len() - 1);
                } else {
                    self.lines[0] = Rc::new(Vec::new());
                }
                self.cx = 0;
                self.want = 0;
            }
        }
        self.ensure_visible();
    }

    fn paste(&mut self) {
        if self.clip.is_empty() {
            self.say("CLIPBOARD EMPTY", 60);
            return;
        }
        self.begin(Kind::Other);
        self.delete_selection();
        let clip = self.clip.clone();
        self.insert_text(&clip);
        self.ensure_visible();
    }

    fn duplicate_line(&mut self) {
        self.begin(Kind::Other);
        let l = self.lines[self.cy].clone();
        self.lines.insert(self.cy + 1, l);
        self.cy += 1;
        self.ensure_visible();
    }

    pub fn save(&mut self) -> bool {
        let mut text = String::new();
        for l in &self.lines {
            text.extend(l.iter());
            text.push('\n');
        }
        match fs::write(&self.path, text) {
            Ok(()) => {
                self.dirty = false;
                self.say("SAVED", 60);
                true
            }
            Err(e) => {
                eprintln!("save failed: {e}");
                self.say("SAVE FAILED", 150);
                false
            }
        }
    }

    // ---- frame -------------------------------------------------------------

    pub fn tick(&mut self, inp: &FrameInput, g: &mut Gfx) -> Action {
        self.blink = self.blink.wrapping_add(1);
        self.msg_t = self.msg_t.saturating_sub(1);
        let mut action = Action::None;

        if inp.esc {
            if self.dirty && !self.armed {
                self.armed = true;
                self.say("UNSAVED: ESC AGAIN TO DISCARD", 150);
            } else {
                return Action::Exit;
            }
        } else if !inp.keys.is_empty() || !inp.chars.is_empty() || !inp.shortcuts.is_empty() {
            self.armed = false;
        }

        for c in &inp.shortcuts {
            match c {
                's' => {
                    self.save();
                }
                'r' => {
                    if self.save() {
                        self.err_line = None;
                        action = Action::Run;
                    }
                }
                'z' if inp.shift => self.do_redo(),
                'z' => self.do_undo(),
                'y' => self.do_redo(),
                'a' => {
                    self.anchor = Some((0, 0));
                    let last = self.lines.len() - 1;
                    self.cy = last;
                    self.cx = self.lines[last].len();
                    self.want = self.cx;
                    self.ensure_visible();
                }
                'c' => self.copy(false),
                'x' => self.copy(true),
                'v' => self.paste(),
                'd' => self.duplicate_line(),
                _ => {}
            }
        }
        for k in &inp.keys {
            self.key(*k, inp.shift, inp.ctrl);
        }
        for c in &inp.chars {
            self.insert_char(*c);
        }

        self.draw(g);
        action
    }

    fn ensure_states(&mut self) {
        if self.states_ok && self.states.len() == self.lines.len() + 1 {
            return;
        }
        self.states.clear();
        self.states.push(0);
        let mut st = 0u16;
        for l in &self.lines {
            st = lex_line(l, st, None);
            self.states.push(st);
        }
        self.states_ok = true;
    }

    fn draw(&mut self, g: &mut Gfx) {
        self.ensure_states();
        let gutter = self.gutter();
        let cols = self.cols();
        let sel = self.sel();
        g.cls(1);
        g.rectfill(gutter - 2, 0, gutter - 2, 119, 5);

        let mut colors: Vec<u8> = Vec::new();
        for i in 0..VIS_ROWS {
            let y = self.top + i;
            if y >= self.lines.len() {
                break;
            }
            let yy = 1 + i as i32 * ROW_H;
            let line = &self.lines[y];
            if y == self.cy {
                g.rectfill(gutter - 1, yy - 1, 127, yy + 5, 0);
            }
            // line number
            let num = (y + 1).to_string();
            let ncol = if self.err_line == Some(y + 1) { 8 } else if y == self.cy { 7 } else { 5 };
            g.print(&num, gutter - 3 - num.len() as i32 * 4 + 1, yy, ncol);
            if self.err_line == Some(y + 1) {
                g.rectfill(0, yy - 1, 0, yy + 5, 8);
            }
            // selection background
            if let Some((a, b)) = sel {
                if y >= a.1 && y <= b.1 {
                    let x0 = if y == a.1 { a.0 } else { 0 };
                    let x1 = if y == b.1 { b.0 } else { line.len() + 1 };
                    for x in x0..x1 {
                        if x >= self.left && x < self.left + cols {
                            let px = gutter + (x - self.left) as i32 * 4;
                            g.rectfill(px - 1, yy - 1, px + 2, yy + 5, 13);
                        }
                    }
                }
            }
            colors.clear();
            lex_line(line, self.states[y], Some(&mut colors));
            for x in self.left..line.len().min(self.left + cols) {
                let ch = line[x];
                if ch == ' ' {
                    continue;
                }
                let in_sel = sel.map_or(false, |(a, b)| {
                    (y, x) >= (a.1, a.0) && (y, x) < (b.1, b.0)
                });
                let col = if in_sel { 0 } else { colors.get(x).copied().unwrap_or(C_TEXT) };
                g.put_char(ch, gutter + (x - self.left) as i32 * 4, yy, col);
            }
        }

        // cursor
        if self.cy >= self.top && self.cy < self.top + VIS_ROWS && self.blink % 60 < 40 {
            let px = gutter + (self.cx.saturating_sub(self.left)) as i32 * 4 - 1;
            let yy = 1 + (self.cy - self.top) as i32 * ROW_H;
            g.rectfill(px, yy - 1, px, yy + 5, 10);
        }

        // status bar
        g.rectfill(0, 120, 127, 127, 0);
        if self.msg_t > 0 {
            g.print(&self.msg, 2, 121, 10);
        } else {
            let pos = format!("{}:{}{}", self.cy + 1, self.cx + 1, if self.dirty { "*" } else { "" });
            g.print(&pos, 2, 121, 6);
            let hint = "^S SAVE ^R RUN";
            g.print(hint, 127 - hint.len() as i32 * 4, 121, 13);
        }
    }
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Does this (trimmed) line open an indented block, so Enter should indent more?
fn opens_block(t: &str) -> bool {
    let last = t.rsplit(|c: char| !is_word(c)).next().unwrap_or("");
    matches!(last, "then" | "do" | "else" | "repeat")
        || t.ends_with('{')
        || (t.contains("function") && t.ends_with(')'))
}

// ---- Lua lexer (highlighting only) -----------------------------------------
//
// Line start state: 0 = normal, 1+n = inside a long comment of level n,
// 101+n = inside a long string of level n.

fn long_open(s: &[char], i: usize) -> Option<(u16, usize)> {
    if s.get(i) != Some(&'[') {
        return None;
    }
    let mut j = i + 1;
    while s.get(j) == Some(&'=') {
        j += 1;
    }
    if s.get(j) == Some(&'[') {
        Some(((j - i - 1) as u16, j - i + 1))
    } else {
        None
    }
}

fn find_close(s: &[char], from: usize, level: u16) -> Option<usize> {
    let mut k = from;
    while k < s.len() {
        if s[k] == ']' {
            let mut j = k + 1;
            let mut n = 0;
            while s.get(j) == Some(&'=') {
                n += 1;
                j += 1;
            }
            if n == level && s.get(j) == Some(&']') {
                return Some(j + 1);
            }
        }
        k += 1;
    }
    None
}

/// Lex one line. Returns the state at the end of the line. If `out` is given it
/// receives one colour per character.
pub fn lex_line(s: &[char], start: u16, mut out: Option<&mut Vec<u8>>) -> u16 {
    let n = s.len();
    if let Some(o) = out.as_deref_mut() {
        o.clear();
        o.resize(n, C_TEXT);
    }
    let mut paint = |from: usize, to: usize, col: u8| {
        if let Some(o) = out.as_deref_mut() {
            for c in o.iter_mut().take(to.min(n)).skip(from) {
                *c = col;
            }
        }
    };
    let mut i = 0;
    if start != 0 {
        let (level, col) = if start < 101 { (start - 1, C_COMMENT) } else { (start - 101, C_STRING) };
        match find_close(s, 0, level) {
            Some(end) => {
                paint(0, end, col);
                i = end;
            }
            None => {
                paint(0, n, col);
                return start;
            }
        }
    }
    while i < n {
        let c = s[i];
        if c == '-' && s.get(i + 1) == Some(&'-') {
            if let Some((level, len)) = long_open(s, i + 2) {
                match find_close(s, i + 2 + len, level) {
                    Some(end) => {
                        paint(i, end, C_COMMENT);
                        i = end;
                    }
                    None => {
                        paint(i, n, C_COMMENT);
                        return 1 + level;
                    }
                }
            } else {
                paint(i, n, C_COMMENT);
                return 0;
            }
        } else if c == '"' || c == '\'' {
            let mut j = i + 1;
            while j < n {
                if s[j] == '\\' {
                    j += 2;
                    continue;
                }
                if s[j] == c {
                    j += 1;
                    break;
                }
                j += 1;
            }
            let j = j.min(n);
            paint(i, j, C_STRING);
            i = j;
        } else if c == '[' && long_open(s, i).is_some() {
            let (level, len) = long_open(s, i).unwrap();
            match find_close(s, i + len, level) {
                Some(end) => {
                    paint(i, end, C_STRING);
                    i = end;
                }
                None => {
                    paint(i, n, C_STRING);
                    return 101 + level;
                }
            }
        } else if c.is_ascii_digit() || (c == '.' && s.get(i + 1).map_or(false, |d| d.is_ascii_digit())) {
            let mut j = i + 1;
            while j < n && (s[j].is_ascii_alphanumeric() || s[j] == '.' || s[j] == '_') {
                j += 1;
            }
            paint(i, j, C_NUMBER);
            i = j;
        } else if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i + 1;
            while j < n && (s[j].is_ascii_alphanumeric() || s[j] == '_') {
                j += 1;
            }
            let word: String = s[i..j].iter().collect();
            let is_field = i > 0 && (s[i - 1] == '.' || s[i - 1] == ':');
            let col = if is_field {
                C_TEXT
            } else if KEYWORDS.contains(&word.as_str()) {
                C_KEYWORD
            } else if API_NAMES.contains(&word.as_str()) {
                C_API
            } else {
                C_TEXT
            };
            paint(i, j, col);
            i = j;
        } else if c.is_whitespace() {
            i += 1;
        } else {
            paint(i, i + 1, C_PUNCT);
            i += 1;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(s: &str, st: u16) -> (Vec<u8>, u16) {
        let chars: Vec<char> = s.chars().collect();
        let mut out = Vec::new();
        let end = lex_line(&chars, st, Some(&mut out));
        (out, end)
    }

    #[test]
    fn highlights_keywords_strings_comments() {
        let (c, end) = lex("if x then print(\"hi\") end -- note", 0);
        assert_eq!(c[0], C_KEYWORD); // if
        assert_eq!(c[3], C_TEXT); // x
        assert_eq!(c[5], C_KEYWORD); // then
        assert_eq!(c[10], C_API); // print
        assert_eq!(c[16], C_STRING); // "hi"
        assert_eq!(c[c.len() - 1], C_COMMENT);
        assert_eq!(end, 0);
    }

    #[test]
    fn long_comments_span_lines() {
        let (_, s1) = lex("--[[ start", 0);
        assert_eq!(s1, 1);
        let (c2, s2) = lex("still comment ]] x = 1", s1);
        assert_eq!(c2[0], C_COMMENT);
        assert_eq!(s2, 0);
        let (_, s3) = lex("a = [==[ str", 0);
        assert_eq!(s3, 103);
    }

    #[test]
    fn block_openers_indent() {
        assert!(opens_block("if a then"));
        assert!(opens_block("function _draw()"));
        assert!(opens_block("for i=1,3 do"));
        assert!(!opens_block("local todo"));
        assert!(!opens_block("x = 1"));
    }
}
