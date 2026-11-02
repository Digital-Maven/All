//! Data View: the current table's rows (with a sticky header row), the formula bar above it, the
//! calculation area of measures below it and one tab per model table at the bottom.

use egui::{Align2, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use gridcraft_engine::datamodel::store::{DataType, Table};
use gridcraft_engine::datamodel::value::sort_order;
use gridcraft_engine::datamodel::{Model, Value};
use serde_json::json;

use super::{Form, Selected, display_value, run, toast};
use crate::SheetApp;
use crate::theme::{self, Tokens};

const ROW_H: f32 = 22.0;
const HEAD_H: f32 = 30.0;
const ROWNUM_W: f32 = 56.0;

pub(super) fn show(app: &mut SheetApp, ui: &mut Ui, m: &Model) {
    let t = Tokens::get(ui.ctx());
    if m.table(&app.powerpivot.table).is_none() {
        app.powerpivot.table = m.tables.first().map(|t| t.name.clone()).unwrap_or_default();
        app.powerpivot.selected = Selected::Nothing;
    }
    let Some(table) = m.table(&app.powerpivot.table) else { return };
    egui::Panel::bottom("pp_table_tabs").exact_size(30.0).frame(egui::Frame::NONE.fill(t.tab_bar).inner_margin(egui::Margin::symmetric(8, 3))).show(
        ui,
        |ui| {
            table_tabs(app, ui, &t, m);
        },
    );
    if app.powerpivot.calc_area {
        egui::Panel::bottom("pp_calc_area")
            .resizable(true)
            .default_size(130.0)
            .min_size(60.0)
            .frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(8))
            .show(ui, |ui| {
                calc_area(app, ui, &t, m, table);
            });
    }
    egui::Panel::top("pp_formula_bar").exact_size(34.0).frame(egui::Frame::NONE.fill(t.window).inner_margin(egui::Margin::symmetric(8, 4))).show(
        ui,
        |ui| {
            formula_bar(app, ui, &t, m, table);
        },
    );
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.grid_bg)).show(ui, |ui| {
        grid(app, ui, &t, table);
    });
}

fn table_tabs(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, m: &Model) {
    egui::ScrollArea::horizontal().id_salt("pp_tabs_scroll").show(ui, |ui| {
        ui.horizontal(|ui| {
            for tb in &m.tables {
                let active = tb.name == app.powerpivot.table;
                let text = if active { RichText::new(&tb.name).strong().color(t.accent) } else { RichText::new(&tb.name).color(t.text) };
                let r = ui.add(egui::Button::new(text).fill(if active { t.tab_active } else { t.tab_bar }).min_size(vec2(0.0, 24.0)));
                if r.clicked() && !active {
                    app.powerpivot.table = tb.name.clone();
                    app.powerpivot.selected = Selected::Nothing;
                }
                r.context_menu(|ui| {
                    if ui.button("New Measure…").clicked() {
                        app.powerpivot.form = Some(Form::new_measure(&tb.name));
                        ui.close();
                    }
                    if ui.button("Add Calculated Column…").clicked() {
                        app.powerpivot.form = Some(Form::new_column(&tb.name));
                        ui.close();
                    }
                    if ui.button("Remove from Data Model").clicked() {
                        run(app, "powerpivot.removeTable", json!({ "table": tb.name }));
                        ui.close();
                    }
                });
            }
        });
    });
}

/// The text the formula bar shows for the selection.
fn formula_text(m: &Model, table: &Table, sel: &Selected) -> String {
    match sel {
        Selected::Measure(n) => m.measure(n).map(|ms| format!("{} := {}", ms.name, ms.expression)).unwrap_or_default(),
        Selected::Column(c) => table.column(*c).and_then(|c| c.expression.clone()).map(|e| format!("= {e}")).unwrap_or_default(),
        Selected::Nothing => String::new(),
    }
}

