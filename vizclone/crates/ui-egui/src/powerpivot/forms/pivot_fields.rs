//! The PivotTable from Data Model builder, laid out like a spreadsheet's PivotTable Fields pane:
//! the live report on the left; on the right the field list (a checkbox per column and measure,
//! grouped by table) over the four areas (Filters, Columns, Rows, Values).
//!
//! Ticking a field places it where it usually belongs (numbers and measures in Values, everything
//! else in Rows); unticking removes it. Fields drag from the list into any area, and the chips in
//! the areas drag between areas and reorder within one. Right-click a field for "Add to …"; click
//! a chip for its menu (move, remove, summarize by, filter values).

use std::sync::Arc;

use egui::{Align2, Id, LayerId, Order, Rect, RichText, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use gridcraft_engine::datamodel::Model;
use gridcraft_engine::datamodel::pivot::layout;
use gridcraft_engine::datamodel::store::DataType;
use serde_json::{Value as Json, json};

use super::{AGGS, FilterField, PivotForm, ValueField, error_line, field_values, fit, name, ok_cancel, preview_grid, quote, window};
use crate::SheetApp;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};

/// One of the four areas of the PivotTable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Area {
    Filters,
    Columns,
    Rows,
    Values,
}

const AREAS: [Area; 4] = [Area::Filters, Area::Columns, Area::Rows, Area::Values];

impl Area {
    fn label(self) -> &'static str {
        match self {
            Area::Filters => "Filters",
            Area::Columns => "Columns",
            Area::Rows => "Rows",
            Area::Values => "Values",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Area::Filters => Icon::Filter,
            Area::Columns => Icon::TextColumns,
            Area::Rows => Icon::SheetList,
            Area::Values => Icon::Sum,
        }
    }
}

/// What a drag carries: a field from the list, or a chip from an area.
#[derive(Clone, Debug)]
enum Drag {
    Field { field: String, measure: bool },
    Chip { area: Area, index: usize },
}

/// A field taken out of an area, on its way to another.
struct Item {
    field: String,
    measure: bool,
    agg: Option<usize>,
    keep: Vec<String>,
}

/// A change to the layout, applied after the frame is drawn.
enum Action {
    Toggle { field: String, measure: bool },
    Add { field: String, measure: bool, to: Area, at: usize },
    Move { from: Area, index: usize, to: Area, at: usize },
    Remove { area: Area, index: usize },
}

fn is_numeric(m: &Model, field: &str) -> bool {
    let Ok(gridcraft_engine::datamodel::dax::Expr::Column { table, column }) = gridcraft_engine::datamodel::dax::parse(field) else { return false };
    m.table(&table).and_then(|t| t.column_index(&column).and_then(|c| t.column(c))).is_some_and(|c| c.data_type() == DataType::Number)
}

/// The column part of `Table[Column]`.
fn short(field: &str) -> &str {
    field.rsplit_once('[').map(|(_, c)| c.trim_end_matches(']')).unwrap_or(field)
}

impl PivotForm {
    fn len(&self, a: Area) -> usize {
        match a {
            Area::Filters => self.filters.len(),
            Area::Columns => self.columns.len(),
            Area::Rows => self.rows.len(),
            Area::Values => self.values.len(),
        }
    }

    /// The chip text of item `i` of area `a`.
    fn chip_label(&self, a: Area, i: usize) -> String {
        match a {
            Area::Filters => self.filters.get(i).map(|f| short(&f.field).to_string()).unwrap_or_default(),
            Area::Columns => self.columns.get(i).map(|f| short(f).to_string()).unwrap_or_default(),
            Area::Rows => self.rows.get(i).map(|f| short(f).to_string()).unwrap_or_default(),
            Area::Values => {
                self.values
                    .get(i)
                    .map(|v| {
                        if v.measure {
                            v.source.clone()
                        } else {
                            format!("{} of {}", AGGS.get(v.agg).map(|a| a.0).unwrap_or("Sum"), short(&v.source))
                        }
                    })
                    .unwrap_or_default()
            }
        }
    }

