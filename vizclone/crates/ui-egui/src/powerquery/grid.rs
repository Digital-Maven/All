//! The preview grid: sticky headers with the column type, quality bar and value distribution,
//! virtual rows, column selection and menus, the Column From Examples column, and the column
//! profile panel.

use egui::{Align2, Color32, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use gridcraft_engine::query::examples::suggest;
use gridcraft_engine::query::steps::{field, text_list};
use gridcraft_engine::query::value::quote_text;
use gridcraft_engine::query::{Evaluator, NoHost, Table, Value};

use super::dialogs::Dialog;
use super::{Examples, add_step, ops};
use crate::SheetApp;
use crate::theme::{self, Tokens};

const ROW_H: f32 = 22.0;
const NUM_W: f32 = 48.0;
const TYPE_W: f32 = 44.0;

fn header_h(app: &SheetApp) -> (f32, f32, f32) {
    let v = app.powerquery.view;
    (30.0, if v.quality { 34.0 } else { 0.0 }, if v.distribution { 48.0 } else { 0.0 })
}

fn col_width(t: &Table, c: usize) -> f32 {
    // The header holds the type badge, the name and the filter button.
    let head = t.columns.get(c).map(|n| n.chars().count() as f32 * 7.8 + TYPE_W + 44.0).unwrap_or(120.0);
    let mut chars = 0;
    for r in t.rows.iter().take(50) {
        chars = chars.max(r.get(c).map(|v| v.display().chars().count().min(40)).unwrap_or(0));
    }
    head.max(chars as f32 * 7.4 + 18.0).clamp(150.0, 340.0)
}

pub fn show(app: &mut SheetApp, ui: &mut Ui, t: &Tokens) {
    let Some(p) = app.powerquery.preview(&app.session) else { return };
    if let Some(e) = &p.error {
        error_view(app, ui, t, e);
        return;
    }
    if let Some(v) = &p.scalar {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| ui.label(RichText::new(v).size(22.0)));
        return;
    }
    if app.powerquery.examples.is_some() {
        examples_bar(app, ui, t, &p.table);
    }
    let profile_col = app.powerquery.selected.first().and_then(|c| p.table.column_index(c)).filter(|_| app.powerquery.view.profile);
    if let Some(c) = profile_col {
        egui::Panel::bottom("pq_profile").resizable(true).default_size(200.0).frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(8)).show(
            ui,
            |ui| {
                profile_panel(app, ui, t, &p.table, c);
            },
        );
    }
    table_view(app, ui, t, &p.table);
}

fn error_view(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, e: &str) {
    ui.add_space(30.0);
    egui::Frame::NONE.fill(t.notice_warn_bg).stroke(Stroke::new(1.0, t.notice_warn_border)).inner_margin(14).corner_radius(6.0).show(ui, |ui| {
        ui.label(RichText::new("This step has an error").strong().color(t.warning_ink));
        ui.add_space(4.0);
        ui.label(e);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("Go to the previous step").clicked() {
                let steps = super::steps(app);
                let cur = app.powerquery.step.clone().or_else(|| steps.last().map(|s| s.name.clone()));
                if let Some(i) = steps.iter().position(|s| Some(&s.name) == cur.as_ref())
                    && i > 0
                {
                    app.powerquery.step = steps.get(i - 1).map(|s| s.name.clone());
                }
            }
            if ui.button("Edit in Advanced Editor").clicked() {
                app.powerquery.dialog = Some(Dialog::advanced(app));
            }
        });
    });
}

/// Colours of the quality bar: valid, error, empty.
fn quality_colors(t: &Tokens) -> (Color32, Color32, Color32) {
    (t.error_check_green, t.danger, t.text_disabled)
}