fn formula_bar(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, m: &Model, table: &Table) {
    let sel = app.powerpivot.selected.clone();
    let key = Some((table.name.clone(), sel.clone()));
    if app.powerpivot.formula_for != key {
        app.powerpivot.formula = formula_text(m, table, &sel);
        app.powerpivot.formula_for = key;
    }
    let (label, hint, editable) = match &sel {
        Selected::Measure(n) => (n.clone(), String::new(), true),
        Selected::Column(c) => {
            let col = table.column(*c);
            let name = col.map(|c| c.name.clone()).unwrap_or_default();
            if col.is_some_and(|c| c.expression.is_some()) {
                (name, String::new(), true)
            } else {
                (name, format!("Imported from the worksheet table {}", table.name), false)
            }
        }
        Selected::Nothing => (table.name.clone(), "Type  Name := formula  and press Enter to add a measure to this table".to_string(), true),
    };
    ui.horizontal(|ui| {
        ui.add_sized(vec2(150.0, 24.0), egui::Label::new(RichText::new(label).color(t.text_dim)).truncate());
        if ui.add(egui::Button::new(RichText::new("fx").italics()).min_size(vec2(28.0, 24.0))).on_hover_text("Open the formula editor").clicked() {
            app.powerpivot.form = Some(match &sel {
                Selected::Measure(n) => Form::edit_measure(m, n),
                Selected::Column(c) => match table.column(*c).filter(|c| c.expression.is_some()) {
                    Some(col) => Form::edit_column(&table.name, &col.name, col.expression.as_deref().unwrap_or_default()),
                    None => Form::new_column(&table.name),
                },
                Selected::Nothing => Form::new_measure(&table.name),
            });
        }
        let width = (ui.available_width() - 70.0).max(100.0);
        let r = ui.add_enabled(
            editable,
            egui::TextEdit::singleline(&mut app.powerpivot.formula).desired_width(width).hint_text(hint).font(egui::FontId::monospace(13.0)),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = ui.add_enabled(editable, egui::Button::new("✓")).on_hover_text("Apply").clicked();
        let cancel = ui.add_enabled(editable, egui::Button::new("×")).on_hover_text("Cancel").clicked();
        if cancel {
            app.powerpivot.formula_for = None;
        }
        if editable && (enter || ok) {
            commit(app, m, table, &sel);
        }
    });
}

fn commit(app: &mut SheetApp, m: &Model, table: &Table, sel: &Selected) {
    let text = app.powerpivot.formula.trim().to_string();
    match sel {
        Selected::Measure(old) => {
            let (name, expr) =
                text.split_once(":=").map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or((old.clone(), text.clone()));
            if run(app, "powerpivot.editMeasure", json!({ "name": old, "newName": name, "expression": expr })).is_some() {
                app.powerpivot.selected = Selected::Measure(name);
                app.powerpivot.formula_for = None;
            }
        }
        Selected::Column(c) => {
            let Some(col) = table.column(*c) else { return };
            let expr = text.strip_prefix('=').unwrap_or(&text).trim().to_string();
            if run(app, "powerpivot.editColumn", json!({ "table": table.name, "name": col.name, "expression": expr })).is_some() {
                app.powerpivot.formula_for = None;
            }
        }
        Selected::Nothing => {
            let Some((name, expr)) = text.split_once(":=") else {
                toast(app, "Write the measure as  Name := formula,  for example  Total Sales := SUM(Sales[Amount])");
                return;
            };
            let name = name.trim().to_string();
            if m.measure(&name).is_some() {
                toast(app, format!("A measure named {name} already exists"));
                return;
            }
            if run(app, "powerpivot.newMeasure", json!({ "table": table.name, "name": name, "expression": expr.trim() })).is_some() {
                app.powerpivot.selected = Selected::Measure(name);
                app.powerpivot.formula_for = None;
                app.powerpivot.calc_area = true;
            }
        }
    }
}

/// How a numeric column is shown, from its name: dates as dates, years and codes without
/// thousands separators, other numbers grouped.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Plain,
    Date,
    Code,
}