    /// Whether the field is used anywhere (its checkbox in the field list).
    fn uses(&self, field: &str, measure: bool) -> bool {
        if measure {
            return self.values.iter().any(|v| v.measure && v.source == field);
        }
        self.rows.iter().any(|f| f == field)
            || self.columns.iter().any(|f| f == field)
            || self.filters.iter().any(|f| f.field == field)
            || self.values.iter().any(|v| !v.measure && v.source == field)
    }

    fn remove_everywhere(&mut self, field: &str, measure: bool) {
        self.values.retain(|v| !(v.source == field && v.measure == measure));
        if !measure {
            self.rows.retain(|f| f != field);
            self.columns.retain(|f| f != field);
            self.filters.retain(|f| f.field != field);
        }
    }

    fn take(&mut self, a: Area, i: usize) -> Option<Item> {
        match a {
            Area::Filters => {
                (i < self.filters.len()).then(|| self.filters.remove(i)).map(|f| Item { field: f.field, measure: false, agg: None, keep: f.keep })
            }
            Area::Columns => {
                (i < self.columns.len()).then(|| self.columns.remove(i)).map(|f| Item { field: f, measure: false, agg: None, keep: vec![] })
            }
            Area::Rows => (i < self.rows.len()).then(|| self.rows.remove(i)).map(|f| Item { field: f, measure: false, agg: None, keep: vec![] }),
            Area::Values => (i < self.values.len()).then(|| self.values.remove(i)).map(|v| Item {
                field: v.source,
                measure: v.measure,
                agg: Some(v.agg),
                keep: vec![],
            }),
        }
    }

    /// Puts `it` at position `at` of area `a`. A column field lives in at most one of Rows,
    /// Columns and Filters, so placing it there moves it. Measures only go in Values.
    fn put(&mut self, m: &Model, a: Area, at: usize, it: Item) -> Result<(), String> {
        if a != Area::Values && it.measure {
            return Err(format!("{} is a measure: measures can only go in Values.", it.field));
        }
        if a != Area::Values {
            self.rows.retain(|f| *f != it.field);
            self.columns.retain(|f| *f != it.field);
            self.filters.retain(|f| f.field != it.field);
        }
        let at = at.min(self.len(a));
        match a {
            Area::Filters => self.filters.insert(at, FilterField { field: it.field, keep: it.keep }),
            Area::Columns => self.columns.insert(at, it.field),
            Area::Rows => self.rows.insert(at, it.field),
            Area::Values => {
                let agg = it.agg.unwrap_or(if it.measure || is_numeric(m, &it.field) { 0 } else { 2 });
                self.values.insert(at, ValueField { source: it.field, measure: it.measure, agg });
            }
        }
        Ok(())
    }

    fn apply(&mut self, m: &Model, action: Action) {
        let r = match action {
            Action::Toggle { field, measure } => {
                if self.uses(&field, measure) {
                    self.remove_everywhere(&field, measure);
                    Ok(())
                } else {
                    let to = if measure || is_numeric(m, &field) { Area::Values } else { Area::Rows };
                    let at = self.len(to);
                    self.put(m, to, at, Item { field, measure, agg: None, keep: vec![] })
                }
            }
            Action::Add { field, measure, to, at } => self.put(m, to, at, Item { field, measure, agg: None, keep: vec![] }),
            Action::Move { from, index, to, at } => {
                // Moving down within one area: the item's own slot closes up first.
                let at = if from == to && at > index { at - 1 } else { at };
                match self.take(from, index) {
                    Some(it) => {
                        let back = (it.field.clone(), it.measure, it.agg, it.keep.clone());
                        let r = self.put(m, to, at, it);
                        if r.is_err() {
                            // Refused (a measure outside Values): put it back where it was.
                            let _ = self.put(m, from, index, Item { field: back.0, measure: back.1, agg: back.2, keep: back.3 });
                        }
                        r
                    }
                    None => Ok(()),
                }
            }
            Action::Remove { area, index } => {
                self.take(area, index);
                Ok(())
            }
        };
        self.error = r.err();
    }
}