fn draw_quality(painter: &egui::Painter, t: &Tokens, rect: Rect, p: &gridcraft_engine::query::profile::ColumnProfile) {
    let (g, r, e) = quality_colors(t);
    let total = p.count.max(1) as f32;
    let bar = Rect::from_min_size(rect.min + vec2(6.0, 4.0), vec2(rect.width() - 12.0, 4.0));
    let wv = bar.width() * p.valid() as f32 / total;
    let we = bar.width() * p.errors as f32 / total;
    painter.rect_filled(Rect::from_min_size(bar.min, vec2(wv, 4.0)), 0.0, g);
    painter.rect_filled(Rect::from_min_size(bar.min + vec2(wv, 0.0), vec2(we, 4.0)), 0.0, r);
    painter.rect_filled(Rect::from_min_size(bar.min + vec2(wv + we, 0.0), vec2(bar.width() - wv - we, 4.0)), 0.0, e);
    let pct = |n: usize| format!("{:.0}%", n as f32 * 100.0 / total);
    let font = theme::ui_font(10.0);
    let y = rect.min.y + 15.0;
    painter.text(pos2(rect.min.x + 6.0, y), Align2::LEFT_CENTER, format!("● Valid {}", pct(p.valid())), font.clone(), g);
    painter.text(
        pos2(rect.min.x + 6.0, y + 12.0),
        Align2::LEFT_CENTER,
        format!("● Error {}   ● Empty {}", pct(p.errors), pct(p.empty)),
        font,
        t.text_dim,
    );
}

fn draw_distribution(painter: &egui::Painter, t: &Tokens, rect: Rect, p: &gridcraft_engine::query::profile::ColumnProfile) {
    let bars: Vec<usize> = p.top.iter().take(20).map(|(_, n)| *n).collect();
    let max = bars.iter().copied().max().unwrap_or(1).max(1) as f32;
    let area = Rect::from_min_max(rect.min + vec2(6.0, 4.0), pos2(rect.max.x - 6.0, rect.max.y - 16.0));
    let n = bars.len().max(1) as f32;
    let w = (area.width() / n).min(14.0);
    for (i, b) in bars.iter().enumerate() {
        let h = area.height() * *b as f32 / max;
        let r = Rect::from_min_size(pos2(area.min.x + i as f32 * w, area.max.y - h), vec2((w - 2.0).max(1.0), h));
        painter.rect_filled(r, 1.0, t.accent.gamma_multiply(0.75));
    }
    painter.text(
        pos2(rect.min.x + 6.0, rect.max.y - 7.0),
        Align2::LEFT_CENTER,
        format!("{} distinct, {} unique", p.distinct, p.unique),
        theme::ui_font(10.0),
        t.text_dim,
    );
}

fn cell_text(v: &Value, ws: bool) -> String {
    let s = v.display();
    if ws { s.replace(' ', "·").replace('\n', "¶") } else { s }
}

struct Clicks {
    header: Option<(usize, bool)>,
    rename: Option<String>,
    filter: Option<String>,
    type_menu: Option<(String, egui::Response)>,
    nav: Option<(usize, String)>,
}

