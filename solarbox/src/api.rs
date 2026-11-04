//! The Lua API exposed to SolarBox carts.

use crate::gfx::Gfx;
use mlua::{Lua, Result};
use std::{cell::RefCell, rc::Rc, time::Instant};

pub struct Console {
    pub gfx: Gfx,
    pub down: [bool; 6], // left, right, up, down, O (Z), X (X)
    pub prev: [bool; 6],
    pub start: Instant,
}

impl Console {
    pub fn new() -> Self {
        Console { gfx: Gfx::new(), down: [false; 6], prev: [false; 6], start: Instant::now() }
    }

    /// Fresh screen and clock for a newly launched cart (input state is kept).
    pub fn reset(&mut self) {
        self.gfx = Gfx::new();
        self.start = Instant::now();
    }
}

/// Lua-side helpers: PICO-8-style maths (turns, inverted sin) and table utilities.
const PRELUDE: &str = r#"
local pi2 = math.pi * 2
function flr(x) return math.floor(x) end
function ceil(x) return math.ceil(x) end
function sin(x) return -math.sin(x * pi2) end
function cos(x) return math.cos(x * pi2) end
function atan2(dx, dy) return (math.atan(-dy, dx) / pi2) % 1 end
function sqrt(x) return math.sqrt(x) end
function abs(x) return math.abs(x) end
function min(a, b) if a < b then return a end return b end
function max(a, b) if a > b then return a end return b end
function sgn(x) if x < 0 then return -1 end return 1 end
function mid(a, b, c)
  if a > b then a, b = b, a end
  if b > c then b = c end
  if a > b then return a end
  return b
end
function rnd(x) return math.random() * (x or 1) end
function srand(s) math.randomseed(flr(s)) end
function add(t, v) t[#t + 1] = v return v end
function del(t, v)
  for i = 1, #t do if t[i] == v then table.remove(t, i) return v end end
end
function count(t) return #t end
function foreach(t, f) for i = 1, #t do f(t[i]) end end
function all(t)
  local i = 0
  return function() i = i + 1 return t[i] end
end
"#;

fn n(v: f64) -> i32 {
    v.floor() as i32
}

pub fn install(lua: &Lua, con: &Rc<RefCell<Console>>) -> Result<()> {
    let g = lua.globals();
    // Carts must not read the host's files.
    g.set("dofile", mlua::Value::Nil)?;
    g.set("loadfile", mlua::Value::Nil)?;

    macro_rules! reg {
        ($name:expr, $f:expr) => {
            g.set($name, lua.create_function($f)?)?;
        };
    }

    let c = con.clone();
    reg!("cls", move |_, col: Option<f64>| {
        let mut s = c.borrow_mut();
        s.gfx.cls(col.map_or(0, |v| n(v) as u8));
        Ok(())
    });

    let c = con.clone();
    reg!("color", move |_, col: f64| {
        c.borrow_mut().gfx.pen = n(col) as u8 & 15;
        Ok(())
    });

    let c = con.clone();
    reg!("pset", move |_, (x, y, col): (f64, f64, Option<f64>)| {
        let mut s = c.borrow_mut();
        let col = col.map_or(s.gfx.pen, |v| n(v) as u8);
        s.gfx.pset(n(x), n(y), col);
        Ok(())
    });

    let c = con.clone();
    reg!("pget", move |_, (x, y): (f64, f64)| Ok(c.borrow().gfx.pget(n(x), n(y)) as i32));

    let c = con.clone();
    reg!("line", move |_, (x0, y0, x1, y1, col): (f64, f64, f64, f64, Option<f64>)| {
        let mut s = c.borrow_mut();
        let col = col.map_or(s.gfx.pen, |v| n(v) as u8);
        s.gfx.line(n(x0), n(y0), n(x1), n(y1), col);
        Ok(())
    });

    let c = con.clone();
    reg!("rect", move |_, (x0, y0, x1, y1, col): (f64, f64, f64, f64, Option<f64>)| {
        let mut s = c.borrow_mut();
        let col = col.map_or(s.gfx.pen, |v| n(v) as u8);
        s.gfx.rect(n(x0), n(y0), n(x1), n(y1), col);
        Ok(())
    });

    let c = con.clone();
    reg!("rectfill", move |_, (x0, y0, x1, y1, col): (f64, f64, f64, f64, Option<f64>)| {
        let mut s = c.borrow_mut();
        let col = col.map_or(s.gfx.pen, |v| n(v) as u8);
        s.gfx.rectfill(n(x0), n(y0), n(x1), n(y1), col);
        Ok(())
    });

    let c = con.clone();
    reg!("circ", move |_, (x, y, r, col): (f64, f64, f64, Option<f64>)| {
        let mut s = c.borrow_mut();
        let col = col.map_or(s.gfx.pen, |v| n(v) as u8);
        s.gfx.circ(n(x), n(y), n(r), col);
        Ok(())
    });

    let c = con.clone();
    reg!("circfill", move |_, (x, y, r, col): (f64, f64, f64, Option<f64>)| {
        let mut s = c.borrow_mut();
        let col = col.map_or(s.gfx.pen, |v| n(v) as u8);
        s.gfx.circfill(n(x), n(y), n(r), col);
        Ok(())
    });

    let c = con.clone();
    reg!("print", move |_, (text, x, y, col): (mlua::Value, Option<f64>, Option<f64>, Option<f64>)| {
        let text = match text {
            mlua::Value::String(s) => s.to_string_lossy().to_string(),
            mlua::Value::Nil => "nil".to_string(),
            other => format!("{}", other.to_string()?),
        };
        let mut s = c.borrow_mut();
        let col = col.map_or(s.gfx.pen, |v| n(v) as u8);
        Ok(s.gfx.print(&text, x.map_or(0, n), y.map_or(0, n), col))
    });

    let c = con.clone();
    reg!("btn", move |_, i: f64| {
        let i = n(i);
        Ok(i >= 0 && (i as usize) < 6 && c.borrow().down[i as usize])
    });

    let c = con.clone();
    reg!("btnp", move |_, i: f64| {
        let i = n(i);
        let s = c.borrow();
        Ok(i >= 0 && (i as usize) < 6 && s.down[i as usize] && !s.prev[i as usize])
    });

    let c = con.clone();
    reg!("time", move |_, ()| Ok(c.borrow().start.elapsed().as_secs_f64()));
    let c = con.clone();
    reg!("t", move |_, ()| Ok(c.borrow().start.elapsed().as_secs_f64()));

    lua.load(PRELUDE).set_name("=prelude").exec()?;
    Ok(())
}