/// A row of the field list: checkbox, (measure) icon and name. Click toggles, drag carries it.
fn field_row(ui: &mut Ui, t: &Tokens, label: &str, checked: bool, measure: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click_and_drag());
    let p = ui.painter();
    if resp.hovered() || resp.dragged() {
        p.rect_filled(rect, 3.0, t.hover);
    }
    let bx = Rect::from_center_size(pos2(rect.left() + 12.0, rect.center().y), vec2(13.0, 13.0));
    if checked {
        p.rect_filled(bx, 2.0, t.accent);
        icons::paint(p, bx.shrink(1.0), Icon::Check, t.on_accent);
    } else {
        p.rect_filled(bx, 2.0, t.input_bg);
        p.rect_stroke(bx, 2.0, Stroke::new(1.0, t.input_border), StrokeKind::Inside);
    }
    let mut x = rect.left() + 26.0;
    if measure {
        icons::paint(p, Rect::from_center_size(pos2(x + 7.0, rect.center().y), vec2(14.0, 14.0)), Icon::Function, t.accent);
        x += 18.0;
    }
    let font = if checked { theme::ui_bold(12.5) } else { theme::ui_font(12.5) };
    p.with_clip_rect(Rect::from_min_max(pos2(x, rect.top()), rect.max)).text(pos2(x, rect.center().y), Align2::LEFT_CENTER, label, font, t.text);
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::CheckBox, enabled, checked, label));
    resp
}

/// The field list: tables with their measures and columns.
fn field_list(ui: &mut Ui, t: &Tokens, m: &Model, f: &mut PivotForm, h: f32, actions: &mut Vec<Action>) {
    let q = f.search.to_lowercase();
    egui::ScrollArea::vertical().id_salt("pp_fields").max_height(h).auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut shown = 0;
        for tb in &m.tables {
            let ti = m.table_index(&tb.name);
            let measures: Vec<_> =
                m.measures.iter().filter(|ms| Some(ms.table) == ti && (q.is_empty() || ms.name.to_lowercase().contains(&q))).collect();
            let cols: Vec<_> = tb.columns.iter().filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q)).collect();
            if measures.is_empty() && cols.is_empty() {
                continue;
            }
            shown += 1;
            let used =
                measures.iter().any(|ms| f.uses(&ms.name, true)) || cols.iter().any(|c| f.uses(&format!("{}[{}]", quote(&tb.name), c.name), false));
            let title = RichText::new(&tb.name).strong().color(if used { t.accent } else { t.text });
            egui::CollapsingHeader::new(title)
                .id_salt(("pp_ft", &tb.name))
                .default_open(m.tables.len() <= 4 || used)
                .open(if q.is_empty() { None } else { Some(true) })
                .show(ui, |ui| {
                    let rows = measures.iter().map(|ms| (ms.name.clone(), ms.name.clone(), true, ms.expression.clone())).chain(
                        cols.iter().map(|c| (format!("{}[{}]", quote(&tb.name), c.name), c.name.clone(), false, format!("{}[{}]", tb.name, c.name))),
                    );
                    for (field, label, measure, tip) in rows {
                        let on = f.uses(&field, measure);
                        let resp = field_row(ui, t, &label, on, measure);
                        if resp.dragged() {
                            resp.dnd_set_drag_payload(Drag::Field { field: field.clone(), measure });
                        }
                        if resp.clicked() {
                            actions.push(Action::Toggle { field: field.clone(), measure });
                        }
                        let _ = resp.context_menu(|ui| {
                            for a in AREAS {
                                let ok = !measure || a == Area::Values;
                                if ui.add_enabled(ok, egui::Button::new(format!("Add to {}", a.label()))).clicked() {
                                    actions.push(Action::Add { field: field.clone(), measure, to: a, at: usize::MAX });
                                    ui.close();
                                }
                            }
                            if on && ui.button("Remove Field").clicked() {
                                actions.push(Action::Toggle { field: field.clone(), measure });
                                ui.close();
                            }
                        });
                        resp.on_hover_text(if measure { format!("Measure: {tip}") } else { format!("{tip}\nTick to add, or drag to an area below") });
                    }
                });
        }
        if shown == 0 {
            ui.label(RichText::new("No fields match the search.").color(t.text_dim));
        }
    });
}