fn kind(name: &str) -> Kind {
    let n = name.to_lowercase();
    if n.contains("date") || n == "day_key" {
        Kind::Date
    } else if n.contains("year")
        || n.contains("month")
        || n.contains("week")
        || n.ends_with("id")
        || n.ends_with("number")
        || n.ends_with("no")
        || n.contains("code")
    {
        Kind::Code
    } else {
        Kind::Plain
    }
}

fn cell_text(k: Kind, v: &Value) -> String {
    let Value::Number(n) = v else { return v.to_text() };
    match k {
        Kind::Date if (1.0..=2_958_465.0).contains(n) => {
            gridcraft_engine::datamodel::dates::to_date(*n).map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| v.to_text())
        }
        Kind::Code | Kind::Date => v.to_text(),
        Kind::Plain => display_value(v, None),
    }
}

fn type_tag(dt: DataType) -> &'static str {
    match dt {
        DataType::Number => "123",
        DataType::Text => "abc",
        DataType::Boolean => "t/f",
        DataType::Mixed => "mix",
        DataType::Empty => "—",
    }
}

fn col_width(table: &Table, c: usize) -> f32 {
    let Some(col) = table.column(c) else { return 100.0 };
    let mut chars = col.name.chars().count() + 5;
    for r in 0..table.row_count().min(60) {
        chars = chars.max(col.value(r).to_text().chars().count().min(40));
    }
    (chars as f32 * 7.4 + 16.0).clamp(70.0, 280.0)
}

/// The row order: as loaded, or sorted by the chosen column (cached per workbook version).
fn row_order(app: &mut SheetApp, table: &Table) -> Vec<usize> {
    let n = table.row_count();
    let Some((tn, c, desc)) = app.powerpivot.sort.clone().filter(|(tn, ..)| *tn == table.name) else { return (0..n).collect() };
    let Some(col) = table.column(c) else { return (0..n).collect() };
    let _ = tn;
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|a, b| {
        let o = sort_order(col.value(*a), col.value(*b));
        if desc { o.reverse() } else { o }
    });
    idx
}