fn table_view(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, table: &std::rc::Rc<Table>) {
    let (hh, qh, dh) = header_h(app);
    let head = hh + qh + dh;
    let widths: Vec<f32> = (0..table.columns.len()).map(|c| col_width(table, c)).collect();
    let ex_w = if app.powerquery.examples.is_some() { 200.0 } else { 0.0 };
    let total_w = NUM_W + widths.iter().sum::<f32>() + ex_w + 20.0;
    let n = table.rows.len();
    let total_h = head + n as f32 * ROW_H + 8.0;
    let mono = app.powerquery.view.monospace;
    let ws = app.powerquery.view.whitespace;
    let font = if mono { egui::FontId::monospace(12.0) } else { theme::ui_font(12.5) };
    let selected = app.powerquery.selected.clone();
    let profiles: Vec<_> = if qh + dh > 0.0 { (0..table.columns.len()).map(|c| app.powerquery.profile(table, c)).collect() } else { vec![] };
    let predicted = app.powerquery.examples.as_ref().map(|e| predictions(e, table));
    let mut clicks = Clicks { header: None, rename: None, filter: None, type_menu: None, nav: None };
    let mut col_menu: Option<String> = None;
    egui::ScrollArea::both().id_salt(("pq_grid", app.powerquery.query.as_str())).auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
        let (whole, _) = ui.allocate_exact_size(vec2(total_w.max(viewport.width()), total_h.max(viewport.height())), Sense::hover());
        let origin = whole.min;
        let painter = ui.painter().clone();
        let first = ((viewport.min.y - head) / ROW_H).floor().max(0.0) as usize;
        let last = (((viewport.max.y - head) / ROW_H).ceil().max(0.0) as usize + 1).min(n);
        for r in first..last {
            let y = origin.y + head + r as f32 * ROW_H;
            if r % 2 == 1 {
                painter.rect_filled(Rect::from_min_size(pos2(origin.x, y), vec2(total_w, ROW_H)), 0.0, t.header_bg.gamma_multiply(0.3));
            }
            painter.text(
                pos2(origin.x + viewport.min.x + NUM_W - 8.0, y + ROW_H / 2.0),
                Align2::RIGHT_CENTER,
                (r + 1).to_string(),
                theme::ui_font(11.0),
                t.text_dim,
            );
            let mut x = origin.x + NUM_W;
            for (c, w) in widths.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(x, y), vec2(*w, ROW_H));
                if x + w >= origin.x + viewport.min.x && x <= origin.x + viewport.max.x {
                    let name = table.columns.get(c).cloned().unwrap_or_default();
                    if selected.contains(&name) {
                        painter.rect_filled(cell, 0.0, t.sel_fill);
                    }
                    let v = table.cell(r, c);
                    let (text, color, right) = match v {
                        Value::Error(_) => ("Error".to_string(), t.danger, false),
                        Value::Null => ("null".to_string(), t.text_disabled, false),
                        Value::Number(_) => (cell_text(v, ws), t.cell_text, true),
                        Value::Table(_) | Value::List(_) | Value::Record(_) => (v.display(), t.accent, false),
                        _ => (cell_text(v, ws), t.cell_text, false),
                    };
                    let pos = if right { cell.right_center() - vec2(6.0, 0.0) } else { cell.left_center() + vec2(6.0, 0.0) };
                    painter.with_clip_rect(cell.intersect(ui.clip_rect())).text(
                        pos,
                        if right { Align2::RIGHT_CENTER } else { Align2::LEFT_CENTER },
                        text,
                        font.clone(),
                        color,
                    );
                    if matches!(v, Value::Table(_) | Value::List(_) | Value::Record(_) | Value::Error(_)) {
                        let resp = ui.interact(cell, ui.scope_id().with(("pq_cell", r, c)), Sense::click());
                        let tip = match v {
                            Value::Error(e) => e.to_string(),
                            _ => "Click to drill into this value".to_string(),
                        };
                        if resp.on_hover_text(tip).clicked() && !v.is_error() {
                            clicks.nav = Some((r, name));
                        }
                    }
                    painter.vline(cell.right(), cell.y_range(), Stroke::new(1.0, t.gridline));
                }
                x += w;
            }
            if let Some(pred) = &predicted {
                example_cell(app, ui, t, Rect::from_min_size(pos2(x, y), vec2(ex_w, ROW_H)), r, pred.get(r).cloned().unwrap_or_default());
            }
            painter.hline(origin.x..=origin.x + total_w, y + ROW_H, Stroke::new(1.0, t.gridline));
        }
        // Sticky header.
        let hy = origin.y + viewport.min.y;
        painter.rect_filled(Rect::from_min_size(pos2(origin.x, hy), vec2(total_w.max(viewport.width()), head)), 0.0, t.header_bg);
        let mut x = origin.x + NUM_W;
        for (c, w) in widths.iter().enumerate() {
            let Some(name) = table.columns.get(c).cloned() else { continue };
            let rect = Rect::from_min_size(pos2(x, hy), vec2(*w, hh));
            if selected.contains(&name) {
                painter.rect_filled(Rect::from_min_size(rect.min, vec2(*w, head)), 0.0, t.header_sel_bg);
            }
            let ty = table.types.get(c).copied().unwrap_or_default();
            let badge = Rect::from_min_size(rect.min + vec2(4.0, 5.0), vec2(TYPE_W, hh - 10.0));
            let badge_r =
                ui.interact(badge, ui.scope_id().with(("pq_type", c)), Sense::click()).on_hover_text(format!("{} (click to change)", ty.name()));
            painter.rect_stroke(badge, 3.0, Stroke::new(1.0, t.header_line), StrokeKind::Inside);
            painter.text(badge.center(), Align2::CENTER_CENTER, ty.label(), theme::ui_font(9.5), t.header_text);
            let drop = Rect::from_min_size(pos2(rect.max.x - 22.0, rect.min.y + 5.0), vec2(18.0, hh - 10.0));
            let drop_r = ui.interact(drop, ui.scope_id().with(("pq_drop", c)), Sense::click()).on_hover_text("Sort and filter");
            painter.text(drop.center(), Align2::CENTER_CENTER, "▼", theme::ui_font(9.0), if drop_r.hovered() { t.accent } else { t.text_dim });
            let name_rect = Rect::from_min_max(pos2(badge.max.x + 6.0, rect.min.y), pos2(drop.min.x - 2.0, rect.max.y));
            painter.with_clip_rect(name_rect.intersect(ui.clip_rect())).text(
                name_rect.left_center(),
                Align2::LEFT_CENTER,
                &name,
                theme::ui_bold(12.5),
                t.header_text,
            );
            let name_r = ui.interact(name_rect, ui.scope_id().with(("pq_head", c)), Sense::click());
            if name_r.clicked() {
                clicks.header = Some((c, ui.input(|i| i.modifiers.command || i.modifiers.shift)));
            }
            if name_r.double_clicked() {
                clicks.rename = Some(name.clone());
            }
            if name_r.secondary_clicked() && !selected.contains(&name) {
                clicks.header = Some((c, false));
            }
            name_r.context_menu(|ui| {
                col_menu = Some(name.clone());
                ops::column_menu(app, ui, &name);
            });
            if drop_r.clicked() {
                clicks.filter = Some(name.clone());
            }
            if badge_r.clicked() {
                clicks.type_menu = Some((name.clone(), badge_r.clone()));
            }
            egui::Popup::menu(&badge_r).show(|ui| {
                for ty in gridcraft_engine::query::MType::COLUMN_TYPES {
                    if ui.button(ty.name()).clicked() {
                        ops::change_type(app, std::slice::from_ref(&name), ty);
                        ui.close();
                    }
                }
            });
            if let Some(p) = profiles.get(c) {
                let clip = painter.with_clip_rect(Rect::from_min_size(pos2(x, hy), vec2(*w, head)).intersect(ui.clip_rect()));
                if qh > 0.0 {
                    draw_quality(&clip, t, Rect::from_min_size(pos2(x, hy + hh), vec2(*w, qh)), p);
                }
                if dh > 0.0 {
                    draw_distribution(&clip, t, Rect::from_min_size(pos2(x, hy + hh + qh), vec2(*w, dh)), p);
                }
            }
            painter.vline(x + w, hy..=hy + head, Stroke::new(1.0, t.header_line));
            x += w;
        }
        if let Some(ex) = &mut app.powerquery.examples {
            let rect = Rect::from_min_size(pos2(x, hy), vec2(ex_w, head));
            painter.rect_filled(rect, 0.0, t.accent_soft);
            let r = ui.put(
                Rect::from_min_size(rect.min + vec2(6.0, 4.0), vec2(ex_w - 12.0, 22.0)),
                egui::TextEdit::singleline(&mut ex.name).hint_text("Column name"),
            );
            super::labeled(&r, "New column name");
            painter.text(rect.min + vec2(8.0, 40.0), Align2::LEFT_TOP, "Type examples below", theme::ui_font(11.0), t.text_dim);
        }
        painter.rect_filled(Rect::from_min_size(pos2(origin.x + viewport.min.x, hy), vec2(NUM_W, head)), 0.0, t.header_all_bg);
        painter.hline(origin.x..=origin.x + total_w, hy + head, Stroke::new(1.0, t.header_line));
    });
    apply_clicks(app, clicks, table);
    let _ = col_menu;
}