/// One of the four drop areas, with its chips.
fn area_box(ui: &mut Ui, t: &Tokens, m: &Model, f: &mut PivotForm, a: Area, size: egui::Vec2, actions: &mut Vec<Action>) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            icons::paint(ui.painter(), r, a.icon(), t.text_dim);
            ui.label(RichText::new(a.label()).size(12.0).color(t.text_dim));
        });
        let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
        let zone = ui.interact(rect, ui.scope_id().with(("pp_area", a.label())), Sense::hover());
        let hovering = zone.dnd_hover_payload::<Drag>();
        let fill = if hovering.is_some() { t.hover } else { t.input_bg };
        let border = if hovering.is_some() { Stroke::new(1.5, t.accent) } else { Stroke::new(1.0, t.input_border) };
        ui.painter().rect_filled(rect, 4.0, fill);
        let mut chips: Vec<Rect> = Vec::new();
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(4.0)), |ui| {
            egui::ScrollArea::vertical().id_salt(("pp_area_scroll", a.label())).auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 3.0;
                for i in 0..f.len(a) {
                    chips.push(chip(ui, t, m, f, a, i, actions));
                }
                if f.len(a) == 0 {
                    let hint = match a {
                        Area::Filters => "Drag fields here to filter the whole report",
                        Area::Columns => "Drag fields here for column headings",
                        Area::Rows => "Drag fields here for row headings",
                        Area::Values => "Drag numbers or measures here",
                    };
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(RichText::new(hint).size(11.0).italics().color(t.text_dim)).wrap());
                }
            });
        });
        ui.painter().rect_stroke(rect, 4.0, border, StrokeKind::Inside);
        // Where a drop would land: before the first chip below the pointer.
        let pointer = ui.input(|i| i.pointer.interact_pos());
        let at = pointer.map(|p| chips.iter().filter(|r| r.center().y < p.y).count()).unwrap_or(usize::MAX);
        if hovering.is_some()
            && let Some(p) = pointer
        {
            let y = chips.get(at).map(|r| r.top() - 2.0).or_else(|| chips.last().map(|r| r.bottom() + 1.0)).unwrap_or(rect.top() + 4.0);
            let _ = p;
            ui.painter().line_segment([pos2(rect.left() + 6.0, y), pos2(rect.right() - 6.0, y)], Stroke::new(2.0, t.accent));
        }
        if let Some(d) = zone.dnd_release_payload::<Drag>() {
            match Arc::unwrap_or_clone(d) {
                Drag::Field { field, measure } => actions.push(Action::Add { field, measure, to: a, at }),
                Drag::Chip { area, index } => actions.push(Action::Move { from: area, index, to: a, at }),
            }
        }
    });
}

