//! Aspetto Adwaita: palette chiara/scura, font e tema scuro del desktop e widget su misura
//! (controllo segmentato, slider, tasti della calcolatrice).

use std::process::Command;

use egui::{
    Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Rect, Response,
    Sense, Shadow, Stroke, TextStyle, Ui, Vec2, pos2, vec2,
};

#[derive(Clone, Copy)]
pub struct Palette {
    pub dark: bool,
    pub window: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub card_border: Color32,
    pub separator: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub text3: Color32,
    pub accent: Color32,
    pub red: Color32,
    pub green: Color32,
    pub control: Color32,
    pub control_selected: Color32,
    pub hover: Color32,
    pub grid: Color32,
    pub axis: Color32,
    pub key_digit: Color32,
    pub key_fn: Color32,
    pub key_top: Color32,
    pub key_op: Color32,
    pub key_text: Color32,
    pub key_top_text: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

// Colori di libadwaita (tema Adwaita di GNOME, usato anche da Cinnamon)
pub const LIGHT: Palette = Palette {
    dark: false,
    window: hex(0xFAFAFB),
    sidebar: hex(0xEBEBED),
    card: hex(0xFFFFFF),
    card_border: hex(0xDADADD),
    separator: hex(0xDADADD),
    text: hex(0x2E2E32),
    text2: hex(0x6F6F74),
    text3: hex(0xA2A2A7),
    accent: hex(0x3584E4),
    red: hex(0xE01B24),
    green: hex(0x1B8553),
    control: hex(0xE1E1E4),
    control_selected: hex(0xFFFFFF),
    hover: Color32::from_rgba_premultiplied(0, 0, 0, 14),
    grid: hex(0xE8E8EB),
    axis: hex(0x909095),
    key_digit: hex(0xFFFFFF),
    key_fn: hex(0xE4E4E7),
    key_top: hex(0xD8D8DC),
    key_op: hex(0x3584E4),
    key_text: hex(0x2E2E32),
    key_top_text: hex(0x2E2E32),
};

pub const DARK: Palette = Palette {
    dark: true,
    window: hex(0x222226),
    sidebar: hex(0x2E2E32),
    card: hex(0x38383C),
    card_border: hex(0x46464B),
    separator: hex(0x18181B),
    text: hex(0xFFFFFF),
    text2: hex(0xA9A9AE),
    text3: hex(0x737378),
    accent: hex(0x3584E4),
    red: hex(0xFF7B63),
    green: hex(0x8FF0A4),
    control: hex(0x3A3A3E),
    control_selected: hex(0x5E5E64),
    hover: Color32::from_rgba_premultiplied(255, 255, 255, 16),
    grid: hex(0x303034),
    axis: hex(0x8A8A8F),
    key_digit: hex(0x48484D),
    key_fn: hex(0x3A3A3E),
    key_top: hex(0x56565C),
    key_op: hex(0x3584E4),
    key_text: hex(0xFFFFFF),
    key_top_text: hex(0xFFFFFF),
};

/// Colori della palette GNOME per le curve.
pub fn series_color(i: usize, dark: bool) -> Color32 {
    const L: [u32; 8] = [0x3584E4, 0xE01B24, 0x26A269, 0xE66100, 0x9141AC, 0x2190A4, 0xD56199, 0xC88800];
    const D: [u32; 8] = [0x62A0EA, 0xF66151, 0x57E389, 0xFFA348, 0xC061CB, 0x4FC0D0, 0xF08DBB, 0xF8E45C];
    hex(if dark { D[i % 8] } else { L[i % 8] })
}

pub const SERIES_COUNT: usize = 8;

fn load_first(paths: &[&str]) -> Option<Vec<u8>> {
    paths.iter().find_map(|p| std::fs::read(p).ok())
}

/// Valore di una chiave di gsettings, senza apici (`None` se gsettings manca).
fn gsetting(schema: &str, key: &str) -> Option<String> {
    let out = Command::new("gsettings").args(["get", schema, key]).output().ok()?;
    let v = String::from_utf8(out.stdout).ok()?;
    out.status.success().then(|| v.trim().trim_matches('\'').to_string())
}

/// Le impostazioni dell'interfaccia stanno in schemi diversi su Cinnamon e GNOME.
const INTERFACE_SCHEMAS: [&str; 2] = ["org.cinnamon.desktop.interface", "org.gnome.desktop.interface"];

/// File del font che fontconfig sceglie per `pattern`.
fn fc_font(pattern: &str) -> Option<Vec<u8>> {
    let out = Command::new("fc-match").args(["-f", "%{file}", pattern]).output().ok()?;
    std::fs::read(String::from_utf8(out.stdout).ok()?).ok()
}

/// Famiglia del font dell'interfaccia del desktop, es. "Adwaita Sans" da "Adwaita Sans 11".
fn desktop_font_family() -> Option<String> {
    let name = INTERFACE_SCHEMAS.iter().find_map(|s| gsetting(s, "font-name"))?;
    let family = match name.rsplit_once(' ') {
        Some((family, size)) if size.parse::<f32>().is_ok() => family,
        _ => &name,
    };
    Some(family.to_string())
}

/// Il desktop chiede il tema scuro (portale, GNOME o nome del tema GTK).
pub fn desktop_prefers_dark() -> bool {
    let scheme = |schema| gsetting(schema, "color-scheme").is_some_and(|v| v == "prefer-dark");
    scheme("org.x.apps.portal")
        || scheme("org.gnome.desktop.interface")
        || INTERFACE_SCHEMAS
            .iter()
            .any(|s| gsetting(s, "gtk-theme").is_some_and(|v| v.to_lowercase().ends_with("-dark")))
}

/// Usa il font dell'interfaccia del desktop; senza fontconfig, un buon sans-serif noto.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let family = desktop_font_family().unwrap_or_else(|| "sans-serif".into());
    let system = fc_font(&family);
    let regular = system.clone().or_else(|| load_first(&[
        "/System/Library/Fonts/SFNS.ttf",
        "/usr/share/fonts/OTF/SF-Pro-Text-Regular.otf",
        "/usr/share/fonts/apple/SF-Pro-Text-Regular.otf",
        "/usr/share/fonts/TTF/Inter-Regular.ttf",
        "/usr/share/fonts/inter/Inter-Regular.otf",
        "/usr/share/fonts/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Regular.ttf",
        "C:\\Windows\\Fonts\\segoeui.ttf",
    ]));
    // Con un font di sistema il medio viene dalla stessa famiglia (per i font variabili è lo stesso file)
    let medium = if system.is_some() { fc_font(&format!("{family}:medium")) } else { None }.or_else(|| load_first(&[
        "/usr/share/fonts/OTF/SF-Pro-Text-Medium.otf",
        "/usr/share/fonts/apple/SF-Pro-Text-Medium.otf",
        "/usr/share/fonts/TTF/Inter-Medium.ttf",
        "/usr/share/fonts/inter/Inter-Medium.otf",
        "/usr/share/fonts/noto/NotoSans-Medium.ttf",
        "/usr/share/fonts/truetype/noto/NotoSans-Medium.ttf",
        "/usr/share/fonts/google-noto/NotoSans-Medium.ttf",
        "C:\\Windows\\Fonts\\seguisb.ttf",
    ]));
    let mono = fc_font("monospace").or_else(|| load_first(&[
        "/System/Library/Fonts/SFNSMono.ttf",
        "/usr/share/fonts/OTF/SF-Mono-Regular.otf",
        "/usr/share/fonts/TTF/JetBrainsMono-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ]));
    // Fallback per simboli matematici (√, ≤, ⁻¹ …)
    let math = load_first(&["/usr/share/fonts/TTF/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"]);

    let mut add = |name: &str, data: Option<Vec<u8>>, family: FontFamily, front: bool| {
        if let Some(data) = data {
            fonts.font_data.insert(name.into(), FontData::from_owned(data).into());
            let list = fonts.families.entry(family).or_default();
            if front { list.insert(0, name.into()) } else { list.push(name.into()) }
        }
    };
    let medium = medium.or_else(|| regular.clone());
    add("ui-regular", regular, FontFamily::Proportional, true);
    add("ui-mono", mono, FontFamily::Monospace, true);
    add("ui-math", math.clone(), FontFamily::Proportional, false);
    add("ui-medium", medium, FontFamily::Name("medium".into()), true);
    // La famiglia "medium" eredita i fallback di quella proporzionale.
    let prop = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.entry(FontFamily::Name("medium".into())).or_default().extend(prop);
    ctx.set_fonts(fonts);
}

pub fn medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("medium".into()))
}

pub fn apply_style(ctx: &egui::Context, p: &Palette) {
    ctx.global_style_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.0)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, FontId::proportional(13.0)),
            (TextStyle::Heading, medium(15.0)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        let s = &mut style.spacing;
        s.item_spacing = vec2(8.0, 6.0);
        s.button_padding = vec2(10.0, 4.0);
        s.interact_size = vec2(36.0, 24.0);
        s.scroll.floating = true;
        s.scroll.bar_width = 6.0;

        let v = &mut style.visuals;
        *v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        v.panel_fill = p.window;
        v.window_fill = p.card;
        v.extreme_bg_color = p.card;
        v.faint_bg_color = p.control;
        v.text_edit_bg_color = Some(Color32::TRANSPARENT);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(8);
        v.window_stroke = Stroke::new(1.0, p.card_border);
        v.window_shadow = Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if p.dark { 120 } else { 40 }) };
        v.popup_shadow = Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(if p.dark { 110 } else { 30 }) };
        v.selection.bg_fill = p.accent.gamma_multiply(if p.dark { 0.55 } else { 0.28 });
        v.selection.stroke = Stroke::new(1.0, p.accent);
        v.hyperlink_color = p.accent;
        v.text_cursor.stroke = Stroke::new(2.0, p.accent);
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Circle;

        let w = &mut v.widgets;
        for (state, fill) in [
            (&mut w.noninteractive, p.card),
            (&mut w.inactive, p.control),
            (&mut w.hovered, p.control),
            (&mut w.active, p.control),
            (&mut w.open, p.control),
        ] {
            state.corner_radius = CornerRadius::same(6);
            state.bg_fill = fill;
            state.weak_bg_fill = fill;
            state.expansion = 0.0;
        }
        w.noninteractive.bg_stroke = Stroke::new(1.0, p.separator);
        w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
        w.inactive.fg_stroke = Stroke::new(1.0, p.text);
        w.inactive.bg_stroke = Stroke::NONE;
        w.hovered.fg_stroke = Stroke::new(1.0, p.text);
        w.hovered.bg_stroke = Stroke::new(1.0, p.card_border);
        w.active.fg_stroke = Stroke::new(1.0, p.text);
        w.active.bg_stroke = Stroke::new(1.0, p.accent);
        w.active.weak_bg_fill = p.control_selected;
    });
}

