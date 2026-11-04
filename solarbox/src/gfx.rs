//! SolarBox graphics: a 128x128 framebuffer of 4-bit palette indices.

pub const W: usize = 128;
pub const H: usize = 128;

/// Palette as 0xRRGGBB. Swap these 16 values to re-skin the console.
pub const PALETTE: [u32; 16] = [
    0x000000, 0x1D2B53, 0x7E2553, 0x008751, 0xAB5236, 0x5F574F, 0xC2C3C7, 0xFFF1E8,
    0xFF004D, 0xFFA300, 0xFFEC27, 0x00E436, 0x29ADFF, 0x83769C, 0xFF77A8, 0xFFCCAA,
];

#[derive(Clone)]
pub struct Gfx {
    pub px: [u8; W * H],
    pub pen: u8,
}

impl Gfx {
    pub fn new() -> Self {
        Gfx { px: [0; W * H], pen: 7 }
    }

    pub fn cls(&mut self, c: u8) {
        self.px.fill(c & 15);
    }

    pub fn pset(&mut self, x: i32, y: i32, c: u8) {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            self.px[y as usize * W + x as usize] = c & 15;
        }
    }

    pub fn pget(&self, x: i32, y: i32) -> u8 {
        if x >= 0 && y >= 0 && (x as usize) < W && (y as usize) < H {
            self.px[y as usize * W + x as usize]
        } else {
            0
        }
    }

    pub fn hline(&mut self, x0: i32, x1: i32, y: i32, c: u8) {
        let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        for x in a..=b {
            self.pset(x, y, c);
        }
    }

    pub fn line(&mut self, mut x0: i32, mut y0: i32, x1: i32, y1: i32, c: u8) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.pset(x0, y0, c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    pub fn rect(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        self.line(x0, y0, x1, y0, c);
        self.line(x0, y1, x1, y1, c);
        self.line(x0, y0, x0, y1, c);
        self.line(x1, y0, x1, y1, c);
    }

    pub fn rectfill(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, c: u8) {
        let (ya, yb) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        for y in ya..=yb {
            self.hline(x0, x1, y, c);
        }
    }

    pub fn circ(&mut self, cx: i32, cy: i32, r: i32, c: u8) {
        let (mut x, mut y, mut err) = (r, 0, 1 - r);
        while x >= y {
            for (px, py) in [(x, y), (y, x), (-y, x), (-x, y), (-x, -y), (-y, -x), (y, -x), (x, -y)] {
                self.pset(cx + px, cy + py, c);
            }
            y += 1;
            if err < 0 {
                err += 2 * y + 1;
            } else {
                x -= 1;
                err += 2 * (y - x) + 1;
            }
        }
    }

    pub fn circfill(&mut self, cx: i32, cy: i32, r: i32, c: u8) {
        for dy in -r..=r {
            let dx = (((r * r - dy * dy) as f64).sqrt()).floor() as i32;
            self.hline(cx - dx, cx + dx, cy + dy, c);
        }
    }

    /// Draws text with the built-in 3x5 font; returns the x after the last glyph.
    pub fn print(&mut self, text: &str, x: i32, y: i32, c: u8) -> i32 {
        let mut cx = x;
        for ch in text.chars() {
            if let Some(rows) = glyph(ch.to_ascii_uppercase()) {
                for (ry, bits) in rows.iter().enumerate() {
                    for rx in 0..3 {
                        if bits & (4 >> rx) != 0 {
                            self.pset(cx + rx, y + ry as i32, c);
                        }
                    }
                }
            }
            cx += 4;
        }
        cx
    }

    /// Draws one character without upper-casing, with real lowercase letters
    /// (used by the code editor; carts' `print` stays upper-case like PICO-8).
    pub fn put_char(&mut self, ch: char, x: i32, y: i32, c: u8) {
        let rows = glyph_raw(ch);
        for (ry, bits) in rows.iter().enumerate() {
            for rx in 0..3 {
                if bits & (4 >> rx) != 0 {
                    self.pset(x + rx, y + ry as i32, c);
                }
            }
        }
    }

    pub fn to_rgb(&self, out: &mut [u32]) {
        for (o, p) in out.iter_mut().zip(self.px.iter()) {
            // Alpha must be 0xFF: Wayland/compositing X11 treat alpha 0 as transparent.
            *o = 0xFF00_0000 | PALETTE[(*p & 15) as usize];
        }
    }
}

/// Case-preserving 3x6 glyphs for the editor: lowercase has its own shapes,
/// unknown characters show as a small box.
fn glyph_raw(c: char) -> [u8; 6] {
    if let Some(l) = glyph_lower(c) {
        return l;
    }
    match glyph(c) {
        Some(r) => [r[0], r[1], r[2], r[3], r[4], 0],
        None if c == ' ' => [0; 6],
        None => [7, 5, 5, 5, 7, 0],
    }
}

