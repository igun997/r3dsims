//! The built-in war HUD, expressed as ordinary overlay elements so it uses the same renderer (and the
//! same emoji) as anything an agent adds. Rebuilt every frame from the war state and the frame size.
//! Turn it off with `"hud": false` in war.json and draw your own with the overlay commands.

use crate::overlay::{Element, Kind};
use crate::war::War;

pub type HudItem = (Element, f32, Option<f32>);

fn text(t: &str, size: f32) -> Element {
    Element::text(t, size)
}

/// `(element, age, seconds left)` for the frame of `w` x `h` pixels.
pub fn build(war: &War, w: usize, h: usize) -> Vec<HudItem> {
    let mut out: Vec<HudItem> = vec![];
    let short = w.min(h) as f32;
    let portrait = h > w;
    let vars = war.vars();
    let keys = war.side_keys();
    let (ka, kb) = (&keys[0], &keys[1]);
    let color = |k: &str| vars.get(&format!("color.{k}")).cloned().unwrap_or_else(|| "#ffffff".into());
    // pixel height of one text line of `size`, as a fraction of the frame height
    let line = |size: f32| (size * short * 8.0 / 5.0) / h as f32;
    let top = if portrait { 0.075 } else { 0.02 };
    let bottom = if portrait { 0.17 } else { 0.03 };

    // title
    let ts = if portrait { 0.05 } else { 0.05 };
    let mut title = text("{title}", ts).at(0.5, top, "top").id("hud.title");
    if let Kind::Text { outline, bg, .. } = &mut title.kind {
        *outline = Some("#000000".into());
        *bg = None;
    }
    out.push((title, 1e3, None));
    let mut y = top + line(ts) + 0.006;

    // round + clock
    let mut clock = text("{lbl.round} {round}/{rounds}   {time}   {biome}", 0.03).at(0.5, y, "top").id("hud.clock");
    if let Kind::Text { bg, .. } = &mut clock.kind {
        *bg = Some("#000000aa".into());
    }
    out.push((clock, 1e3, None));
    y += line(0.03) + 0.014;
    if vars.get("hint").is_some_and(|h| !h.is_empty()) {
        let mut hint = text("{hint}", 0.022).at(0.5, y - 0.008, "top").id("hud.hint");
        if let Kind::Text { bg, color: c, .. } = &mut hint.kind {
            *bg = Some("#000000aa".into());
            *c = "#ffe27a".into();
        }
        out.push((hint, 1e3, None));
    }
    y += line(0.022) + 0.012;

    // health bars
    let bar_w = 0.44 * w as f32 / short;
    let bar_h = 0.032;
    for (i, k) in [ka, kb].into_iter().enumerate() {
        let right = i == 1;
        let x = if right { 0.97 } else { 0.03 };
        let anchor = if right { "top-right" } else { "top-left" };
        let mut bar = Element::new(Kind::Bar {
            w: bar_w,
            h: bar_h,
            value: 1.0,
            bind: Some(format!("hp_frac.{k}")),
            color: color(k),
            bg: "#000000b0".into(),
            border: Some("#ffffff".into()),
            from_right: right,
        })
        .at(x, y, anchor)
        .id(&format!("hud.bar.{k}"));
        bar.z = 1;
        out.push((bar, 1e3, None));
        let mut label = text(&format!("{{name.{k}}}\n{{lbl.hp}} {{hp.{k}}}  {{lbl.units}} {{units.{k}}}  {{lbl.wins}} {{wins.{k}}}  {{shield.{k}}}"), 0.024).at(x, y + (bar_h * short + 4.0) / h as f32, anchor).id(&format!("hud.label.{k}"));
        if let Kind::Text { align, color: c, outline, .. } = &mut label.kind {
            *align = if right { "right".into() } else { "left".into() };
            *c = color(k);
            *outline = Some("#000000".into());
        }
        out.push((label, 1e3, None));
    }

    // gift menu along the bottom
    let legend = war.legend();
    let cols = if portrait { 2 } else { 4 }.min(legend.len().max(1));
    let size = 0.024;
    // two text lines per cell, so a long label wraps instead of running into its neighbour
    let cell_h = line(size) * 2.0 + 0.02;
    let rows = legend.len().div_ceil(cols);
    let menu_top = 1.0 - bottom - rows as f32 * cell_h;
    for (n, (icon, label, side)) in legend.iter().take(12).enumerate() {
        let (r, c) = (n / cols, n % cols);
        let cx = (c as f32 + 0.5) / cols as f32;
        let cy = menu_top + (r as f32 + 0.5) * cell_h;
        let mut e = text(&format!("{icon} {label}"), size).at(cx, cy, "center").id(&format!("hud.menu.{n}"));
        if let Kind::Text { bg, color: col, max_width, .. } = &mut e.kind {
            *bg = Some("#000000b8".into());
            *max_width = Some(1.0 / cols as f32 - 0.014);
            if let Some(s) = side {
                *col = color(&keys[*s]);
            }
        }
        out.push((e, 1e3, None));
    }

    // ticker above the menu
    let tick_size = if portrait { 0.026 } else { 0.028 };
    let tick_h = line(tick_size) + 0.006;
    let mut ty = menu_top - 0.012;
    for i in 1..=4 {
        if vars.get(&format!("ticker.{i}")).is_none_or(|t| t.trim().is_empty()) {
            continue;
        }
        let mut e = text(&format!("{{ticker.{i}}}"), tick_size).at(0.03, ty, "bottom-left").id(&format!("hud.ticker.{i}"));
        if let Kind::Text { bg, color: col, .. } = &mut e.kind {
            *bg = Some("#000000aa".into());
            *col = format!("{{tickercolor.{i}}}");
        }
        e.opacity = 1.0 - 0.18 * (i - 1) as f32;
        out.push((e, 1e3, None));
        ty -= tick_h + 0.006;
    }

    // banner
    if let Some((txt, col, age, left)) = war.banner_info() {
        let mut e = text(&txt, if portrait { 0.075 } else { 0.08 }).at(0.5, 0.36, "center").id("hud.banner");
        if let Kind::Text { bg, color: c, max_width, outline, .. } = &mut e.kind {
            *bg = Some("#000000bb".into());
            *c = col;
            *max_width = Some(0.9);
            *outline = Some("#000000".into());
        }
        e.anim_in = Some("pop".into());
        e.anim_out = Some("fade".into());
        e.z = 10;
        out.push((e, age, if left.is_finite() { Some(left) } else { None }));
    }
    out
}
