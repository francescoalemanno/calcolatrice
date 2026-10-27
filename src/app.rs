use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align, Align2, Color32, FontId, Id, Key, Layout, Margin, Modifiers, Pos2, Rect,
    RichText, Sense, Shadow, Stroke, StrokeKind, TextureHandle, Ui, ViewportCommand, pos2, vec2,
};
use egui_plot::{HLine, HoverPosition, Line, Plot as EguiPlot, PlotBounds, PlotImage, PlotPoint, PlotPoints, Points, VLine};

use crate::analysis::{self, Input, Notebook, Param, Plot, Summary, pretty};
use crate::eval::{FUNCTION_HELP, format_number, format_short};
use crate::plotting::{self, View};
use crate::theme::{self, Icon, KeyFace, KeyKind, Palette};

/// Altezza delle barre in cima a barra laterale e grafico
const HEADER_H: f32 = 46.0;
const DEFAULT_VIEW: View = View { x0: -10.0, x1: 10.0, y0: -7.0, y1: 7.0 };

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Entry {
    id: u64,
    text: String,
    color: usize,
    visible: bool,
    axis: Option<String>,
    /// Intervallo del parametro per curve parametriche e polari
    range: [f64; 2],
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct App {
    entries: Vec<Entry>,
    /// Formula in scrittura nella barra in basso, non ancora calcolata
    draft: String,
    params: BTreeMap<String, Param>,
    next_id: u64,
    degrees: bool,
    dark: Option<bool>,
    show_sidebar: bool,
    show_keypad: bool,
    /// Pannello del grafico (senza, la barra laterale occupa tutta la finestra)
    show_plot: bool,
    /// Tastierino scientifico (altrimenti base)
    scientific: bool,
    equal_aspect: bool,
    /// Tasto "2nd" attivo: funzioni inverse sul tastierino scientifico
    #[serde(skip)]
    second: bool,
    #[serde(skip)]
    show_help: bool,
    #[serde(skip)]
    styled_dark: Option<bool>,
    /// Il desktop usa il tema scuro (letto all'avvio)
    #[serde(skip)]
    system_dark: bool,
    /// Valore di `show_plot` per cui è stata impostata la dimensione minima della finestra
    #[serde(skip)]
    min_size_for: Option<bool>,
    /// Ultima riga con il cursore: destinazione del tastierino
    #[serde(skip)]
    active: Option<u64>,
    /// Riga a cui dare il fuoco al prossimo frame, con posizione del cursore
    #[serde(skip)]
    focus: Option<(u64, Option<usize>)>,
    /// Riga da portare in vista nell'elenco quando compare
    #[serde(skip)]
    reveal: Option<u64>,
    #[serde(skip)]
    view: Option<View>,
    #[serde(skip)]
    pending_bounds: Option<View>,
    #[serde(skip)]
    plot_width: f32,
    #[serde(skip)]
    heat: HashMap<u64, (u64, TextureHandle)>,
    #[serde(skip)]
    copied: Option<(u64, f64)>,
}

impl Default for App {
    fn default() -> Self {
        let demo = [
            "f(x) = a sin(b x)",
            "diff(f(x), x)",
            "x^2 + y^2 = 16",
            "x^3 - 2x = 1",
            "integral(exp(-t^2), t, -inf, inf)",
            "12! / (3! 9!)",
        ];
        let entries: Vec<Entry> = demo
            .iter()
            .enumerate()
            .map(|(i, t)| Entry {
                id: i as u64,
                text: t.to_string(),
                color: i % theme::SERIES_COUNT,
                visible: true,
                axis: None,
                range: [0.0, std::f64::consts::TAU],
            })
            .collect();
        let mut params = BTreeMap::new();
        params.insert("a".into(), Param { value: 2.0, ..Default::default() });
        params.insert("b".into(), Param { value: 1.5, min: 0.0, max: 5.0, ..Default::default() });
        App {
            next_id: entries.len() as u64,
            entries,
            draft: String::new(),
            params,
            degrees: false,
            dark: None,
            show_sidebar: true,
            show_keypad: true,
            show_plot: true,
            scientific: false,
            equal_aspect: true,
            second: false,
            show_help: false,
            styled_dark: None,
            system_dark: false,
            min_size_for: None,
            active: None,
            focus: None,
            reveal: None,
            view: None,
            pending_bounds: Some(DEFAULT_VIEW),
            plot_width: 800.0,
            heat: HashMap::new(),
            copied: None,
        }
    }
}

enum KeyAction {
    Insert(&'static str),
    Clear,
    Backspace,
    NewLine,
    Second,
    Degrees,
}

/// Id riservato alla barra di inserimento in basso (stesso spazio degli id delle righe)
const COMPOSER: u64 = u64::MAX;

fn text_id(entry: u64) -> Id {
    Id::unique(("entry-text", entry))
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn byte_at(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map_or(s.len(), |(b, _)| b)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let mut app: App = cc
            .storage
            .and_then(|s| eframe::get_value(s, eframe::APP_KEY))
            .unwrap_or_default();
        app.pending_bounds = Some(DEFAULT_VIEW);
        app.plot_width = 800.0;
        app.system_dark = theme::desktop_prefers_dark();
        // Almeno un pannello deve essere visibile
        app.show_sidebar |= !app.show_plot;
        // Si comincia a scrivere dalla barra in basso
        app.active = Some(COMPOSER);
        app.focus = Some((COMPOSER, None));
        app
    }

    /// Aggiunge in fondo all'elenco la formula della barra in basso e la svuota.
    fn submit_draft(&mut self) {
        let text = self.draft.trim().to_string();
        if !text.is_empty() {
            let id = self.add_entry(self.entries.len(), text);
            self.reveal = Some(id);
            self.draft.clear();
        }
        self.active = Some(COMPOSER);
        self.focus = Some((COMPOSER, Some(0)));
    }

    fn add_entry(&mut self, at: usize, text: String) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        // Colore successivo a quelli usati, per non ripetere subito lo stesso
        let used: Vec<usize> = self.entries.iter().map(|e| e.color).collect();
        let color = (0..theme::SERIES_COUNT).find(|c| !used.contains(c)).unwrap_or(id as usize % theme::SERIES_COUNT);
        let at = at.min(self.entries.len());
        self.entries.insert(
            at,
            Entry { id, text, color, visible: true, axis: None, range: [0.0, std::f64::consts::TAU] },
        );
        self.focus = Some((id, None));
        id
    }

    fn palette(&self, ctx: &egui::Context) -> Palette {
        let dark = self.dark.unwrap_or_else(|| self.system_dark || ctx.system_theme() == Some(egui::Theme::Dark));
        if dark { theme::DARK } else { theme::LIGHT }
    }

    fn animate_params(&mut self, ctx: &egui::Context) {
        let dt = ctx.input(|i| i.stable_dt).min(0.1) as f64;
        let mut any = false;
        for p in self.params.values_mut().filter(|p| p.playing) {
            any = true;
            let span = (p.max - p.min).max(1e-12);
            p.value += p.direction * span * dt / 6.0;
            if p.value >= p.max {
                p.value = p.max;
                p.direction = -1.0;
            } else if p.value <= p.min {
                p.value = p.min;
                p.direction = 1.0;
            }
        }
        if any {
            ctx.request_repaint();
        }
    }

    /// Esegue un tasto del tastierino sulla riga attiva, rispettando il cursore.
    fn key_action(&mut self, ctx: &egui::Context, action: KeyAction) {
        match action {
            KeyAction::Second => return self.second = !self.second,
            KeyAction::Degrees => return self.degrees = !self.degrees,
            _ => {}
        }
        // Senza una riga dell'elenco attiva il tastierino scrive nella barra in basso
        let row = self.active.and_then(|id| self.entries.iter().position(|e| e.id == id));
        if let KeyAction::NewLine = action {
            match row {
                Some(i) => {
                    let id = self.add_entry(i + 1, String::new());
                    self.active = Some(id);
                }
                None => self.submit_draft(),
            }
            return;
        }
        let (eid, text) = match row {
            Some(i) => (self.entries[i].id, &mut self.entries[i].text),
            None => (COMPOSER, &mut self.draft),
        };
        let tid = text_id(eid);
        let len = char_len(text);
        let mut state = egui::text_edit::TextEditState::load(ctx, tid).unwrap_or_default();
        let (start, end) = state
            .cursor
            .char_range()
            .map(|r| {
                let [a, b] = r.sorted_cursors();
                (a.index.0.min(len), b.index.0.min(len))
            })
            .unwrap_or((len, len));
        let (bs, be) = (byte_at(text, start), byte_at(text, end));
        let cursor = match action {
            KeyAction::Insert(s) => {
                text.replace_range(bs..be, s);
                start + char_len(s)
            }
            KeyAction::Clear => {
                text.clear();
                0
            }
            KeyAction::Backspace => {
                if start != end {
                    text.replace_range(bs..be, "");
                    start
                } else if start > 0 {
                    let b0 = byte_at(text, start - 1);
                    text.replace_range(b0..bs, "");
                    start - 1
                } else {
                    0
                }
            }
            KeyAction::NewLine | KeyAction::Second | KeyAction::Degrees => unreachable!(),
        };
        state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
        state.store(ctx, tid);
        self.focus = Some((eid, Some(cursor)));
    }

    fn analyze_all(&mut self) -> (Notebook, Vec<Option<Vec<f64>>>) {
        let inputs: Vec<Input> = self
            .entries
            .iter()
            .map(|e| Input { text: &e.text, axis_override: e.axis.as_deref() })
            .collect();
        let mut nb = analysis::analyze(&inputs, &mut self.params, self.degrees);
        let view = self.view.unwrap_or(DEFAULT_VIEW);
        let roots = nb
            .rows
            .iter_mut()
            .map(|row| match &mut row.plot {
                Plot::Roots { f, horizontal, .. } => {
                    let (a, b) = if *horizontal { (view.y0, view.y1) } else { (view.x0, view.x1) };
                    Some(plotting::find_roots(f, a, b, 2000))
                }
                _ => None,
            })
            .collect();
        (nb, roots)
    }

    fn hash_inputs(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for e in &self.entries {
            e.text.hash(&mut h);
        }
        for (k, p) in &self.params {
            k.hash(&mut h);
            p.value.to_bits().hash(&mut h);
        }
        self.degrees.hash(&mut h);
        h.finish()
    }
}

impl eframe::App for App {
    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let pal = self.palette(&ctx);
        if self.styled_dark != Some(pal.dark) {
            ctx.set_theme(if pal.dark { egui::Theme::Dark } else { egui::Theme::Light });
            theme::apply_style(&ctx, &pal);
            self.styled_dark = Some(pal.dark);
            self.heat.clear();
        }
        self.animate_params(&ctx);
        if self.min_size_for != Some(self.show_plot) {
            // Senza grafico la calcolatrice può stare in una finestra stretta
            let min = if self.show_plot { vec2(760.0, 500.0) } else { vec2(320.0, 500.0) };
            ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(min));
            // Il grafico ricompare in una finestra stretta: allargala
            if let Some(inner) = ctx.input(|i| i.viewport().inner_rect)
                && self.show_plot
                && inner.width() < min.x
            {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(vec2(1200.0, inner.height().max(min.y))));
            }
            self.min_size_for = Some(self.show_plot);
        }

        // Analisi di tutte le righe (costa microsecondi: si rifà a ogni frame).
        let (mut nb, mut roots) = self.analyze_all();

        if !self.show_plot {
            // Solo la barra laterale, a tutta finestra
            let frame = egui::Frame::new().fill(pal.sidebar);
            egui::CentralPanel::default().frame(frame).show(ui, |ui| self.sidebar(ui, &pal, &mut nb, &roots));
            if nb.rows.len() != self.entries.len() {
                ctx.request_repaint();
            }
        } else {
            if self.show_sidebar {
                let frame = egui::Frame::new().fill(pal.sidebar);
                egui::Panel::left("sidebar")
                    .resizable(true)
                    .default_size(350.0)
                    .size_range(300.0..=560.0)
                    .frame(frame)
                    .show(ui, |ui| self.sidebar(ui, &pal, &mut nb, &roots));
                // La sidebar può aggiungere o togliere righe: rifai l'analisi per restare allineati.
                if nb.rows.len() != self.entries.len() {
                    (nb, roots) = self.analyze_all();
                    ctx.request_repaint();
                }
            }
            egui::CentralPanel::no_frame().show(ui, |ui| {
                self.toolbar(ui, &pal);
                self.plot_area(ui, &pal, &mut nb, &roots);
            });
        }

        if self.show_help {
            self.help_window(&ctx, &pal);
        }

    }
}