fn apply_clicks(app: &mut SheetApp, c: Clicks, table: &Table) {
    if let Some((col, add)) = c.header {
        let name = table.columns.get(col).cloned().unwrap_or_default();
        if add {
            if let Some(i) = app.powerquery.selected.iter().position(|x| *x == name) {
                app.powerquery.selected.remove(i);
            } else {
                app.powerquery.selected.push(name);
            }
        } else {
            app.powerquery.selected = vec![name];
        }
    }
    if let Some(n) = c.rename {
        app.powerquery.dialog = Some(Dialog::rename_column(&n));
    }
    if let Some(n) = c.filter {
        app.powerquery.dialog = Some(Dialog::filter(table, &n));
    }
    if let Some((r, col)) = c.nav {
        add_step(app, &col, &format!("{{prev}}{{{r}}}{}", field(&col)));
    }
    let _ = c.type_menu;
}

// ---------------------------------------------------------------- Column From Examples

fn input_columns(ex: &Examples, t: &Table) -> Vec<usize> {
    ex.inputs.iter().filter_map(|c| t.column_index(c)).collect()
}

/// Recomputes the suggestions when the examples change.
fn refresh_suggestions(ex: &mut Examples, t: &Table) {
    let mut cur: Vec<(usize, String)> = ex.values.iter().filter(|(_, v)| !v.trim().is_empty()).map(|(r, v)| (*r, v.trim().to_string())).collect();
    cur.sort();
    if cur != ex.computed_for {
        ex.suggestions = suggest(t, &input_columns(ex, t), &cur);
        ex.chosen = 0;
        ex.computed_for = cur;
    }
}