// ───────────────────────────── Icone disegnate a mano ─────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Sidebar,
    Keypad,
    Sun,
    Moon,
    Plus,
    Minus,
    Fit,
    Close,
    Play,
    Pause,
    Backspace,
    Return,
    Help,
    Aspect,
    Plot,
}

pub fn paint_icon(painter: &egui::Painter, icon: Icon, rect: Rect, color: Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let stroke = Stroke::new(1.5, color);
    let r = |x: f32, y: f32| pos2(c.x + x * s, c.y + y * s);
    match icon {
        Icon::Sidebar => {
            let b = Rect::from_center_size(c, vec2(s * 0.9, s * 0.72));
            painter.rect_stroke(b, 3.0, stroke, egui::StrokeKind::Inside);
            painter.line_segment([pos2(b.left() + s * 0.3, b.top()), pos2(b.left() + s * 0.3, b.bottom())], stroke);
            for k in 0..3 {
                let y = b.top() + s * (0.17 + 0.12 * k as f32);
                painter.line_segment([pos2(b.left() + s * 0.08, y), pos2(b.left() + s * 0.2, y)], Stroke::new(1.2, color));
            }
        }
        Icon::Keypad => {
            let b = Rect::from_center_size(c, vec2(s * 0.75, s * 0.85));
            painter.rect_stroke(b, 3.0, stroke, egui::StrokeKind::Inside);
            let d = Rect::from_min_max(b.min + vec2(s * 0.12, s * 0.12), pos2(b.max.x - s * 0.12, b.min.y + s * 0.27));
            painter.rect_filled(d, 1.0, color);
            for j in 0..2 {
                for i in 0..3 {
                    let p = pos2(b.left() + s * (0.19 + 0.185 * i as f32), b.top() + s * (0.45 + 0.2 * j as f32));
                    painter.circle_filled(p, s * 0.05, color);
                }
            }
        }
        Icon::Sun => {
            painter.circle_stroke(c, s * 0.2, stroke);
            for k in 0..8 {
                let a = k as f32 * std::f32::consts::TAU / 8.0;
                let d = vec2(a.cos(), a.sin());
                painter.line_segment([c + d * s * 0.32, c + d * s * 0.44], stroke);
            }
        }
        Icon::Moon => {
            let bg = painter.ctx().global_style().visuals.panel_fill;
            painter.circle_filled(c, s * 0.36, color);
            painter.circle_filled(c + vec2(s * 0.17, -s * 0.14), s * 0.3, bg);
        }
        Icon::Plus => {
            painter.line_segment([r(-0.32, 0.0), r(0.32, 0.0)], stroke);
            painter.line_segment([r(0.0, -0.32), r(0.0, 0.32)], stroke);
        }
        Icon::Minus => {
            painter.line_segment([r(-0.32, 0.0), r(0.32, 0.0)], stroke);
        }
        Icon::Close => {
            painter.line_segment([r(-0.22, -0.22), r(0.22, 0.22)], stroke);
            painter.line_segment([r(0.22, -0.22), r(-0.22, 0.22)], stroke);
        }
        Icon::Fit => {
            let (a, b) = (0.36, 0.14);
            for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                painter.line_segment([r(sx * a, sy * a), r(sx * (a - b), sy * a)], stroke);
                painter.line_segment([r(sx * a, sy * a), r(sx * a, sy * (a - b))], stroke);
            }
            painter.circle_filled(c, s * 0.06, color);
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![r(-0.2, -0.28), r(0.3, 0.0), r(-0.2, 0.28)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            for x in [-0.13, 0.13] {
                painter.rect_filled(Rect::from_center_size(r(x, 0.0), vec2(s * 0.12, s * 0.5)), 1.0, color);
            }
        }
        Icon::Backspace => {
            let pts = vec![r(-0.42, 0.0), r(-0.2, -0.26), r(0.4, -0.26), r(0.4, 0.26), r(-0.2, 0.26)];
            painter.add(egui::Shape::closed_line(pts, stroke));
            painter.line_segment([r(-0.03, -0.1), r(0.19, 0.1)], stroke);
            painter.line_segment([r(0.19, -0.1), r(-0.03, 0.1)], stroke);
        }
        Icon::Return => {
            painter.line_segment([r(0.3, -0.3), r(0.3, 0.1)], stroke);
            painter.line_segment([r(0.3, 0.1), r(-0.3, 0.1)], stroke);
            painter.line_segment([r(-0.3, 0.1), r(-0.12, -0.08)], stroke);
            painter.line_segment([r(-0.3, 0.1), r(-0.12, 0.28)], stroke);
        }
        Icon::Help => {
            painter.circle_stroke(c, s * 0.38, stroke);
            painter.text(c + vec2(0.0, 0.5), egui::Align2::CENTER_CENTER, "?", medium(s * 0.5), color);
        }
        Icon::Aspect => {
            let b = Rect::from_center_size(c, vec2(s * 0.8, s * 0.8));
            painter.rect_stroke(b, 2.0, stroke, egui::StrokeKind::Inside);
            painter.text(c, egui::Align2::CENTER_CENTER, "1:1", medium(s * 0.32), color);
        }
        Icon::Plot => {
            // Assi e una curva
            painter.line_segment([r(-0.38, -0.38), r(-0.38, 0.38)], stroke);
            painter.line_segment([r(-0.38, 0.38), r(0.4, 0.38)], stroke);
            let pts = (0..=12)
                .map(|k| {
                    let t = k as f32 / 12.0;
                    r(-0.3 + 0.66 * t, 0.05 - 0.22 * (t * std::f32::consts::TAU).sin())
                })
                .collect();
            painter.add(egui::Shape::line(pts, stroke));
        }
    }
}