/// A field chip in an area: click for its menu, drag to move it.
#[allow(clippy::too_many_arguments)]
fn chip(ui: &mut Ui, t: &Tokens, m: &Model, f: &mut PivotForm, a: Area, i: usize, actions: &mut Vec<Action>) -> Rect {
    let label = f.chip_label(a, i);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
    let p = ui.painter();
    p.rect_filled(rect, 3.0, if resp.hovered() { t.hover } else { t.window });
    p.rect_stroke(rect, 3.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.input_border }), StrokeKind::Inside);
    let filtered = a == Area::Filters && f.filters.get(i).is_some_and(|x| !x.keep.is_empty());
    let text_right = rect.right() - 22.0;
    p.with_clip_rect(Rect::from_min_max(rect.min, pos2(text_right, rect.max.y))).text(
        pos2(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        &label,
        theme::ui_font(12.5),
        t.text,
    );
    icons::paint(p, Rect::from_center_size(pos2(rect.right() - 11.0, rect.center().y), vec2(10.0, 10.0)), Icon::Chevron, t.text_dim);
    if filtered {
        icons::paint(p, Rect::from_center_size(pos2(text_right - 6.0, rect.center().y), vec2(12.0, 12.0)), Icon::Filter, t.accent);
    }
    let enabled = ui.is_enabled();
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, format!("{label} in {}", a.label())));
    if resp.dragged() {
        resp.dnd_set_drag_payload(Drag::Chip { area: a, index: i });
    }
    let n = f.len(a);
    egui::Popup::menu(&resp).show(|ui| {
        let mut mv = |ui: &mut Ui, text: &str, ok: bool, to: Area, at: usize| {
            if ui.add_enabled(ok, egui::Button::new(text)).clicked() {
                actions.push(Action::Move { from: a, index: i, to, at });
                ui.close();
            }
        };
        mv(ui, "Move Up", i > 0, a, i.saturating_sub(1));
        mv(ui, "Move Down", i + 1 < n, a, i + 2);
        mv(ui, "Move to Beginning", i > 0, a, 0);
        mv(ui, "Move to End", i + 1 < n, a, n);
        ui.separator();
        let measure = a == Area::Values && f.values.get(i).is_some_and(|v| v.measure);
        for to in AREAS {
            mv(ui, &format!("Move to {}", to.label()), to != a && (!measure || to == Area::Values), to, usize::MAX);
        }
        ui.separator();
        if ui.button("Remove Field").clicked() {
            actions.push(Action::Remove { area: a, index: i });
            ui.close();
        }
        if a == Area::Values
            && let Some(v) = f.values.get_mut(i)
            && !v.measure
        {
            ui.separator();
            ui.menu_button("Summarize Values By", |ui| {
                for (k, (lbl, _)) in AGGS.iter().enumerate() {
                    if ui.radio(v.agg == k, *lbl).clicked() {
                        v.agg = k;
                        ui.close();
                    }
                }
            });
        }
        if a == Area::Filters
            && let Some(flt) = f.filters.get_mut(i)
        {
            ui.separator();
            ui.menu_button("Filter Values", |ui| filter_values(ui, m, flt));
        }
    });
    let tip = match a {
        Area::Values => f.values.get(i).map(|v| if v.measure { format!("Measure {}", v.source) } else { v.source.clone() }),
        Area::Filters => {
            f.filters.get(i).map(|x| if x.keep.is_empty() { format!("{}: (All)", x.field) } else { format!("{}: {}", x.field, x.keep.join(", ")) })
        }
        Area::Rows => f.rows.get(i).cloned(),
        Area::Columns => f.columns.get(i).cloned(),
    };
    if let Some(tip) = tip {
        resp.on_hover_text(format!("{tip}\nClick for options, drag to move"));
    }
    rect
}

/// The values a report filter keeps (none ticked off = all).
fn filter_values(ui: &mut Ui, m: &Model, flt: &mut FilterField) {
    let all = field_values(m, &flt.field);
    if ui.button("(Select All)").clicked() {
        flt.keep.clear();
    }
    egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
        for v in &all {
            let mut on = flt.keep.is_empty() || flt.keep.contains(v);
            if ui.checkbox(&mut on, v).changed() {
                if flt.keep.is_empty() {
                    flt.keep = all.clone();
                }
                if on {
                    flt.keep.push(v.clone());
                } else {
                    flt.keep.retain(|x| x != v);
                }
                if flt.keep.len() == all.len() {
                    flt.keep.clear();
                }
            }
        }
    });
}

/// The floating chip that follows the pointer during a drag.
fn drag_ghost(ui: &Ui, t: &Tokens, f: &PivotForm) {
    let Some(d) = egui::DragAndDrop::payload::<Drag>(ui.ctx()) else { return };
    let Some(p) = ui.ctx().pointer_interact_pos() else { return };
    let label = match d.as_ref() {
        Drag::Field { field, measure } => {
            if *measure {
                field.clone()
            } else {
                short(field).to_string()
            }
        }
        Drag::Chip { area, index } => f.chip_label(*area, *index),
    };
    let painter = ui.ctx().layer_painter(LayerId::new(Order::Tooltip, Id::unique("pp_drag_ghost")));
    let galley = painter.layout_no_wrap(label, theme::ui_font(12.5), t.text);
    let r = Rect::from_min_size(p + vec2(12.0, 6.0), galley.size() + vec2(16.0, 10.0));
    painter.rect_filled(r.translate(vec2(1.0, 2.0)), 4.0, t.shadow);
    painter.rect_filled(r, 4.0, t.window);
    painter.rect_stroke(r, 4.0, Stroke::new(1.5, t.accent), StrokeKind::Inside);
    painter.galley(r.min + vec2(8.0, 5.0), galley, t.text);
}