/// The chosen formula's value for each row (as text).
fn predictions(ex: &Examples, t: &Table) -> Vec<String> {
    let Some(s) = ex.suggestions.get(ex.chosen) else { return vec![] };
    let host = NoHost;
    let ev = Evaluator::new(&host);
    let Ok(e) = gridcraft_engine::query::m::parse(&format!("each {}", s.expr)) else { return vec![] };
    let Ok(f) = ev.eval(&e, &Default::default()) else { return vec![] };
    (0..t.rows.len().min(1000))
        .map(|r| ev.apply1(&f, Value::record(t.row_record(r))).map(|v| v.display()).unwrap_or_else(|_| "Error".into()))
        .collect()
}

fn example_cell(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, rect: Rect, r: usize, predicted: String) {
    let Some(ex) = app.powerquery.examples.as_mut() else { return };
    ui.painter().rect_filled(rect, 0.0, t.accent_soft.gamma_multiply(0.4));
    let buf = ex.values.entry(r).or_default();
    let typed = !buf.is_empty();
    let edit = egui::TextEdit::singleline(buf).hint_text(RichText::new(predicted).color(t.text_dim)).frame(egui::Frame::NONE).text_color(if typed {
        t.text
    } else {
        t.text_dim
    });
    let resp = ui.put(rect.shrink2(vec2(4.0, 1.0)), edit);
    super::labeled(&resp, &format!("Example for row {}", r + 1));
    if resp.changed() {
        ex.values.retain(|_, v| !v.is_empty());
    }
}