// ─────────────────────────────── Sidebar ───────────────────────────────

impl App {
    fn sidebar(&mut self, ui: &mut Ui, pal: &Palette, nb: &mut Notebook, roots: &[Option<Vec<f64>>]) {
        let ctx = ui.ctx().clone();
        let top = ui.max_rect();
        let bar = Rect::from_min_size(top.min, vec2(top.width(), HEADER_H));
        // Pulsanti in alto a destra; senza grafico qui stanno anche quelli della toolbar
        let buttons = Rect::from_min_max(pos2(bar.left() + 12.0, bar.top() + 9.0), pos2(bar.right() - 12.0, bar.top() + 37.0));
        let mut buttons_ui = ui.new_child(egui::UiBuilder::new().max_rect(buttons).layout(Layout::right_to_left(Align::Center)));
        buttons_ui.spacing_mut().item_spacing.x = 4.0;
        if theme::toolbar_button(&mut buttons_ui, pal, Icon::Plus, false, "Nuova espressione").clicked() {
            self.active = Some(COMPOSER);
            self.focus = Some((COMPOSER, None));
        }
        if !self.show_plot {
            if theme::toolbar_button(&mut buttons_ui, pal, Icon::Plot, false, "Mostra grafico").clicked() {
                self.show_plot = true;
            }
            if theme::toolbar_button(&mut buttons_ui, pal, Icon::Keypad, self.show_keypad, "Tastierino").clicked() {
                self.show_keypad = !self.show_keypad;
            }
            if theme::toolbar_button(&mut buttons_ui, pal, Icon::Help, self.show_help, "Guida").clicked() {
                self.show_help = !self.show_help;
            }
            let dark_icon = if pal.dark { Icon::Sun } else { Icon::Moon };
            if theme::toolbar_button(&mut buttons_ui, pal, dark_icon, false, "Tema chiaro/scuro").clicked() {
                self.dark = Some(!pal.dark);
            }
        }
        ui.add_space(HEADER_H);

        if self.show_keypad {
            // Il tastierino non deve prendere più di ~60% della barra laterale.
            let max_keys_h = ui.available_height() * 0.6 - 60.0;
            egui::Panel::bottom("keypad")
                .resizable(false)
                .frame(egui::Frame::new().inner_margin(Margin { left: 14, right: 14, top: 10, bottom: 14 }))
                .show(ui, |ui| {
                    ui.painter().hline(ui.max_rect().x_range(), ui.max_rect().top() - 10.0, Stroke::new(1.0, pal.separator));
                    let mut mode = self.scientific as usize;
                    ui.vertical_centered(|ui| theme::segmented(ui, pal, &mut mode, &["Base", "Scientifica"]));
                    self.scientific = mode == 1;
                    ui.add_space(10.0);
                    if let Some(a) = keypad(ui, pal, self.scientific, self.second, self.degrees, max_keys_h) {
                        self.key_action(&ctx, a);
                    }
                });
        }

        // Barra di inserimento sempre in vista, fuori dall'area che scorre
        egui::Panel::bottom("composer")
            .resizable(false)
            .frame(egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 6, bottom: 10 }))
            .show(ui, |ui| self.composer(ui, pal));

        egui::CentralPanel::no_frame().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 0, bottom: 12 }).show(ui, |ui| {
                    ui.add_space(2.0);
                    theme::section_header(ui, pal, "ESPRESSIONI");
                    ui.add_space(2.0);
                    self.entry_list(ui, pal, nb, roots);
                    if !nb.params.is_empty() {
                        ui.add_space(14.0);
                        theme::section_header(ui, pal, "PARAMETRI");
                        ui.add_space(2.0);
                        self.param_list(ui, pal, &nb.params);
                    }
                });
            });
        });
    }

    /// [ Scrivi una formula… ] [ Calcola ]: Invio o il pulsante aggiungono la formula in fondo.
    fn composer(&mut self, ui: &mut Ui, pal: &Palette) {
        let ctx = ui.ctx().clone();
        let tid = text_id(COMPOSER);
        let has_focus = ctx.memory(|m| m.has_focus(tid));
        if has_focus
            && let Some(last) = self.entries.last()
            && ui.input_mut(|inp| inp.consume_key(Modifiers::NONE, Key::ArrowUp))
        {
            self.focus = Some((last.id, None));
        }
        let h = 36.0;
        let button_w = 92.0;
        let mut submit = false;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let field_w = ui.available_width() - button_w - 8.0;
            let frame = egui::Frame::new()
                .fill(pal.card)
                .stroke(Stroke::new(1.0, if has_focus { pal.accent } else { pal.card_border }))
                .corner_radius(10)
                .inner_margin(Margin { left: 12, right: 8, top: 8, bottom: 8 });
            let field = frame.show(ui, |ui| {
                ui.set_width(field_w - 20.0);
                let edit = egui::TextEdit::singleline(&mut self.draft)
                    .id(tid)
                    .frame(egui::Frame::NONE)
                    .font(FontId::proportional(16.0))
                    .text_color(pal.text)
                    .hint_text(RichText::new("Scrivi una formula…").color(pal.text3))
                    .desired_width(f32::INFINITY);
                let resp = ui.add(edit);
                if resp.has_focus() || resp.gained_focus() {
                    self.active = Some(COMPOSER);
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    submit = true;
                }
                if let Some((fid, cursor)) = self.focus
                    && fid == COMPOSER
                {
                    resp.request_focus();
                    if let Some(c) = cursor {
                        let mut st = egui::text_edit::TextEditState::load(&ctx, tid).unwrap_or_default();
                        st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(c))));
                        st.store(&ctx, tid);
                    }
                    self.focus = None;
                }
            });
            if has_focus {
                ui.painter().rect_stroke(
                    field.response.rect.expand(2.0),
                    12,
                    Stroke::new(3.0, pal.accent.gamma_multiply(0.35)),
                    StrokeKind::Outside,
                );
            }
            let button = theme::calc_key(ui, pal, KeyFace::Text("Calcola"), KeyKind::Operator, vec2(button_w, h));
            submit |= button.on_hover_text("Aggiungi all'elenco (Invio)").clicked();
        });
        if submit {
            self.submit_draft();
        }
    }

    fn entry_list(&mut self, ui: &mut Ui, pal: &Palette, nb: &mut Notebook, roots: &[Option<Vec<f64>>]) {
        let ctx = ui.ctx().clone();
        let mut delete: Option<usize> = None;
        let mut insert_after: Option<usize> = None;
        let mut move_focus: Option<(usize, i32)> = None;
        let now = ctx.input(|i| i.time);
        // Il tastierino può aver appena aggiunto una riga non ancora analizzata.
        let n = self.entries.len().min(nb.rows.len());

        for i in 0..n {
            let row = &nb.rows[i];
            let tid = text_id(self.entries[i].id);
            let has_focus = ctx.memory(|m| m.has_focus(tid));

            // Scorciatoie da tastiera prima che il TextEdit consumi gli eventi
            if has_focus {
                if ui.input_mut(|inp| inp.consume_key(Modifiers::NONE, Key::ArrowUp)) {
                    move_focus = Some((i, -1));
                }
                if ui.input_mut(|inp| inp.consume_key(Modifiers::NONE, Key::ArrowDown)) {
                    move_focus = Some((i, 1));
                }
                if self.entries[i].text.is_empty()
                    && ui.input_mut(|inp| inp.consume_key(Modifiers::NONE, Key::Backspace))
                {
                    delete = Some(i);
                    move_focus = Some((i, -1));
                }
            }

            let entry = &mut self.entries[i];
            let color = theme::series_color(entry.color, pal.dark);
            let plottable = row.plot.is_some();
            // Riga appena aggiunta o raggiunta col tastierino/frecce: va portata in vista
            let reveal = self.focus.is_some_and(|(fid, _)| fid == entry.id) || self.reveal == Some(entry.id);
            let frame = egui::Frame::new()
                .fill(pal.card)
                .stroke(Stroke::new(1.0, if has_focus { pal.accent } else { pal.card_border }))
                .corner_radius(10)
                .inner_margin(Margin { left: 6, right: 6, top: 6, bottom: 6 });
            let card = frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let dot = theme::color_dot(ui, color, entry.visible, plottable);
                    if dot.clicked() {
                        entry.visible = !entry.visible;
                    }
                    if dot.secondary_clicked() || (dot.clicked() && ui.input(|i| i.modifiers.shift)) {
                        entry.color = (entry.color + 1) % theme::SERIES_COUNT;
                    }
                    let dot = dot.on_hover_text("Clic: mostra/nascondi · clic destro: cambia colore");
                    let _ = dot;
                    let w = ui.available_width() - 26.0;
                    let edit = egui::TextEdit::singleline(&mut entry.text)
                        .id(tid)
                        .frame(egui::Frame::NONE)
                        .font(FontId::proportional(16.0))
                        .text_color(pal.text)
                        .hint_text(RichText::new("Scrivi una formula…").color(pal.text3))
                        .desired_width(w);
                    let resp = ui.add(edit);
                    if resp.has_focus() || resp.gained_focus() {
                        self.active = Some(entry.id);
                    }
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        insert_after = Some(i);
                    }
                    if let Some((fid, cursor)) = self.focus
                        && fid == entry.id
                    {
                        resp.request_focus();
                        if let Some(c) = cursor {
                            let mut st = egui::text_edit::TextEditState::load(&ctx, tid).unwrap_or_default();
                            st.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(c))));
                            st.store(&ctx, tid);
                        }
                        self.focus = None;
                    }
                    let (xr, xresp) = ui.allocate_exact_size(vec2(20.0, 22.0), Sense::click());
                    let xc = if xresp.hovered() { pal.red } else { pal.text3 };
                    theme::paint_icon(ui.painter(), Icon::Close, Rect::from_center_size(xr.center(), vec2(16.0, 16.0)), xc);
                    if xresp.on_hover_text("Elimina").clicked() {
                        delete = Some(i);
                    }
                });
                summary_row(ui, pal, entry, row, roots[i].as_deref(), &mut self.copied, now);
            });
            if reveal {
                ui.scroll_to_rect(card.response.rect.expand(4.0), None);
                self.reveal = None;
            }
            if has_focus {
                // Anello di fuoco
                ui.painter().rect_stroke(
                    card.response.rect.expand(2.0),
                    12,
                    Stroke::new(3.0, pal.accent.gamma_multiply(0.35)),
                    StrokeKind::Outside,
                );
            }
            ui.add_space(6.0);
        }

        if let Some((i, dir)) = move_focus {
            let j = i as i32 + dir;
            match usize::try_from(j).ok().and_then(|j| self.entries.get(j)) {
                Some(e) => self.focus = Some((e.id, None)),
                // Sotto all'ultima riga, o eliminata la prima: si torna alla barra in basso
                None if dir > 0 || delete.is_some() => {
                    self.active = Some(COMPOSER);
                    self.focus = Some((COMPOSER, None));
                }
                None => {}
            }
        }
        if let Some(i) = delete {
            let id = self.entries[i].id;
            self.entries.remove(i);
            self.heat.remove(&id);
        }
        if let Some(i) = insert_after {
            let id = self.add_entry(i + 1, String::new());
            self.active = Some(id);
        }
    }

    fn param_list(&mut self, ui: &mut Ui, pal: &Palette, names: &[String]) {
        for name in names {
            let Some(p) = self.params.get_mut(name) else { continue };
            egui::Frame::new()
                .fill(pal.card)
                .stroke(Stroke::new(1.0, pal.card_border))
                .corner_radius(10)
                .inner_margin(Margin { left: 12, right: 8, top: 6, bottom: 8 })
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(pretty(name)).font(FontId::proportional(16.0)).italics().color(pal.text));
                        ui.label(RichText::new("=").color(pal.text2));
                        ui.add(
                            egui::DragValue::new(&mut p.value)
                                .speed(((p.max - p.min) / 300.0).max(1e-6))
                                .custom_formatter(|v, _| format_short(v)),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let icon = if p.playing { Icon::Pause } else { Icon::Play };
                            if theme::toolbar_button(ui, pal, icon, p.playing, "Anima").clicked() {
                                p.playing = !p.playing;
                            }
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        fn small(v: &mut f64) -> egui::DragValue<'_> {
                            egui::DragValue::new(v).speed(0.1).custom_formatter(|v, _| format_short(v))
                        }
                        ui.add_sized([44.0, 18.0], small(&mut p.min));
                        let w = ui.available_width() - 50.0;
                        if theme::slider(ui, pal, &mut p.value, p.min, p.max, w).dragged() {
                            p.playing = false;
                        }
                        ui.add_sized([44.0, 18.0], small(&mut p.max));
                    });
                    if p.min > p.max {
                        std::mem::swap(&mut p.min, &mut p.max);
                    }
                });
            ui.add_space(6.0);
        }
    }
}