fn glyph_lower(c: char) -> Option<[u8; 6]> {
    Some(match c {
        'a' => [0, 0, 3, 5, 7, 0], 'b' => [4, 4, 6, 5, 6, 0], 'c' => [0, 0, 3, 4, 3, 0],
        'd' => [1, 1, 3, 5, 3, 0], 'e' => [0, 0, 3, 7, 3, 0], 'f' => [3, 2, 7, 2, 2, 0],
        'g' => [0, 0, 3, 5, 3, 6], 'h' => [4, 4, 6, 5, 5, 0], 'i' => [2, 0, 2, 2, 2, 0],
        'j' => [1, 0, 1, 1, 1, 6], 'k' => [4, 4, 5, 6, 5, 0], 'l' => [2, 2, 2, 2, 3, 0],
        'm' => [0, 0, 7, 7, 5, 0], 'n' => [0, 0, 6, 5, 5, 0], 'o' => [0, 0, 2, 5, 2, 0],
        'p' => [0, 0, 6, 5, 6, 4], 'q' => [0, 0, 3, 5, 3, 1], 'r' => [0, 0, 3, 4, 4, 0],
        's' => [0, 0, 3, 2, 6, 0], 't' => [0, 2, 7, 2, 3, 0], 'u' => [0, 0, 5, 5, 7, 0],
        'v' => [0, 0, 5, 5, 2, 0], 'w' => [0, 0, 5, 7, 7, 0], 'x' => [0, 0, 5, 2, 5, 0],
        'y' => [0, 0, 5, 5, 3, 6], 'z' => [0, 0, 6, 2, 3, 0],
        _ => return None,
    })
}

/// 3x5 glyphs, one byte per row (low 3 bits, left pixel = bit 2).
fn glyph(c: char) -> Option<[u8; 5]> {
    Some(match c {
        ' ' => [0, 0, 0, 0, 0],
        'A' => [2, 5, 7, 5, 5], 'B' => [6, 5, 6, 5, 6], 'C' => [3, 4, 4, 4, 3],
        'D' => [6, 5, 5, 5, 6], 'E' => [7, 4, 6, 4, 7], 'F' => [7, 4, 6, 4, 4],
        'G' => [3, 4, 5, 5, 3], 'H' => [5, 5, 7, 5, 5], 'I' => [7, 2, 2, 2, 7],
        'J' => [1, 1, 1, 5, 2], 'K' => [5, 5, 6, 5, 5], 'L' => [4, 4, 4, 4, 7],
        'M' => [5, 7, 7, 5, 5], 'N' => [6, 5, 5, 5, 5], 'O' => [2, 5, 5, 5, 2],
        'P' => [6, 5, 6, 4, 4], 'Q' => [2, 5, 5, 6, 3], 'R' => [6, 5, 6, 5, 5],
        'S' => [3, 4, 2, 1, 6], 'T' => [7, 2, 2, 2, 2], 'U' => [5, 5, 5, 5, 7],
        'V' => [5, 5, 5, 5, 2], 'W' => [5, 5, 7, 7, 5], 'X' => [5, 5, 2, 5, 5],
        'Y' => [5, 5, 2, 2, 2], 'Z' => [7, 1, 2, 4, 7],
        '0' => [7, 5, 5, 5, 7], '1' => [2, 6, 2, 2, 7], '2' => [6, 1, 2, 4, 7],
        '3' => [6, 1, 2, 1, 6], '4' => [5, 5, 7, 1, 1], '5' => [7, 4, 6, 1, 6],
        '6' => [3, 4, 7, 5, 7], '7' => [7, 1, 2, 2, 2], '8' => [7, 5, 7, 5, 7],
        '9' => [7, 5, 7, 1, 6],
        '.' => [0, 0, 0, 0, 2], ',' => [0, 0, 0, 2, 4], ':' => [0, 2, 0, 2, 0],
        '!' => [2, 2, 2, 0, 2], '?' => [6, 1, 2, 0, 2], '-' => [0, 0, 7, 0, 0],
        '+' => [0, 2, 7, 2, 0], '=' => [0, 7, 0, 7, 0], '/' => [1, 1, 2, 4, 4],
        '(' => [1, 2, 2, 2, 1], ')' => [4, 2, 2, 2, 4], '_' => [0, 0, 0, 0, 7],
        '*' => [0, 5, 2, 5, 0], '%' => [5, 1, 2, 4, 5], '<' => [1, 2, 4, 2, 1],
        '>' => [4, 2, 1, 2, 4], '\'' => [2, 2, 0, 0, 0], '"' => [5, 5, 0, 0, 0],
        '[' => [3, 2, 2, 2, 3], ']' => [6, 2, 2, 2, 6], ';' => [0, 2, 0, 2, 4],
        '#' => [5, 7, 5, 7, 5], '|' => [2, 2, 2, 2, 2], '\\' => [4, 4, 2, 1, 1],
        '^' => [2, 5, 0, 0, 0], '`' => [4, 2, 0, 0, 0], '{' => [3, 2, 6, 2, 3],
        '}' => [6, 2, 3, 2, 6], '@' => [7, 5, 7, 4, 3], '&' => [2, 5, 2, 5, 3],
        '$' => [3, 6, 2, 3, 6], '~' => [0, 3, 6, 0, 0],
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_pixels_are_opaque() {
        let mut g = Gfx::new();
        g.cls(7);
        let mut out = vec![0u32; W * H];
        g.to_rgb(&mut out);
        assert!(out.iter().all(|p| p >> 24 == 0xFF));
        assert_eq!(out[0] & 0xFFFFFF, PALETTE[7]);
    }
}