fn examples_bar(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, table: &Table) {
    if let Some(ex) = app.powerquery.examples.as_mut() {
        refresh_suggestions(ex, table);
    }
    let mut ok = false;
    let mut cancel = false;
    egui::Frame::NONE.fill(t.accent_soft).inner_margin(10).show(ui, |ui| {
        let Some(ex) = app.powerquery.examples.as_mut() else { return };
        ui.horizontal(|ui| {
            ui.label(RichText::new("Add Column From Examples").strong());
            ui.label(
                RichText::new(if ex.inputs.is_empty() { "from all columns".to_string() } else { format!("from {}", ex.inputs.join(", ")) })
                    .color(t.text_dim),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                cancel = ui.button("Cancel").clicked();
                ok = ui.add_enabled(!ex.suggestions.is_empty(), egui::Button::new(RichText::new("OK").strong())).clicked();
            });
        });
        ui.label("Enter sample values in the new column on the right; a formula that produces them appears here.");
        match ex.suggestions.get(ex.chosen) {
            Some(s) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Transform:").strong());
                    ui.label(RichText::new(&s.expr).monospace());
                });
                if ex.suggestions.len() > 1 {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Other formulas:").small().color(t.text_dim));
                        for (i, alt) in ex.suggestions.iter().enumerate() {
                            if i != ex.chosen && ui.small_button(alt.expr.chars().take(40).collect::<String>()).on_hover_text(&alt.expr).clicked() {
                                ex.chosen = i;
                            }
                        }
                    });
                }
            }
            None if ex.values.values().any(|v| !v.trim().is_empty()) => {
                ui.colored_label(t.danger, "No formula reproduces these examples yet. Add another example, or check the values.");
            }
            None => {}
        }
    });
    if cancel {
        app.powerquery.examples = None;
    }
    if ok
        && let Some(ex) = app.powerquery.examples.take()
        && let Some(s) = ex.suggestions.get(ex.chosen)
    {
        let name = if ex.name.trim().is_empty() { "Custom".to_string() } else { ex.name.trim().to_string() };
        let ty = if s.ty == gridcraft_engine::query::MType::Number { "type number" } else { "type text" };
        add_step(app, &format!("Inserted {name}"), &format!("Table.AddColumn({{prev}}, {}, each {}, {ty})", quote_text(&name), s.expr));
    }
}

// ---------------------------------------------------------------- column profile

fn profile_panel(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, table: &Table, c: usize) {
    let p = app.powerquery.profile(table, c);
    let name = table.columns.get(c).cloned().unwrap_or_default();
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(260.0);
            ui.label(RichText::new(format!("Column statistics — {name}")).strong());
            egui::Grid::new("pq_stats").num_columns(2).spacing(vec2(16.0, 2.0)).show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.label(RichText::new(k).color(t.text_dim));
                    ui.label(v);
                    ui.end_row();
                };
                row("Count", p.count.to_string());
                row("Error", p.errors.to_string());
                row("Empty", p.empty.to_string());
                row("Distinct", p.distinct.to_string());
                row("Unique", p.unique.to_string());
                if let Some(m) = &p.min {
                    row("Min", m.display());
                }
                if let Some(m) = &p.max {
                    row("Max", m.display());
                }
                if let Some(a) = p.average {
                    row("Average", format!("{a:.4}"));
                }
                if let Some(s) = p.std_dev {
                    row("Standard deviation", format!("{s:.4}"));
                }
                if let Some(s) = p.sum {
                    row("Sum", gridcraft_engine::query::value::number_text(s));
                }
                if p.min_length.is_some() {
                    row("Min length", p.min_length.unwrap_or(0).to_string());
                    row("Max length", p.max_length.unwrap_or(0).to_string());
                }
                if p.sum.is_some() {
                    row("Zero", p.zeros.to_string());
                }
            });
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.label(RichText::new("Value distribution").strong());
            let max = p.top.first().map(|x| x.1).unwrap_or(1).max(1) as f32;
            egui::ScrollArea::vertical().id_salt("pq_dist").show(ui, |ui| {
                for (v, n) in p.top.iter().take(20) {
                    ui.horizontal(|ui| {
                        ui.add_sized(vec2(160.0, 16.0), egui::Label::new(RichText::new(v).small()).truncate());
                        let (r, resp) = ui.allocate_exact_size(vec2(260.0, 14.0), Sense::click());
                        ui.painter().rect_filled(
                            Rect::from_min_size(r.min, vec2(r.width() * *n as f32 / max, r.height())),
                            2.0,
                            t.accent.gamma_multiply(0.7),
                        );
                        ui.label(RichText::new(n.to_string()).small().color(t.text_dim));
                        if resp.on_hover_text("Keep only this value").clicked() {
                            let lit = table
                                .rows
                                .iter()
                                .filter_map(|row| row.get(c))
                                .find(|x| x.display() == *v)
                                .map(Value::m_literal)
                                .unwrap_or_else(|| quote_text(v));
                            add_step(app, "Filtered Rows", &format!("Table.SelectRows({{prev}}, each {} = {lit})", field(&name)));
                        }
                    });
                }
            });
        });
    });
    let _ = text_list;
}