fn summary_row(
    ui: &mut Ui,
    pal: &Palette,
    entry: &mut Entry,
    row: &analysis::Analysis,
    roots: Option<&[f64]>,
    copied: &mut Option<(u64, f64)>,
    now: f64,
) {
    let small = FontId::proportional(12.0);
    let show_copy = |ui: &mut Ui, text: String, font: FontId, color: Color32, copied: &mut Option<(u64, f64)>| {
        let h = font.size + 8.0;
        ui.allocate_ui_with_layout(vec2(ui.available_width(), h), Layout::right_to_left(Align::Center), |ui| {
            let recently = copied.is_some_and(|(id, t)| id == entry.id && now - t < 1.2);
            let label = if recently { "Copiato".to_string() } else { text.clone() };
            let resp = ui
                .add(egui::Label::new(RichText::new(label).font(font).color(if recently { pal.accent } else { color })).sense(Sense::click()))
                .on_hover_cursor(egui::CursorIcon::Copy)
                .on_hover_text("Clic per copiare");
            if resp.clicked() {
                ui.ctx().copy_text(text.trim_start_matches("= ").replace('−', "-"));
                *copied = Some((entry.id, now));
            }
            if recently {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(300));
            }
        });
    };
    match &row.summary {
        Summary::Empty => {}
        Summary::Value(v) => {
            show_copy(ui, format!("= {}", format_number(*v)), FontId::proportional(24.0), pal.text, copied);
        }
        Summary::Defined(v) => {
            show_copy(ui, format!("= {}", format_number(*v)), FontId::proportional(14.0), pal.text2, copied);
        }
        Summary::Bool { ok, lhs, rhs } => {
            ui.allocate_ui_with_layout(vec2(ui.available_width(), 22.0), Layout::right_to_left(Align::Center), |ui| {
                let (txt, col) = if *ok {
                    ("vero".to_string(), pal.green)
                } else {
                    (format!("falso: {} ≠ {}", format_short(*lhs), format_short(*rhs)), pal.red)
                };
                ui.label(RichText::new(txt).font(theme::medium(13.0)).color(col));
            });
        }
        Summary::Error(e) => {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(26.0);
                ui.label(RichText::new(e).font(small.clone()).color(pal.red));
            });
        }
        Summary::Function(sig) | Summary::Info(sig) => {
            ui.horizontal_wrapped(|ui| {
                ui.add_space(26.0);
                ui.spacing_mut().item_spacing.x = 4.0;
                let text = if matches!(row.summary, Summary::Function(_)) { format!("funzione {sig}") } else { sig.clone() };
                ui.label(RichText::new(text).font(small.clone()).color(pal.text2));
                if row.axis_choices.len() > 1 {
                    let cur = row.axis.clone().unwrap_or_default();
                    let resp = ui
                        .add(
                            egui::Label::new(RichText::new(format!("· asse: {} ⇄", pretty(&cur))).font(small.clone()).color(pal.accent))
                                .sense(Sense::click()),
                        )
                        .on_hover_text("Cambia la variabile sull'asse orizzontale");
                    if resp.clicked() {
                        let k = row.axis_choices.iter().position(|c| *c == cur).unwrap_or(0);
                        entry.axis = Some(row.axis_choices[(k + 1) % row.axis_choices.len()].clone());
                    }
                }
                if !row.params.is_empty() {
                    let ps: Vec<String> = row.params.iter().map(|p| pretty(p)).collect();
                    ui.label(RichText::new(format!("· slider: {}", ps.join(", "))).font(small.clone()).color(pal.text3));
                }
            });
            if let Plot::Parametric { .. } | Plot::Polar { .. } = row.plot {
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    let var = if matches!(row.plot, Plot::Polar { .. }) { "θ" } else { "t" };
                    ui.label(RichText::new(format!("{var} da")).font(small.clone()).color(pal.text2));
                    ui.add(egui::DragValue::new(&mut entry.range[0]).speed(0.05).custom_formatter(|v, _| format_short(v)));
                    ui.label(RichText::new("a").font(small.clone()).color(pal.text2));
                    ui.add(egui::DragValue::new(&mut entry.range[1]).speed(0.05).custom_formatter(|v, _| format_short(v)));
                });
            }
            if let (Some(rs), Plot::Roots { var, .. }) = (roots, &row.plot) {
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(26.0);
                    let txt = if rs.is_empty() {
                        "nessuna soluzione nell'area visibile".to_string()
                    } else {
                        let shown: Vec<String> = rs.iter().take(6).map(|r| format_short(*r)).collect();
                        let more = if rs.len() > 6 { format!(" (+{})", rs.len() - 6) } else { String::new() };
                        format!("{} ≈ {}{more}", pretty(var), shown.join(",  "))
                    };
                    ui.label(RichText::new(txt).font(theme::medium(13.0)).color(pal.text));
                });
            }
        }
    }
}

