//! The built-in HUD, expressed as ordinary overlay elements so it uses the same renderer (and the same emoji) as anything
//! an agent adds. Rebuilt every frame from the show's variables and the frame size. It is deliberately small and light:
//! plain smooth text with an outline, thin bars, no boxes around individual lines, so the viewers keep the picture. Turn it
//! off with `"hud": false` in the show file and draw your own with the overlay commands.
//!
//! A war gets two health bars and the round clock; a studio gets its title, an optional goal bar and the top supporters.
//! Both share the gift menu, the ticker and the banner at the bottom.

use crate::overlay::{Element, Kind, Vars};
use crate::war::War;

pub type HudItem = (Element, f32, Option<f32>);

/// One line of the gift menu: icon, label, colour.
pub type MenuItem = (String, String, String);

/// A side colour mixed halfway to white, so small text stays readable on any scenery.
fn lighten(c: &str) -> String {
    let hex = c.trim_start_matches('#');
    if hex.len() < 6 || !hex.is_ascii() || c.contains('{') {
        return c.into();
    }
    let ch = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_or(255, |v| ((v as u16 + 255 * 2) / 3) as u8);
    format!("#{:02x}{:02x}{:02x}", ch(0), ch(2), ch(4))
}

/// Outlined smooth text, no background.
fn text(t: &str, size: f32, color: &str) -> Element {
    let mut e = Element::text(t, size);
    if let Kind::Text { outline, color: c, shadow, pad, .. } = &mut e.kind {
        *outline = Some("#000000cc".into());
        *c = color.into();
        *shadow = false;
        *pad = 0.0;
    }
    e
}

/// Frame geometry shared by the builders: text sizes are fractions of the shorter side, a phone screen needs them a bit bigger.
struct Frame {
    w: usize,
    h: usize,
    short: f32,
    portrait: bool,
    k: f32,
}

impl Frame {
    fn new(w: usize, h: usize) -> Frame {
        let portrait = h > w;
        Frame { w, h, short: w.min(h) as f32, portrait, k: if portrait { 1.45 } else { 1.0 } }
    }

    /// height of one text line of font size `size`, as a fraction of the frame height
    fn line(&self, size: f32) -> f32 {
        (size * self.short * 1.3) / self.h as f32
    }

    fn top(&self) -> f32 {
        if self.portrait { 0.075 } else { 0.015 }
    }

    fn bottom(&self) -> f32 {
        if self.portrait { 0.17 } else { 0.015 }
    }
}

/// `(element, age, seconds left)` for the war HUD of a frame `w` x `h` pixels.
pub fn build(war: &War, w: usize, h: usize) -> Vec<HudItem> {
    let mut out: Vec<HudItem> = vec![];
    let f = Frame::new(w, h);
    let (short, portrait, k) = (f.short, f.portrait, f.k);
    let vars = war.vars();
    let keys = war.side_keys();
    let (ka, kb) = (&keys[0], &keys[1]);
    let color = |key: &str| vars.get(&format!("color.{key}")).cloned().unwrap_or_else(|| "#ffffff".into());
    let top = f.top();

    // title and clock, one under the other, centred
    let ts = 0.034 * k;
    out.push((text("{title}", ts, "#ffffff").at(0.5, top, "top").id("hud.title"), 1e3, None));
    let mut y = top + f.line(ts);
    let cs = 0.022 * k;
    out.push((text("{lbl.round} {round}/{rounds}   |   {time}   |   {biome}", cs, "#f2f2f2").at(0.5, y, "top").id("hud.clock"), 1e3, None));
    y += f.line(cs);
    let hs = 0.018 * k;
    if vars.get("hint").is_some_and(|v| !v.is_empty()) {
        out.push((text("{hint}", hs, "#ffe27a").at(0.5, y, "top").id("hud.hint"), 1e3, None));
    }
    y += f.line(hs) + 0.004;

    // thin health bars with one line of numbers under each
    let bar_w = 0.34 * w as f32 / short;
    let bar_h = 0.014 * k;
    let ls = if portrait { 0.017 } else { 0.02 } * k;
    let sep = if portrait { "\n" } else { "   " };
    for (i, key) in [ka, kb].into_iter().enumerate() {
        let right = i == 1;
        let (x, anchor) = if right { (0.975, "top-right") } else { (0.025, "top-left") };
        let mut bar = Element::new(Kind::Bar {
            w: bar_w,
            h: bar_h,
            value: 1.0,
            bind: Some(format!("hp_frac.{key}")),
            color: color(key),
            bg: "#00000099".into(),
            border: Some("#ffffffcc".into()),
            from_right: right,
        })
        .at(x, y, anchor)
        .id(&format!("hud.bar.{key}"));
        bar.z = 1;
        out.push((bar, 1e3, None));
        let mut label = text(&format!("{{name.{key}}}{sep}{{lbl.hp}} {{hp.{key}}}   {{lbl.units}} {{units.{key}}}{sep}{{lbl.wins}} {{wins.{key}}}   {{lbl.losses}} {{losses.{key}}}  {{shield.{key}}}"), ls, &lighten(&color(key))).at(x, y + (bar_h * short + 3.0) / h as f32, anchor).id(&format!("hud.label.{key}"));
        if let Kind::Text { align, .. } = &mut label.kind {
            *align = if right { "right".into() } else { "left".into() };
        }
        out.push((label, 1e3, None));
    }

    let menu: Vec<MenuItem> = war.legend().into_iter().map(|(icon, label, side)| (icon, label, side.map(|s| color(&keys[s])).unwrap_or_else(|| "#ffffff".into()))).collect();
    footer(&mut out, &f, &vars, &menu, war.banner_info());
    out
}