/// The report preview, or how to start one.
fn preview(ui: &mut Ui, t: &Tokens, m: &Model, f: &mut PivotForm, w: f32, h: f32) {
    let spec = f.spec();
    let key = format!("{:?}|{:?}|{:?}|{:?}", spec.rows, spec.columns, spec.values, spec.filters);
    if f.preview.as_ref().is_none_or(|(k, _)| *k != key) {
        let r = if spec.values.is_empty() && spec.rows.is_empty() && spec.columns.is_empty() {
            Err(String::new())
        } else if spec.values.is_empty() {
            Err("Add a number or a measure to Values to see the totals.".to_string())
        } else {
            layout(m, &spec).map_err(|e| e.to_string())
        };
        f.preview = Some((key, r));
    }
    egui::Frame::NONE.fill(t.grid_bg).stroke(Stroke::new(1.0, t.input_border)).corner_radius(4.0).inner_margin(10.0).show(ui, |ui| {
        // A fixed size: sizing from the space available would feed the window's own width back
        // into itself and widen it a little every frame.
        ui.set_width(w - 22.0);
        ui.set_min_height(h);
        match f.preview.as_ref().map(|(_, r)| r) {
            Some(Ok(g)) => preview_grid(ui, t, g, h),
            Some(Err(e)) if !e.is_empty() => {
                ui.label(RichText::new(e).color(t.text_dim));
            }
            _ => {
                ui.add_space(h * 0.3);
                ui.vertical_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::hover());
                    icons::paint(ui.painter(), r, Icon::PivotTable, t.text_dim);
                    ui.add_space(8.0);
                    ui.label(RichText::new("To build a report, choose fields from the PivotTable Fields list").strong());
                    ui.label(RichText::new("Tick a field, or drag it into Filters, Columns, Rows or Values.").color(t.text_dim));
                });
            }
        }
    });
}