// ───────────────────────────── Widget ─────────────────────────────

/// Pulsante della toolbar: solo icona, sfondo arrotondato al passaggio del mouse.
pub fn toolbar_button(ui: &mut Ui, p: &Palette, icon: Icon, active: bool, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(32.0, 28.0), Sense::click());
    let painter = ui.painter();
    if active {
        painter.rect_filled(rect, 6.0, p.control);
    } else if resp.hovered() {
        painter.rect_filled(rect, 6.0, p.hover);
    }
    let color = if active { p.accent } else { p.text2 };
    paint_icon(painter, icon, Rect::from_center_size(rect.center(), Vec2::splat(18.0)), color);
    resp.on_hover_text(tooltip)
}

/// Controllo segmentato (due o più pulsanti collegati).
pub fn segmented(ui: &mut Ui, p: &Palette, selected: &mut usize, labels: &[&str]) -> Response {
    let font = FontId::proportional(12.0);
    let widths: Vec<f32> = labels
        .iter()
        .map(|l| ui.fonts_mut(|f| f.layout_no_wrap(l.to_string(), font.clone(), p.text).size().x) + 22.0)
        .collect();
    let total = vec2(widths.iter().sum::<f32>() + 4.0, 26.0);
    let (rect, mut resp) = ui.allocate_exact_size(total, Sense::click());
    let painter = ui.painter();
    painter.rect_filled(rect, 7.0, p.control);
    let mut x = rect.left() + 2.0;
    for (i, (label, w)) in labels.iter().zip(&widths).enumerate() {
        let seg = Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(*w, rect.height() - 4.0));
        if i == *selected {
            painter.add(egui::Shape::from(Shadow {
                offset: [0, 1],
                blur: 3,
                spread: 0,
                color: Color32::from_black_alpha(if p.dark { 60 } else { 30 }),
            }
            .as_shape(seg, 5.0)));
            painter.rect_filled(seg, 5.0, p.control_selected);
        } else if i + 1 != *selected && i + 1 < labels.len() {
            painter.line_segment(
                [pos2(seg.right(), seg.top() + 5.0), pos2(seg.right(), seg.bottom() - 5.0)],
                Stroke::new(1.0, p.text3.gamma_multiply(0.5)),
            );
        }
        painter.text(seg.center(), egui::Align2::CENTER_CENTER, *label, if i == *selected { medium(12.0) } else { font.clone() }, p.text);
        if resp.clicked() && resp.interact_pointer_pos().is_some_and(|pos| seg.contains(pos)) && *selected != i {
            *selected = i;
            resp.mark_changed();
        }
        x += w;
    }
    resp
}

