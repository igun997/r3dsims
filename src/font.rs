//! Tiny 3x5 pixel font for name tags and speech bubbles (drawn at render resolution).

// 5 rows x 3 cols per glyph, '#' = pixel.
const GLYPHS: &[(char, [&str; 5])] = &[
    ('A', [".#.", "#.#", "###", "#.#", "#.#"]),
    ('B', ["##.", "#.#", "##.", "#.#", "##."]),
    ('C', [".##", "#..", "#..", "#..", ".##"]),
    ('D', ["##.", "#.#", "#.#", "#.#", "##."]),
    ('E', ["###", "#..", "##.", "#..", "###"]),
    ('F', ["###", "#..", "##.", "#..", "#.."]),
    ('G', [".##", "#..", "#.#", "#.#", ".##"]),
    ('H', ["#.#", "#.#", "###", "#.#", "#.#"]),
    ('I', ["###", ".#.", ".#.", ".#.", "###"]),
    ('J', ["..#", "..#", "..#", "#.#", ".#."]),
    ('K', ["#.#", "#.#", "##.", "#.#", "#.#"]),
    ('L', ["#..", "#..", "#..", "#..", "###"]),
    ('M', ["#.#", "###", "###", "#.#", "#.#"]),
    ('N', ["##.", "#.#", "#.#", "#.#", "#.#"]),
    ('O', [".#.", "#.#", "#.#", "#.#", ".#."]),
    ('P', ["##.", "#.#", "##.", "#..", "#.."]),
    ('Q', [".#.", "#.#", "#.#", "##.", ".##"]),
    ('R', ["##.", "#.#", "##.", "#.#", "#.#"]),
    ('S', [".##", "#..", ".#.", "..#", "##."]),
    ('T', ["###", ".#.", ".#.", ".#.", ".#."]),
    ('U', ["#.#", "#.#", "#.#", "#.#", "###"]),
    ('V', ["#.#", "#.#", "#.#", "#.#", ".#."]),
    ('W', ["#.#", "#.#", "###", "###", "#.#"]),
    ('X', ["#.#", "#.#", ".#.", "#.#", "#.#"]),
    ('Y', ["#.#", "#.#", ".#.", ".#.", ".#."]),
    ('Z', ["###", "..#", ".#.", "#..", "###"]),
    ('0', ["###", "#.#", "#.#", "#.#", "###"]),
    ('1', [".#.", "##.", ".#.", ".#.", "###"]),
    ('2', ["##.", "..#", ".#.", "#..", "###"]),
    ('3', ["##.", "..#", ".#.", "..#", "##."]),
    ('4', ["#.#", "#.#", "###", "..#", "..#"]),
    ('5', ["###", "#..", "##.", "..#", "##."]),
    ('6', [".##", "#..", "###", "#.#", "###"]),
    ('7', ["###", "..#", ".#.", ".#.", ".#."]),
    ('8', ["###", "#.#", "###", "#.#", "###"]),
    ('9', ["###", "#.#", "###", "..#", "##."]),
    ('.', ["...", "...", "...", "...", ".#."]),
    (',', ["...", "...", "...", ".#.", "#.."]),
    ('!', [".#.", ".#.", ".#.", "...", ".#."]),
    ('?', ["##.", "..#", ".#.", "...", ".#."]),
    (':', ["...", ".#.", "...", ".#.", "..."]),
    ('-', ["...", "...", "###", "...", "..."]),
    ('@', [".##.", "#.##", "#.#.", "#...", ".###"]),
    ('\'', [".#.", ".#.", "...", "...", "..."]),
    ('/', ["..#", "..#", ".#.", "#..", "#.."]),
    ('+', ["...", ".#.", "###", ".#.", "..."]),
    ('%', ["#.#", "..#", ".#.", "#..", "#.#"]),
    ('(', [".#", "#.", "#.", "#.", ".#"]),
    (')', ["#.", ".#", ".#", ".#", "#."]),
    ('=', ["...", "###", "...", "###", "..."]),
    ('>', ["#..", ".#.", "..#", ".#.", "#.."]),
    ('<', ["..#", ".#.", "#..", ".#.", "..#"]),
    ('*', ["...", "#.#", ".#.", "#.#", "..."]),
    ('#', ["#.#", "###", "#.#", "###", "#.#"]),
    ('_', ["...", "...", "...", "...", "###"]),
    ('"', ["#.#", "#.#", "...", "...", "..."]),
    ('&', [".#.", "#.#", ".##", "#.#", ".##"]),
    ('|', [".#.", ".#.", ".#.", ".#.", ".#."]),
    ('[', ["##", "#.", "#.", "#.", "##"]),
    (']', ["##", ".#", ".#", ".#", "##"]),
];

fn glyph(c: char) -> Option<&'static [&'static str; 5]> {
    let c = c.to_ascii_uppercase();
    GLYPHS.iter().find(|(g, _)| *g == c).map(|(_, rows)| rows)
}

fn advance(rows: &[&str; 5]) -> i32 {
    rows[0].chars().count() as i32 + 1
}

/// Glyph rows of `c` (case-insensitive), `None` for characters the font lacks (spaces included).
pub fn glyph_rows(c: char) -> Option<&'static [&'static str; 5]> {
    glyph(c)
}

/// Horizontal advance of `c` in font pixels (unknown characters and spaces advance 4).
pub fn advance_px(c: char) -> i32 {
    glyph(c).map_or(4, advance)
}

pub fn text_width(s: &str) -> i32 {
    (s.chars().map(|c| glyph(c).map_or(4, advance)).sum::<i32>() - 1).max(0)
}

pub const LINE_H: i32 = 6;

/// Writes one opaque pixel into an RGBA8 buffer (the overlay texture).
pub fn put(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, c: [u8; 3]) {
    if x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h {
        let i = (y as usize * w + x as usize) * 4;
        buf[i..i + 3].copy_from_slice(&c);
        buf[i + 3] = 255;
    }
}

pub fn rect(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, rw: i32, rh: i32, c: [u8; 3]) {
    for yy in y..y + rh {
        for xx in x..x + rw {
            put(buf, w, h, xx, yy, c);
        }
    }
}

pub fn draw_text(buf: &mut [u8], w: usize, h: usize, x: i32, y: i32, text: &str, c: [u8; 3]) {
    let mut cx = x;
    for ch in text.chars() {
        match glyph(ch) {
            Some(rows) => {
                for (ry, row) in rows.iter().enumerate() {
                    for (rx, px) in row.chars().enumerate() {
                        if px == '#' {
                            put(buf, w, h, cx + rx as i32, y + ry as i32, c);
                        }
                    }
                }
                cx += advance(rows);
            }
            None => cx += 4,
        }
    }
}

/// Word-wraps to at most `max` chars per line.
pub fn wrap(text: &str, max: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_rows_are_rectangular() {
        for (c, rows) in GLYPHS {
            let w = rows[0].chars().count();
            assert!(rows.iter().all(|r| r.chars().count() == w), "glyph {c}");
        }
    }

    #[test]
    fn wraps() {
        assert_eq!(wrap("halo budi apa kabar", 9), vec!["halo budi", "apa kabar"]);
    }
}