/// The HUD of a studio (a show with no game): title, an optional goal bar, the top supporters, and the shared footer.
/// `vars` carries `title`, `goal_frac` (present only when there is a goal), `goal_label`, `goal_value`, `goal_target`,
/// `goal_color`, `top1..3` and the ticker lines.
pub fn build_studio(vars: &Vars, menu: &[MenuItem], banner: Option<(String, String, f32, f32)>, w: usize, h: usize) -> Vec<HudItem> {
    let mut out: Vec<HudItem> = vec![];
    let f = Frame::new(w, h);
    let (short, k) = (f.short, f.k);
    let top = f.top();
    let ts = 0.034 * k;
    out.push((text("{title}", ts, "#ffffff").at(0.5, top, "top").id("hud.title"), 1e3, None));
    let mut y = top + f.line(ts) + 0.006;

    if vars.contains_key("goal_frac") {
        let bar_h = 0.016 * k;
        let color = vars.get("goal_color").cloned().unwrap_or_else(|| "#f2a31b".into());
        let mut bar = Element::new(Kind::Bar { w: 0.5 * w as f32 / short, h: bar_h, value: 0.0, bind: Some("goal_frac".into()), color: color.clone(), bg: "#00000099".into(), border: Some("#ffffffcc".into()), from_right: false })
            .at(0.5, y, "top")
            .id("hud.goal");
        bar.z = 1;
        out.push((bar, 1e3, None));
        let label = text("{goal_label}   {goal_value} / {goal_target}", 0.02 * k, &lighten(&color)).at(0.5, y + (bar_h * short + 3.0) / h as f32, "top").id("hud.goal.label");
        out.push((label, 1e3, None));
        y += f.line(0.02 * k) + bar_h * short / h as f32 + 0.01;
    }

    if vars.get("top1").is_some_and(|t| !t.is_empty()) {
        let mut tops = text("TOP\n{top1}\n{top2}\n{top3}", 0.019 * k, "#f2f2f2").at(0.975, y.max(top), "top-right").id("hud.top");
        if let Kind::Text { align, .. } = &mut tops.kind {
            *align = "right".into();
        }
        out.push((tops, 1e3, None));
    }

    footer(&mut out, &f, vars, menu, banner);
    out
}

/// The gift menu along the bottom, the ticker above it and the banner in the middle: shared by every HUD.
fn footer(out: &mut Vec<HudItem>, f: &Frame, vars: &Vars, menu: &[MenuItem], banner: Option<(String, String, f32, f32)>) {
    let (w, h, short, portrait, k) = (f.w, f.h, f.short, f.portrait, f.k);
    let bottom = f.bottom();

    // gift menu: one slim translucent strip along the bottom, small single-line entries
    let cols = if portrait { 2 } else { 4 }.min(menu.len().max(1));
    let ms = 0.018 * k;
    let cell_h = f.line(ms) * 1.05;
    let rows = menu.len().div_ceil(cols);
    let strip_h = rows as f32 * cell_h + 0.012;
    let menu_top = 1.0 - bottom - strip_h;
    if !menu.is_empty() {
        let mut strip = Element::new(Kind::Rect { w: w as f32 / short, h: strip_h * h as f32 / short, color: "#00000066".into(), radius: 0.0, border: None, border_w: 0.0, full: false }).at(0.5, 1.0 - bottom, "bottom").id("hud.menu.strip");
        strip.z = -1;
        out.push((strip, 1e3, None));
    }
    for (n, (icon, label, col)) in menu.iter().take(12).enumerate() {
        let (r, c) = (n / cols, n % cols);
        let cx = 0.012 + (c as f32 + 0.0) / cols as f32;
        let cy = menu_top + 0.006 + r as f32 * cell_h;
        let mut e = text(&format!("{icon} {label}"), ms, col).at(cx, cy, "top-left").id(&format!("hud.menu.{n}"));
        if let Kind::Text { max_width, .. } = &mut e.kind {
            *max_width = Some(1.0 / cols as f32 - 0.02);
        }
        out.push((e, 1e3, None));
    }

    // ticker above the menu: three short lines, older ones fainter
    let ts2 = 0.021 * k;
    let tick_h = f.line(ts2);
    let mut ty = menu_top - 0.008;
    for i in 1..=3 {
        if vars.get(&format!("ticker.{i}")).is_none_or(|t| t.trim().is_empty()) {
            continue;
        }
        let mut e = text(&format!("{{ticker.{i}}}"), ts2, &format!("{{tickercolor.{i}}}")).at(0.025, ty, "bottom-left").id(&format!("hud.ticker.{i}"));
        e.opacity = 1.0 - 0.28 * (i - 1) as f32;
        out.push((e, 1e3, None));
        ty -= tick_h;
    }

    // banner (round start, winners, announcements): modest, not a wall across the picture
    if let Some((txt, col, age, left)) = banner {
        let mut e = text(&txt, 0.05 * k, &col).at(0.5, 0.3, "center").id("hud.banner");
        if let Kind::Text { bg, max_width, pad, .. } = &mut e.kind {
            *bg = Some("#000000a0".into());
            *max_width = Some(0.8);
            *pad = 0.012;
        }
        e.anim_in = Some("pop".into());
        e.anim_out = Some("fade".into());
        e.z = 10;
        out.push((e, age, if left.is_finite() { Some(left) } else { None }));
    }
}