/// Slider sottile con pomello bianco, come quelli di Adwaita.
pub fn slider(ui: &mut Ui, p: &Palette, value: &mut f64, min: f64, max: f64, width: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 20.0), Sense::click_and_drag());
    let knob_r = 8.0;
    let track = Rect::from_min_max(
        pos2(rect.left() + knob_r, rect.center().y - 2.0),
        pos2(rect.right() - knob_r, rect.center().y + 2.0),
    );
    if let Some(pos) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.clicked())
        && max > min
    {
        let t = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64;
        let mut v = min + t * (max - min);
        // Aggancio a valori "tondi" con 2 cifre rispetto all'ampiezza
        let step = 10f64.powf(((max - min) / 200.0).log10().floor());
        v = (v / step).round() * step;
        if v != *value {
            *value = v;
            resp.mark_changed();
        }
    }
    let t = if max > min { ((*value - min) / (max - min)).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let kx = track.left() + t * track.width();
    let painter = ui.painter();
    painter.rect_filled(track, 2.0, p.control);
    painter.rect_filled(Rect::from_min_max(track.min, pos2(kx, track.max.y)), 2.0, p.accent);
    let kc = pos2(kx, rect.center().y);
    painter.add(egui::Shape::from(
        Shadow { offset: [0, 1], blur: 4, spread: 0, color: Color32::from_black_alpha(70) }
            .as_shape(Rect::from_center_size(kc, Vec2::splat(2.0 * knob_r)), knob_r),
    ));
    let knob = if p.dark { hex(0xE5E5EA) } else { Color32::WHITE };
    painter.circle(kc, knob_r, knob, Stroke::new(0.5, Color32::from_black_alpha(30)));
    resp
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum KeyKind {
    Digit,
    Function,
    Top,
    Operator,
}

pub enum KeyFace<'a> {
    Text(&'a str),
    Icon(Icon),
}

/// Tasto della calcolatrice: rettangolo arrotondato come i pulsanti di Adwaita.
pub fn calc_key(ui: &mut Ui, p: &Palette, face: KeyFace, kind: KeyKind, size: Vec2) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let (fill, text) = match kind {
        KeyKind::Digit => (p.key_digit, p.key_text),
        KeyKind::Function => (p.key_fn, p.key_text),
        KeyKind::Top => (p.key_top, p.key_top_text),
        KeyKind::Operator => (p.key_op, Color32::WHITE),
    };
    let fill = if resp.is_pointer_button_down_on() {
        if p.dark { fill.linear_multiply(1.5) } else { fill.gamma_multiply(0.8) }
    } else if resp.hovered() {
        if p.dark { fill.linear_multiply(1.18) } else { fill.gamma_multiply(0.93) }
    } else {
        fill
    };
    let painter = ui.painter();
    let radius = 8.0;
    painter.rect_filled(rect, radius, fill);
    if !p.dark && kind == KeyKind::Digit {
        // Nel tema chiaro i tasti bianchi hanno bisogno di un bordo sullo sfondo grigio
        painter.rect_stroke(rect, radius, Stroke::new(1.0, p.card_border), egui::StrokeKind::Inside);
    }
    match face {
        KeyFace::Text(t) => {
            let size = if t.chars().count() > 2 { 14.0f32 } else { 19.0 }.min(size.y * 0.52);
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, t, FontId::proportional(size), text);
        }
        KeyFace::Icon(icon) => paint_icon(painter, icon, Rect::from_center_size(rect.center(), Vec2::splat(20.0)), text),
    }
    resp
}

/// Pallino colorato: pieno se la curva è visibile, vuoto se nascosta.
pub fn color_dot(ui: &mut Ui, color: Color32, filled: bool, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let c = rect.center();
    let painter = ui.painter();
    if !enabled {
        painter.circle_filled(c, 3.0, ui.visuals().weak_text_color());
    } else if filled {
        painter.circle_filled(c, 6.5, color);
        if resp.hovered() {
            painter.circle_stroke(c, 8.5, Stroke::new(1.5, color.gamma_multiply(0.4)));
        }
    } else {
        painter.circle_stroke(c, 5.75, Stroke::new(1.5, color));
    }
    resp
}

pub fn section_header(ui: &mut Ui, p: &Palette, text: &str) {
    ui.label(egui::RichText::new(text).font(medium(11.0)).color(p.text3));
}