fn grid(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, table: &Table) {
    let widths: Vec<f32> = (0..table.columns.len()).map(|c| col_width(table, c)).collect();
    let kinds: Vec<Kind> = table.columns.iter().map(|c| kind(&c.name)).collect();
    let total_w = ROWNUM_W + widths.iter().sum::<f32>() + 40.0;
    let n = table.row_count();
    let total_h = HEAD_H + n as f32 * ROW_H + 8.0;
    let order = row_order(app, table);
    let sel_col = match app.powerpivot.selected {
        Selected::Column(c) => Some(c),
        _ => None,
    };
    let font = theme::ui_font(12.5);
    let mut clicked: Option<usize> = None;
    let mut sort_req: Option<Option<(usize, bool)>> = None;
    let mut add_column = false;
    egui::ScrollArea::both().id_salt(("pp_grid", table.name.as_str())).auto_shrink([false, false]).show_viewport(ui, |ui, viewport| {
        let (whole, _) = ui.allocate_exact_size(vec2(total_w.max(viewport.width()), total_h.max(viewport.height())), Sense::hover());
        let origin = whole.min;
        let painter = ui.painter().clone();
        let first = ((viewport.min.y - HEAD_H) / ROW_H).floor().max(0.0) as usize;
        let last = (((viewport.max.y - HEAD_H) / ROW_H).ceil().max(0.0) as usize + 1).min(n);
        // Rows.
        for vis in first..last {
            let Some(&r) = order.get(vis) else { continue };
            let y = origin.y + HEAD_H + vis as f32 * ROW_H;
            if vis % 2 == 1 {
                painter.rect_filled(Rect::from_min_size(pos2(origin.x, y), vec2(total_w, ROW_H)), 0.0, t.header_bg.gamma_multiply(0.35));
            }
            let num_rect = Rect::from_min_size(pos2(origin.x + viewport.min.x, y), vec2(ROWNUM_W, ROW_H));
            painter.text(num_rect.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, (r + 1).to_string(), font.clone(), t.text_dim);
            let mut x = origin.x + ROWNUM_W;
            for (c, w) in widths.iter().enumerate() {
                let cell = Rect::from_min_size(pos2(x, y), vec2(*w, ROW_H));
                if x + w >= origin.x + viewport.min.x && x <= origin.x + viewport.max.x {
                    if sel_col == Some(c) {
                        painter.rect_filled(cell, 0.0, t.sel_fill);
                    }
                    let v = table.column(c).map(|col| col.value(r)).cloned().unwrap_or_default();
                    let (text, align) = match &v {
                        Value::Number(_) => (cell_text(kinds.get(c).copied().unwrap_or(Kind::Plain), &v), Align2::RIGHT_CENTER),
                        _ => (v.to_text(), Align2::LEFT_CENTER),
                    };
                    let p = if align == Align2::RIGHT_CENTER { cell.right_center() - vec2(6.0, 0.0) } else { cell.left_center() + vec2(6.0, 0.0) };
                    painter.with_clip_rect(cell.intersect(ui.clip_rect())).text(p, align, text, font.clone(), t.cell_text);
                    painter.vline(cell.right(), cell.y_range(), Stroke::new(1.0, t.gridline));
                }
                x += w;
            }
            painter.hline(origin.x..=origin.x + total_w, y + ROW_H, Stroke::new(1.0, t.gridline));
        }
        // Sticky header row.
        let hy = origin.y + viewport.min.y;
        painter.rect_filled(Rect::from_min_size(pos2(origin.x, hy), vec2(total_w.max(viewport.width()), HEAD_H)), 0.0, t.header_bg);
        let mut x = origin.x + ROWNUM_W;
        for (c, w) in widths.iter().enumerate() {
            let Some(col) = table.column(c) else { continue };
            let rect = Rect::from_min_size(pos2(x, hy), vec2(*w, HEAD_H));
            let resp = ui.interact(rect, ui.scope_id().with(("pp_head", c)), Sense::click());
            let calc = col.expression.is_some();
            if sel_col == Some(c) {
                painter.rect_filled(rect, 0.0, t.header_sel_bg);
            } else if calc {
                painter.rect_filled(rect, 0.0, t.accent_soft);
            } else if resp.hovered() {
                painter.rect_filled(rect, 0.0, t.hover);
            }
            let sort_mark = match &app.powerpivot.sort {
                Some((tn, sc, desc)) if *tn == table.name && *sc == c => {
                    if *desc {
                        " ▼"
                    } else {
                        " ▲"
                    }
                }
                _ => "",
            };
            let label = format!("{}{}", col.name, sort_mark);
            let clip = painter.with_clip_rect(rect.intersect(ui.clip_rect()));
            clip.text(rect.left_center() + vec2(6.0, -5.0), Align2::LEFT_CENTER, label, theme::ui_bold(12.5), t.header_text);
            let tag = if calc { "fx (calculated)".to_string() } else { format!("{} · {} distinct", type_tag(col.data_type()), col.cardinality()) };
            clip.text(rect.left_center() + vec2(6.0, 8.0), Align2::LEFT_CENTER, tag, theme::ui_font(10.0), t.text_dim);
            painter.vline(rect.right(), rect.y_range(), Stroke::new(1.0, t.header_line));
            if resp.clicked() {
                clicked = Some(c);
            }
            let tip = if calc {
                format!("Calculated column: = {}", col.expression.clone().unwrap_or_default())
            } else {
                format!("{} values, {} distinct", n, col.cardinality())
            };
            let resp = resp.on_hover_text(tip);
            resp.context_menu(|ui| {
                if ui.button("Sort Ascending").clicked() {
                    sort_req = Some(Some((c, false)));
                    ui.close();
                }
                if ui.button("Sort Descending").clicked() {
                    sort_req = Some(Some((c, true)));
                    ui.close();
                }
                if ui.button("Clear Sort").clicked() {
                    sort_req = Some(None);
                    ui.close();
                }
            });
            x += w;
        }
        // "Add Column" header, as the last column.
        let add = Rect::from_min_size(pos2(x, hy), vec2(40.0, HEAD_H));
        let r = ui.interact(add, ui.scope_id().with("pp_add_col"), Sense::click()).on_hover_text("Add a calculated column");
        painter.text(add.center(), Align2::CENTER_CENTER, "+", theme::ui_bold(14.0), if r.hovered() { t.accent } else { t.text_dim });
        if r.clicked() {
            add_column = true;
        }
        let corner = Rect::from_min_size(pos2(origin.x + viewport.min.x, hy), vec2(ROWNUM_W, HEAD_H));
        painter.rect_filled(corner, 0.0, t.header_all_bg);
        painter.rect_stroke(Rect::from_min_size(pos2(origin.x, hy), vec2(total_w, HEAD_H)), 0.0, Stroke::new(1.0, t.header_line), StrokeKind::Inside);
        let _ = Pos2::ZERO;
    });
    if let Some(c) = clicked {
        app.powerpivot.selected = if sel_col == Some(c) { Selected::Nothing } else { Selected::Column(c) };
    }
    if let Some(req) = sort_req {
        app.powerpivot.sort = req.map(|(c, d)| (table.name.clone(), c, d));
    }
    if add_column {
        app.powerpivot.form = Some(Form::new_column(&table.name));
    }
}