pub(super) fn pivot(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model, f: &mut PivotForm) -> bool {
    let mut keep = true;
    let mut actions: Vec<Action> = Vec::new();
    // Every width here is fixed from the screen size, never from the space available, so the
    // window keeps its size frame after frame.
    let screen = ctx.content_rect();
    let pane_w = 330.0;
    let total_w = (screen.width() - 80.0).clamp(720.0, 1120.0);
    let preview_w = total_w - pane_w - 12.0;
    fit(window("PivotTable from Data Model").resizable(false), ctx, vec2(total_w + 20.0, 700.0)).show(ctx, |ui| {
        ui.set_width(total_w);
        let body_h = (screen.height() - 200.0).clamp(380.0, 640.0);
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(preview_w);
                ui.label(RichText::new("PivotTable").strong());
                preview(ui, t, m, f, preview_w, body_h - 4.0);
            });
            ui.add_space(12.0);
            ui.vertical(|ui| {
                ui.set_width(pane_w);
                ui.label(RichText::new("PivotTable Fields").strong().size(15.0));
                ui.label(RichText::new("Choose fields to add to report:").size(11.5).color(t.text_dim));
                let r = ui.add(egui::TextEdit::singleline(&mut f.search).hint_text("Search").desired_width(pane_w));
                name(&r, "Search fields");
                let list_h = (body_h * 0.42).max(140.0);
                egui::Frame::NONE.stroke(Stroke::new(1.0, t.input_border)).corner_radius(4.0).inner_margin(4.0).show(ui, |ui| {
                    field_list(ui, t, m, f, list_h, &mut actions);
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Drag fields between areas below:").size(11.5).color(t.text_dim));
                let box_h = ((body_h - list_h - 120.0) / 2.0).max(72.0);
                let box_w = (pane_w - 8.0) / 2.0;
                for pair in [[Area::Filters, Area::Columns], [Area::Rows, Area::Values]] {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        for a in pair {
                            area_box(ui, t, m, f, a, vec2(box_w, box_h), &mut actions);
                        }
                    });
                }
            });
        });
        drag_ghost(ui, t, f);
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Put the PivotTable on");
            ui.radio_value(&mut f.new_sheet, true, "a new worksheet");
            ui.radio_value(&mut f.new_sheet, false, "an existing worksheet at");
            let r = ui.add_enabled(!f.new_sheet, egui::TextEdit::singleline(&mut f.at).hint_text("Sheet1!H2").desired_width(140.0));
            name(&r, "Location");
            if ui
                .add_enabled(
                    f.len(Area::Filters) + f.len(Area::Columns) + f.len(Area::Rows) + f.len(Area::Values) > 0,
                    egui::Button::new("Clear All"),
                )
                .clicked()
            {
                f.rows.clear();
                f.columns.clear();
                f.values.clear();
                f.filters.clear();
            }
        });
        error_line(ui, t, &f.error);
        let (ok, cancel) = ok_cancel(ui, "Insert PivotTable");
        if cancel {
            keep = false;
        }
        if ok {
            let spec = f.spec();
            let mut params = json!({ "rows": spec.rows, "columns": spec.columns, "values": spec.values, "filters": spec.filters });
            if !f.new_sheet && !f.at.trim().is_empty() {
                params["at"] = Json::String(f.at.trim().to_string());
            }
            match app.run("powerpivot.insertPivot", params) {
                Ok(r) => {
                    if let Some(sheet) = r["sheet"].as_str() {
                        let _ = app.run("sheet.activate", json!({ "sheet": sheet }));
                    }
                    app.powerpivot.open = false;
                    keep = false;
                }
                Err(e) => f.error = Some(e),
            }
        }
    });
    for a in actions {
        f.apply(m, a);
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;
    use gridcraft_engine::datamodel::store::Table;
    use gridcraft_engine::datamodel::{Model, Value};

    fn model() -> Model {
        let mut m = Model::new();
        let rows = vec![vec![Value::Text("East".into()), Value::Number(5.0)], vec![Value::Text("West".into()), Value::Number(7.0)]];
        m.add_table(Table::from_rows("Sales", &["Region".to_string(), "Amount".to_string()], &rows).unwrap()).unwrap();
        m
    }

    fn form() -> PivotForm {
        let super::super::Form::Pivot(p) = super::super::Form::pivot() else { panic!("not a pivot form") };
        *p
    }

    #[test]
    fn ticking_places_numbers_in_values_and_text_in_rows() {
        let m = model();
        let mut f = form();
        f.apply(&m, Action::Toggle { field: "Sales[Region]".into(), measure: false });
        f.apply(&m, Action::Toggle { field: "Sales[Amount]".into(), measure: false });
        assert_eq!(f.rows, vec!["Sales[Region]".to_string()]);
        assert_eq!(f.values.len(), 1);
        assert_eq!(f.chip_label(Area::Values, 0), "Sum of Amount");
        // Unticking removes it everywhere.
        f.apply(&m, Action::Toggle { field: "Sales[Region]".into(), measure: false });
        assert!(f.rows.is_empty());
    }

    #[test]
    fn moving_between_areas_and_reordering() {
        let m = model();
        let mut f = form();
        f.apply(&m, Action::Add { field: "Sales[Region]".into(), measure: false, to: Area::Rows, at: usize::MAX });
        f.apply(&m, Action::Add { field: "Sales[Amount]".into(), measure: false, to: Area::Rows, at: usize::MAX });
        f.apply(&m, Action::Move { from: Area::Rows, index: 0, to: Area::Rows, at: 2 });
        assert_eq!(f.rows, vec!["Sales[Amount]".to_string(), "Sales[Region]".to_string()]);
        // A column field lives in one of Rows / Columns / Filters: adding it to Columns moves it.
        f.apply(&m, Action::Add { field: "Sales[Region]".into(), measure: false, to: Area::Columns, at: 0 });
        assert_eq!(f.rows, vec!["Sales[Amount]".to_string()]);
        assert_eq!(f.columns, vec!["Sales[Region]".to_string()]);
        f.apply(&m, Action::Move { from: Area::Columns, index: 0, to: Area::Filters, at: 0 });
        assert_eq!(f.filters.len(), 1);
        assert!(f.columns.is_empty());
    }

    #[test]
    fn measures_stay_in_values() {
        let m = model();
        let mut f = form();
        f.apply(&m, Action::Add { field: "Total".into(), measure: true, to: Area::Rows, at: 0 });
        assert!(f.rows.is_empty());
        assert!(f.error.is_some());
        f.apply(&m, Action::Toggle { field: "Total".into(), measure: true });
        assert_eq!(f.values.len(), 1);
        f.apply(&m, Action::Move { from: Area::Values, index: 0, to: Area::Columns, at: 0 });
        assert_eq!(f.values.len(), 1, "a refused move puts the measure back");
        assert!(f.columns.is_empty());
    }
}