/// Tastierino della calcolatrice. Restituisce l'azione premuta.
/// In modalità scientifica sopra ai tasti base compaiono i tasti funzione;
/// con "2nd" attivo diventano le rispettive inverse.
fn keypad(ui: &mut Ui, pal: &Palette, scientific: bool, second: bool, degrees: bool, max_h: f32) -> Option<KeyAction> {
    use KeyAction::*;
    use KeyKind::*;
    type K = (KeyFace<'static>, KeyKind, KeyAction, usize);
    let t = KeyFace::Text;
    let w = ((ui.available_width() - 4.0 * 7.0) / 5.0).floor();
    // Altezze naturali dei tasti, ridotte se la finestra è bassa
    let (mut h_sci, mut h_base) = ((w * 0.5).clamp(26.0, 32.0), (w * 0.62).clamp(30.0, 42.0));
    let sci_rows = if scientific { 5.0 } else { 0.0 };
    let natural = sci_rows * h_sci + 5.0 * h_base + (sci_rows + 4.0) * 7.0;
    let k = (max_h / natural).clamp(0.7, 1.0);
    h_sci = (h_sci * k).round();
    h_base = (h_base * k).round();
    let gap = (7.0 * k).round();
    let w = ((ui.available_width() - 4.0 * gap) / 5.0).floor();
    let mut out = None;
    ui.spacing_mut().item_spacing = vec2(gap, gap);
    let mut grid = |ui: &mut Ui, rows: Vec<Vec<K>>, h: f32| {
        for row in rows {
            ui.horizontal(|ui| {
                for (face, kind, action, span) in row {
                    let size = vec2(w * span as f32 + gap * (span as f32 - 1.0), h);
                    if theme::calc_key(ui, pal, face, kind, size).clicked() {
                        out = Some(action);
                    }
                }
            });
        }
    };

    if scientific {
        // (etichetta, testo inserito, etichetta con 2nd, testo con 2nd)
        const SCI: [[(&str, &str, &str, &str); 5]; 5] = [
            [("2nd", "", "2nd", ""), ("x²", "^2", "√x", "√("), ("x³", "^3", "∛x", "cbrt("), ("xʸ", "^", "ʸ√x", "root("), ("eˣ", "exp(", "ln", "ln(")],
            [("10ˣ", "10^", "log₁₀", "log("), ("2ˣ", "2^", "log₂", "log2("), ("1/x", "1/(", "1/x", "1/("), ("x!", "!", "Γ", "gamma("), ("|x|", "abs(", "sgn", "sign(")],
            [("sin", "sin(", "sin⁻¹", "asin("), ("cos", "cos(", "cos⁻¹", "acos("), ("tan", "tan(", "tan⁻¹", "atan("), ("π", "π", "τ", "τ"), ("e", "e", "φ", "φ")],
            [("sinh", "sinh(", "sinh⁻¹", "asinh("), ("cosh", "cosh(", "cosh⁻¹", "acosh("), ("tanh", "tanh(", "tanh⁻¹", "atanh("), ("EE", "×10^", "EE", "×10^"), ("Rad", "", "Rad", "")],
            [("∫", "integral(", "∫", "integral("), ("d/dx", "diff(", "d/dx", "diff("), ("Σ", "sum(", "Π", "prod("), ("mod", "mod(", "nCr", "binom("), (",", ", ", ",", ", ")],
        ];
        let rows = SCI
            .iter()
            .map(|row| {
                row.iter()
                    .map(|&(l, ins, l2, ins2)| match l {
                        "2nd" => (t("2nd"), if second { Operator } else { Function }, Second, 1),
                        "Rad" => (t(if degrees { "Deg" } else { "Rad" }), Function, Degrees, 1),
                        _ if second => (t(l2), Function, Insert(ins2), 1),
                        _ => (t(l), Function, Insert(ins), 1),
                    })
                    .collect()
            })
            .collect();
        grid(ui, rows, h_sci);
    }

    let rows: Vec<Vec<K>> = vec![
        vec![(t("AC"), Top, Clear, 1), (KeyFace::Icon(Icon::Backspace), Top, Backspace, 1), (t("("), Top, Insert("("), 1), (t(")"), Top, Insert(")"), 1), (t("÷"), Operator, Insert("÷"), 1)],
        vec![(t("7"), Digit, Insert("7"), 1), (t("8"), Digit, Insert("8"), 1), (t("9"), Digit, Insert("9"), 1), (t("%"), Top, Insert("%"), 1), (t("×"), Operator, Insert("×"), 1)],
        vec![(t("4"), Digit, Insert("4"), 1), (t("5"), Digit, Insert("5"), 1), (t("6"), Digit, Insert("6"), 1), (t("x"), Function, Insert("x"), 1), (t("−"), Operator, Insert("−"), 1)],
        vec![(t("1"), Digit, Insert("1"), 1), (t("2"), Digit, Insert("2"), 1), (t("3"), Digit, Insert("3"), 1), (t("y"), Function, Insert("y"), 1), (t("+"), Operator, Insert("+"), 1)],
        vec![(t("0"), Digit, Insert("0"), 2), (t("."), Digit, Insert("."), 1), (t("="), Operator, Insert("="), 1), (KeyFace::Icon(Icon::Return), Operator, NewLine, 1)],
    ];
    grid(ui, rows, h_base);
    out
}

// ─────────────────────────────── Toolbar e grafico ───────────────────────────────

impl App {
    fn toolbar(&mut self, ui: &mut Ui, pal: &Palette) {
        let full = ui.max_rect();
        let bar = Rect::from_min_size(full.min, vec2(full.width(), HEADER_H));
        let mut bar_ui = ui.new_child(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(12.0, 0.0))));
        bar_ui.horizontal_centered(|ui| {
            {
                if theme::toolbar_button(ui, pal, Icon::Sidebar, false, "Mostra/nascondi barra laterale").clicked() {
                    self.show_sidebar = !self.show_sidebar;
                }
                if theme::toolbar_button(ui, pal, Icon::Plot, true, "Nascondi grafico").clicked() {
                    self.show_plot = false;
                    self.show_sidebar = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let dark_icon = if pal.dark { Icon::Sun } else { Icon::Moon };
                    if theme::toolbar_button(ui, pal, dark_icon, false, "Tema chiaro/scuro").clicked() {
                        self.dark = Some(!pal.dark);
                    }
                    if theme::toolbar_button(ui, pal, Icon::Help, self.show_help, "Guida").clicked() {
                        self.show_help = !self.show_help;
                    }
                    if theme::toolbar_button(ui, pal, Icon::Keypad, self.show_keypad, "Tastierino").clicked() {
                        self.show_keypad = !self.show_keypad;
                        self.show_sidebar |= self.show_keypad;
                    }
                    if theme::toolbar_button(ui, pal, Icon::Aspect, self.equal_aspect, "Assi in scala 1:1").clicked() {
                        self.equal_aspect = !self.equal_aspect;
                    }
                    ui.add_space(8.0);
                    let mut sel = usize::from(self.degrees);
                    if theme::segmented(ui, pal, &mut sel, &["Rad", "Gradi"]).changed() {
                        self.degrees = sel == 1;
                    }
                });
            }
        });
        ui.painter().hline(full.x_range(), bar.bottom(), Stroke::new(1.0, pal.separator.gamma_multiply(0.6)));
        ui.add_space(HEADER_H + 1.0);
    }

    fn plot_area(&mut self, ui: &mut Ui, pal: &Palette, nb: &mut Notebook, roots: &[Option<Vec<f64>>]) {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        let avail = Rect::from_min_max(pos2(full.left(), full.top() + HEADER_H + 1.0), full.max).shrink(12.0);
        let card = avail;
        ui.painter().rect(card, 10, pal.card, Stroke::new(1.0, pal.card_border), StrokeKind::Inside);
        let plot_rect = card.shrink(1.0);
        self.plot_width = plot_rect.width();

        let dark = pal.dark;
        let degrees = self.degrees;
        let global_key = self.hash_inputs();
        let equal = self.equal_aspect;
        let Self { entries, heat, pending_bounds, .. } = self;
        let pending = pending_bounds.take();
        let n_samples = (plot_rect.width() as usize * 3).clamp(600, 6000);
        let grid = pal.grid;
        let axis_col = pal.axis.gamma_multiply(0.7);

        let mut plot = EguiPlot::new("main-plot")
            .show_background(false)
            .auto_bounds(egui::Vec2b::FALSE)
            .allow_double_click_reset(false)
            .show_axes([true, true])
            .grid_color(grid)
            .cursor_color(pal.accent.gamma_multiply(0.35))
            .x_axis_formatter(|m, _| format_short(m.value))
            .y_axis_formatter(|m, _| format_short(m.value))
            .label_formatter(|pos| match pos {
                HoverPosition::NearDataPoint { plot_name, position, .. } if !plot_name.is_empty() => Some(format!(
                    "{plot_name}\n({}, {})",
                    format_short(position.x),
                    format_short(position.y)
                )),
                _ => None,
            });
        if equal {
            plot = plot.data_aspect(1.0);
        }

        let mut ui_plot = ui.new_child(egui::UiBuilder::new().max_rect(plot_rect));
        let response = plot.show(&mut ui_plot, |pu| {
            if let Some(v) = pending {
                pu.set_plot_bounds(PlotBounds::from_min_max([v.x0, v.y0], [v.x1, v.y1]));
            }
            let b = pu.plot_bounds();
            let v = View { x0: b.min()[0], x1: b.max()[0], y0: b.min()[1], y1: b.max()[1] };
            let ext = v.w() * 0.5;

            // Prima le mappe di colore, sotto alle curve
            for (i, row) in nb.rows.iter_mut().enumerate() {
                let e = &entries[i];
                let Plot::Field { f } = &mut row.plot else { continue };
                if !e.visible {
                    continue;
                }
                let cols = (plot_rect.width() / 3.0).clamp(40.0, 320.0) as usize;
                let rows = ((plot_rect.height() / plot_rect.width()) * cols as f32).clamp(30.0, 320.0) as usize;
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (global_key, e.id, cols, rows, dark).hash(&mut h);
                [v.x0, v.x1, v.y0, v.y1].map(f64::to_bits).hash(&mut h);
                let key = h.finish();
                let tex = match heat.get(&e.id) {
                    Some((k, t)) if *k == key => t.clone(),
                    _ => {
                        let (vals, lo, hi) = plotting::field_values(f, v, cols, rows);
                        let img = plotting::field_image(&vals, cols, rows, lo, hi, dark);
                        let t = pu.ctx().load_texture(format!("field-{}", e.id), img, egui::TextureOptions::LINEAR);
                        heat.insert(e.id, (key, t.clone()));
                        t
                    }
                };
                pu.image(PlotImage::new(
                    "",
                    tex.id(),
                    PlotPoint::new((v.x0 + v.x1) / 2.0, (v.y0 + v.y1) / 2.0),
                    [v.w() as f32, v.h() as f32],
                ));
            }

            pu.hline(HLine::new("", 0.0).color(axis_col).width(1.0));
            pu.vline(VLine::new("", 0.0).color(axis_col).width(1.0));

            for (i, row) in nb.rows.iter_mut().enumerate() {
                let e = &entries[i];
                if !e.visible {
                    continue;
                }
                let color = theme::series_color(e.color, dark);
                let name = e.text.clone();
                let lines = |segs: Vec<Vec<[f64; 2]>>, pu: &mut egui_plot::PlotUi| {
                    for (k, seg) in segs.into_iter().enumerate() {
                        pu.line(Line::new(name.clone(), PlotPoints::new(seg)).color(color).width(2.5).id((e.id, k)));
                    }
                };
                match &mut row.plot {
                    Plot::Function { f } => {
                        lines(plotting::sample_function(f, v.x0 - ext, v.x1 + ext, n_samples, (v.y0, v.y1)), pu);
                    }
                    Plot::FunctionOfY { g } => lines(plotting::sample_function_of_y(g, v, n_samples), pu),
                    Plot::Parametric { fx, fy } => {
                        lines(plotting::sample_parametric(fx, fy, e.range[0], e.range[1], 4000, v), pu);
                    }
                    Plot::Polar { r } => {
                        lines(plotting::sample_polar(r, e.range[0], e.range[1], degrees, 4000, v), pu);
                    }
                    Plot::Roots { horizontal, .. } => {
                        for (k, r) in roots[i].iter().flatten().enumerate() {
                            if *horizontal {
                                pu.hline(HLine::new(name.clone(), *r).color(color).width(2.0).id((e.id, k)));
                            } else {
                                pu.vline(VLine::new(name.clone(), *r).color(color).width(2.0).id((e.id, k)));
                            }
                        }
                    }
                    Plot::Point { x, y } => {
                        pu.points(Points::new(name.clone(), vec![[*x, *y]]).color(color).radius(5.0).filled(true));
                    }
                    Plot::Field { .. } | Plot::Implicit { .. } | Plot::None => {}
                }
            }
        });

        let transform = response.transform;
        let b = transform.bounds();
        let v = View { x0: b.min()[0], x1: b.max()[0], y0: b.min()[1], y1: b.max()[1] };
        if v.w().is_finite() && v.h().is_finite() && v.w() > 0.0 && v.h() > 0.0 {
            if self.view.is_none_or(|old| (old.x0, old.x1, old.y0, old.y1) != (v.x0, v.x1, v.y0, v.y1)) {
                // Le radici dipendono dall'area visibile: ricalcola al prossimo frame
                ctx.request_repaint();
            }
            self.view = Some(v);
        }

        // Curve implicite: marching squares disegnato direttamente sopra il grafico
        let painter = ui.painter_at(response.response.rect);
        let cols = (plot_rect.width() / 3.0).clamp(40.0, 500.0) as usize;
        let rows = (plot_rect.height() / 3.0).clamp(30.0, 500.0) as usize;
        for (i, row) in nb.rows.iter_mut().enumerate() {
            let e = &self.entries[i];
            let Plot::Implicit { f } = &mut row.plot else { continue };
            if !e.visible {
                continue;
            }
            let color = theme::series_color(e.color, dark);
            let stroke = Stroke::new(2.5, color);
            for [a, b] in plotting::implicit_segments(f, v, cols, rows) {
                let pa = transform.position_from_point(&PlotPoint::new(a[0], a[1]));
                let pb = transform.position_from_point(&PlotPoint::new(b[0], b[1]));
                painter.line_segment([pa, pb], stroke);
                painter.circle_filled(pa, 1.25, color);
            }
        }

        // Etichetta dei punti chiave delle radici
        for (i, row) in nb.rows.iter().enumerate() {
            let e = &self.entries[i];
            if !e.visible {
                continue;
            }
            if let (Plot::Roots { horizontal, .. }, Some(rs)) = (&row.plot, &roots[i]) {
                for (k, r) in rs.iter().take(12).enumerate() {
                    let (pt, align) = if *horizontal {
                        (PlotPoint::new(v.x0, *r), Align2::LEFT_BOTTOM)
                    } else {
                        (PlotPoint::new(*r, v.y1), Align2::LEFT_TOP)
                    };
                    // Sfalsate per non sovrapporsi quando le radici sono vicine
                    let stagger = (k % 3) as f32 * 24.0;
                    let off = if *horizontal { vec2(6.0 + stagger * 3.0, -4.0) } else { vec2(6.0, 6.0 + stagger) };
                    let pos = transform.position_from_point(&pt) + off;
                    pill_text(&painter, pal, pos, align, &format_short(*r), theme::series_color(e.color, dark));
                }
            }
        }

        // Coordinate del puntatore (e valore della mappa di colore)
        if let Some(hover) = response.response.hover_pos() {
            let p = transform.value_from_position(hover);
            let mut txt = format!("x  {}     y  {}", format_short(p.x), format_short(p.y));
            for (i, row) in nb.rows.iter_mut().enumerate() {
                if let (Plot::Field { f }, true) = (&mut row.plot, self.entries[i].visible) {
                    txt += &format!("     f  {}", format_short(f.eval2(p.x, p.y)));
                }
            }
            let pos = pos2(plot_rect.right() - 12.0, plot_rect.top() + 12.0);
            pill_text(&painter, pal, pos, Align2::RIGHT_TOP, &txt, pal.text2);
        }

        if response.response.double_clicked() {
            self.pending_bounds = Some(DEFAULT_VIEW);
        }
        self.zoom_controls(ui, pal, plot_rect, v);
    }

    fn zoom_controls(&mut self, ui: &mut Ui, pal: &Palette, plot_rect: Rect, v: View) {
        let size = vec2(34.0, 34.0 * 3.0);
        let rect = Rect::from_min_size(plot_rect.right_bottom() - size - vec2(12.0, 12.0), size);
        let painter = ui.painter();
        painter.add(egui::Shape::from(
            Shadow { offset: [0, 2], blur: 10, spread: 0, color: Color32::from_black_alpha(if pal.dark { 90 } else { 28 }) }
                .as_shape(rect, 9.0),
        ));
        painter.rect(rect, 9, pal.card, Stroke::new(1.0, pal.card_border), StrokeKind::Inside);
        let icons = [(Icon::Plus, "Ingrandisci"), (Icon::Minus, "Riduci"), (Icon::Fit, "Vista iniziale (o doppio clic)")];
        for (k, (icon, tip)) in icons.into_iter().enumerate() {
            let r = Rect::from_min_size(rect.min + vec2(0.0, 34.0 * k as f32), vec2(34.0, 34.0));
            let resp = ui.interact(r, Id::unique(("zoom", k)), Sense::click()).on_hover_text(tip);
            if resp.hovered() {
                ui.painter().rect_filled(r.shrink(3.0), 6, pal.hover);
            }
            theme::paint_icon(ui.painter(), icon, Rect::from_center_size(r.center(), vec2(16.0, 16.0)), pal.text2);
            if k > 0 {
                ui.painter().hline(r.x_range().shrink(8.0), r.top(), Stroke::new(1.0, pal.separator.gamma_multiply(0.5)));
            }
            if resp.clicked() {
                self.pending_bounds = Some(match k {
                    0 => zoom(v, 0.7),
                    1 => zoom(v, 1.0 / 0.7),
                    _ => DEFAULT_VIEW,
                });
                ui.ctx().request_repaint();
            }
        }
    }

    fn help_window(&mut self, ctx: &egui::Context, pal: &Palette) {
        let frame = egui::Frame::new()
            .fill(pal.card)
            .stroke(Stroke::new(1.0, pal.card_border))
            .corner_radius(12)
            .shadow(ctx.global_style().visuals.window_shadow)
            .inner_margin(Margin::same(16));
        egui::Window::new("Guida")
            .title_bar(false)
            .resizable(false)
            .collapsible(false)
            .anchor(Align2::RIGHT_TOP, vec2(-20.0, HEADER_H + 10.0))
            .frame(frame)
            .default_width(380.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Guida rapida").font(theme::medium(16.0)).color(pal.text));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if theme::toolbar_button(ui, pal, Icon::Close, false, "Chiudi").clicked() {
                            self.show_help = false;
                        }
                    });
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    let section = |ui: &mut Ui, title: &str, rows: &[(&str, &str)]| {
                        ui.add_space(8.0);
                        theme::section_header(ui, pal, title);
                        egui::Grid::new(title).num_columns(2).spacing([14.0, 5.0]).show(ui, |ui| {
                            for (a, b) in rows {
                                ui.label(RichText::new(*a).font(FontId::monospace(12.5)).color(pal.text));
                                ui.label(RichText::new(*b).font(FontId::proportional(12.5)).color(pal.text2));
                                ui.end_row();
                            }
                        });
                    };
                    section(ui, "SCRITTURA", &[
                        ("2x  3(x+1)  sin x", "moltiplicazione implicita"),
                        ("x^2  x²  √2  |x|  5!", "potenze, radici, fattoriale"),
                        ("pi  e  tau  phi  inf", "costanti"),
                    ]);
                    section(ui, "GRAFICI", &[
                        ("sin(x)   y = x²", "funzione di x"),
                        ("x = y²", "funzione di y"),
                        ("x² + y² = 9", "curva implicita"),
                        ("x·y   sin(x)cos(y)", "mappa di colore f(x, y)"),
                        ("(cos t, sin 2t)", "curva parametrica"),
                        ("r = 1 + cos θ", "curva polare"),
                        ("(1, 2)", "punto"),
                        ("x³ − 2x = 1", "soluzioni (righe verticali)"),
                    ]);
                    section(ui, "VARIABILI", &[
                        ("a = 3", "definisce una variabile"),
                        ("a = 5   a = a + 1", "la ridefinisce dalla riga dopo in giù"),
                        ("ans", "risultato della riga sopra"),
                        ("f(x) = x² + a", "definisce una funzione"),
                        ("a sin(k x)", "a e k sconosciute → slider"),
                        ("2 + 2 = 4", "verifica un'uguaglianza"),
                    ]);
                    section(ui, "FUNZIONI", FUNCTION_HELP);
                    section(ui, "TASTIERA", &[
                        ("Invio nella barra in basso", "calcola e aggiungi all'elenco"),
                        ("Invio su una riga", "nuova riga sotto"),
                        ("↑ ↓", "riga precedente/successiva"),
                        ("⌫ su riga vuota", "elimina la riga"),
                        ("doppio clic sul grafico", "vista iniziale"),
                    ]);
                });
            });
    }
}

fn zoom(v: View, k: f64) -> View {
    let (cx, cy) = ((v.x0 + v.x1) / 2.0, (v.y0 + v.y1) / 2.0);
    let (hw, hh) = (v.w() / 2.0 * k, v.h() / 2.0 * k);
    View { x0: cx - hw, x1: cx + hw, y0: cy - hh, y1: cy + hh }
}

fn pill_text(painter: &egui::Painter, pal: &Palette, pos: Pos2, align: Align2, text: &str, color: Color32) {
    let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(12.0), color);
    let rect = align.anchor_size(pos, galley.size() + vec2(14.0, 6.0));
    let bg = if pal.dark { Color32::from_rgba_unmultiplied(44, 44, 46, 230) } else { Color32::from_rgba_unmultiplied(255, 255, 255, 235) };
    painter.rect(rect, 7, bg, Stroke::new(1.0, pal.card_border), StrokeKind::Inside);
    painter.galley(rect.min + vec2(7.0, 3.0), galley, color);
}