fn calc_area(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, m: &Model, table: &Table) {
    let ti = m.table_index(&table.name);
    let measures: Vec<(String, Option<String>)> =
        m.measures.iter().filter(|ms| Some(ms.table) == ti).map(|ms| (ms.name.clone(), ms.format.clone())).collect();
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Measures in {}", table.name)).strong());
        ui.label(RichText::new("click to select · double-click to edit · right-click for more").small().color(t.text_dim));
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical().id_salt("pp_calc_scroll").auto_shrink([false, false]).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            for (name, _) in &measures {
                let value = app.powerpivot.measure_value(m, name);
                let selected = app.powerpivot.selected == Selected::Measure(name.clone());
                let text = RichText::new(format!("{name}:  {value}")).color(if selected { t.on_accent } else { t.text });
                let r = ui.add(
                    egui::Button::new(text)
                        .fill(if selected { t.accent } else { t.input_bg })
                        .stroke(Stroke::new(1.0, t.input_border))
                        .min_size(vec2(160.0, 28.0)),
                );
                let expr = m.measure(name).map(|ms| ms.expression.clone()).unwrap_or_default();
                let r = r.on_hover_text(format!("{name} := {expr}"));
                if r.clicked() {
                    app.powerpivot.selected = Selected::Measure(name.clone());
                }
                if r.double_clicked() {
                    app.powerpivot.form = Some(Form::edit_measure(m, name));
                }
                r.context_menu(|ui| {
                    if ui.button("Edit Measure…").clicked() {
                        app.powerpivot.form = Some(Form::edit_measure(m, name));
                        ui.close();
                    }
                    if ui.button("Delete Measure").clicked() {
                        if run(app, "powerpivot.deleteMeasure", json!({ "name": name })).is_some() {
                            app.powerpivot.selected = Selected::Nothing;
                        }
                        ui.close();
                    }
                });
            }
            if ui.add(egui::Button::new(RichText::new("+ New Measure").color(t.accent)).min_size(vec2(130.0, 28.0))).clicked() {
                app.powerpivot.form = Some(Form::new_measure(&table.name));
            }
        });
        if measures.is_empty() {
            ui.label(
                RichText::new(
                    "No measures in this table yet. A measure is a DAX formula such as SUM(Table[Column]) that PivotTables evaluate for each cell.",
                )
                .small()
                .color(t.text_dim),
            );
        }
    });
}
