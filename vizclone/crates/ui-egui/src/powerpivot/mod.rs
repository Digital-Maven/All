//! The Power Pivot window: the workbook's data model, shown in place of the ribbon and grid
//! (like the File view) while it is open.
//!
//! - **Data View** ([`data_view`]): one tab per model table, its rows in a grid, a formula bar
//!   and a calculation area holding the table's measures with their values.
//! - **Diagram View** ([`diagram`]): tables as boxes; relationships drawn between key columns,
//!   created by dragging one column onto another.
//! - **Forms** ([`forms`]): New/Edit Measure, Calculated Column, Create/Manage Relationships,
//!   Manage Measures, New Date Table and the data-model PivotTable builder with a live preview.
//!   Forms also open from the main ribbon's Power Pivot tab with the window closed.
//!
//! Everything the window changes goes through the `powerpivot.*` engine commands
//! ([`SheetApp::run`]); the state here is UI-only. The model itself is built once per workbook
//! version and cached ([`State::model`]).

use std::collections::HashMap;
use std::rc::Rc;

use egui::{RichText, Ui, vec2};
use gridcraft_engine::datamodel::Model;
use serde_json::{Value as Json, json};

use crate::SheetApp;
use crate::clock::now_ms;
use crate::theme::Tokens;

mod data_view;
mod diagram;
mod forms;

pub use forms::Form;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Data,
    Diagram,
}

/// What the formula bar is showing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Selected {
    #[default]
    Nothing,
    /// A column of the current table, by index.
    Column(usize),
    /// A measure, by name.
    Measure(String),
}

#[derive(Default)]
pub struct State {
    /// The window is showing (instead of the ribbon and grid).
    pub open: bool,
    pub view: View,
    /// The Home or Design toolbar tab.
    pub tab: usize,
    /// The Data View's table, by name.
    pub table: String,
    pub selected: Selected,
    /// The formula bar's text, and what it was loaded from.
    pub formula: String,
    formula_for: Option<(String, Selected)>,
    pub calc_area: bool,
    pub diagram: diagram::DiagramState,
    pub form: Option<Form>,
    cache: Option<Cache>,
    /// Sort of the Data View: (table, column, descending).
    pub sort: Option<(String, usize, bool)>,
}

struct Cache {
    key: (u64, usize),
    model: Result<Rc<Model>, String>,
    /// Measure name → displayed value.
    values: HashMap<String, String>,
}

impl State {
    /// The active workbook's model, rebuilt when the workbook changes.
    pub fn model(&mut self, app_session: &gridcraft_engine::Session) -> Result<Rc<Model>, String> {
        let Ok(d) = app_session.doc() else { return Err("No workbook is open.".into()) };
        let key = (d.uid, std::sync::Arc::as_ptr(&d.wb) as usize);
        if self.cache.as_ref().is_none_or(|c| c.key != key) {
            let model = gridcraft_engine::cmd::powerpivot::model_of(&d.wb).map(Rc::new).map_err(|e| e.to_string());
            self.cache = Some(Cache { key, model, values: HashMap::new() });
        }
        self.cache.as_ref().map(|c| c.model.clone()).unwrap_or_else(|| Err("No model".into()))
    }

    /// A measure's value over the whole model, formatted, computed once per workbook version.
    pub fn measure_value(&mut self, m: &Model, name: &str) -> String {
        if let Some(v) = self.cache.as_ref().and_then(|c| c.values.get(name)) {
            return v.clone();
        }
        let expr = format!("[{}]", name.replace(']', "]]"));
        let fmt = m.measure(name).and_then(|ms| ms.format.clone());
        let v = match m.evaluate(&expr) {
            Ok(v) => display_value(&v, fmt.as_deref()),
            Err(e) => format!("Error: {e}"),
        };
        if let Some(c) = self.cache.as_mut() {
            c.values.insert(name.to_string(), v.clone());
        }
        v
    }
}

/// A value as the grids show it: numbers with thousands separators (or the measure's format
/// when it is a simple one), blanks empty.
pub fn display_value(v: &gridcraft_engine::datamodel::Value, fmt: Option<&str>) -> String {
    use gridcraft_engine::datamodel::Value as V;
    match v {
        V::Number(n) if n.is_finite() => {
            let decimals = fmt.and_then(|f| f.split_once('.')).map(|(_, d)| d.chars().take_while(|c| *c == '0' || *c == '#').count());
            let pct = fmt.is_some_and(|f| f.ends_with('%'));
            let n = if pct { n * 100.0 } else { *n };
            let decimals = decimals.unwrap_or(if n.fract() == 0.0 { 0 } else { 2 });
            let s = group_thousands(&format!("{n:.decimals$}"));
            if pct { format!("{s}%") } else { s }
        }
        V::Number(n) => n.to_string(),
        other => other.to_text(),
    }
}

fn group_thousands(s: &str) -> String {
    let (sign, rest) = s.strip_prefix('-').map(|r| ("-", r)).unwrap_or(("", s));
    let (int, frac) = rest.split_once('.').map(|(a, b)| (a, Some(b))).unwrap_or((rest, None));
    let mut out = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    match frac {
        Some(f) => format!("{sign}{out}.{f}"),
        None => format!("{sign}{out}"),
    }
}

/// Opens the window (the Data View on the active cell's table when it is in the model).
pub fn open(app: &mut SheetApp) {
    app.powerpivot.open = true;
    app.backstage = None;
    if let Ok(m) = app.powerpivot.model(&app.session)
        && m.table(&app.powerpivot.table).is_none()
    {
        let active = app.session.doc().ok().and_then(|d| d.wb.active().and_then(|sh| sh.table_at(d.selection.active).map(|t| t.name.clone())));
        app.powerpivot.table = active.filter(|t| m.table(t).is_some()).or_else(|| m.tables.first().map(|t| t.name.clone())).unwrap_or_default();
    }
}

/// Runs a command; on failure shows the error as a toast and returns `None`.
pub(crate) fn run(app: &mut SheetApp, id: &str, params: Json) -> Option<Json> {
    match app.run(id, params) {
        Ok(v) => Some(v),
        Err(e) => {
            app.toast = Some((e, now_ms()));
            None
        }
    }
}

pub(crate) fn toast(app: &mut SheetApp, msg: impl Into<String>) {
    app.toast = Some((msg.into(), now_ms()));
}

/// Shows the window in place of the ribbon, formula bar, grid and sheet tabs.
pub fn show(app: &mut SheetApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && app.powerpivot.form.is_none() {
        app.powerpivot.open = false;
        return;
    }
    let model = app.powerpivot.model(&app.session);
    egui::Panel::top("pp_toolbar")
        .frame(egui::Frame::NONE.fill(t.window).inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 6 }))
        .show(ui, |ui| {
            toolbar(app, ui, &t, model.as_ref().ok());
        });
    egui::Panel::bottom("pp_status").exact_size(28.0).frame(egui::Frame::NONE.fill(t.status_bar).inner_margin(egui::Margin::symmetric(12, 4))).show(
        ui,
        |ui| {
            status_bar(app, ui, &t, model.as_ref().ok());
        },
    );
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.grid_bg)).show(ui, |ui| match &model {
        Err(e) => {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("The data model can't be built").heading());
                ui.label(e);
                ui.add_space(8.0);
                ui.label("Fix the problem in the workbook (for example a table that was renamed or deleted), or remove the table from the model.");
            });
        }
        Ok(m) if m.tables.is_empty() => empty_model(app, ui),
        Ok(m) => match app.powerpivot.view {
            View::Data => data_view::show(app, ui, m),
            View::Diagram => diagram::show(app, ui, m),
        },
    });
}

fn empty_model(app: &mut SheetApp, ui: &mut Ui) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("The data model is empty").heading());
        ui.add_space(6.0);
        ui.label("Format your data as tables (Insert › Table), then add them to the data model.");
        ui.add_space(14.0);
        let tables = app.session.doc().map(|d| d.wb.sheets.iter().map(|s| s.tables.len()).sum::<usize>()).unwrap_or(0);
        if tables > 0 {
            if ui.button(format!("Add all {tables} workbook tables")).clicked() && run(app, "powerpivot.addAllTables", json!({})).is_some() {
                app.powerpivot.table.clear();
                open(app);
            }
        } else {
            ui.label(RichText::new("This workbook has no tables yet.").italics());
        }
    });
}

fn tab_button(ui: &mut Ui, t: &Tokens, label: &str, active: bool) -> bool {
    let text = if active { RichText::new(label).strong().color(t.text) } else { RichText::new(label).color(t.text_dim) };
    ui.add(egui::Button::new(text).frame(false)).clicked()
}

fn tool(ui: &mut Ui, label: &str, tip: &str) -> bool {
    ui.add(egui::Button::new(label).min_size(vec2(0.0, 26.0))).on_hover_text(tip).clicked()
}

fn toolbar(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, m: Option<&Rc<Model>>) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Power Pivot").strong().size(15.0).color(t.accent));
        ui.add_space(16.0);
        for (i, name) in ["Home", "Design"].iter().enumerate() {
            if tab_button(ui, t, name, app.powerpivot.tab == i) {
                app.powerpivot.tab = i;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Back to Workbook").on_hover_text("Close the Power Pivot window (Esc)").clicked() {
                app.powerpivot.open = false;
            }
        });
    });
    ui.add_space(4.0);
    let table = app.powerpivot.table.clone();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if app.powerpivot.tab == 0 {
            if tool(ui, "Refresh", "Reload every table from its worksheet table and rewrite the data-model PivotTables")
                && let Some(r) = run(app, "powerpivot.refresh", json!({}))
            {
                toast(app, format!("Refreshed {} report(s)", r["refreshed"]));
            }
            if tool(ui, "PivotTable", "Summarize the data model in a PivotTable") {
                app.powerpivot.form = Some(Form::pivot());
            }
            ui.separator();
            if tool(ui, "New Measure", "A DAX formula evaluated for each cell of a PivotTable") {
                app.powerpivot.form = Some(Form::new_measure(&table));
            }
            if tool(ui, "Manage Measures", "List, edit and delete the model's measures") {
                app.powerpivot.form = Some(Form::ManageMeasures);
            }
            if tool(ui, "Calculated Column", "A DAX formula evaluated for each row of the table") {
                app.powerpivot.form = Some(Form::new_column(&table));
            }
            ui.separator();
            if tool(ui, "Add All Tables", "Add every workbook table that isn't in the model yet")
                && run(app, "powerpivot.addAllTables", json!({})).is_some()
            {
                toast(app, "Tables added to the data model");
            }
            ui.separator();
            let view = app.powerpivot.view;
            if ui.selectable_label(view == View::Data, "Data View").clicked() {
                app.powerpivot.view = View::Data;
            }
            if ui.selectable_label(view == View::Diagram, "Diagram View").clicked() {
                app.powerpivot.view = View::Diagram;
            }
            if ui.selectable_label(app.powerpivot.calc_area, "Calculation Area").on_hover_text("Show the measures under the grid").clicked() {
                app.powerpivot.calc_area = !app.powerpivot.calc_area;
            }
        } else {
            if tool(ui, "Create Relationship", "Relate a column of one table to the key column of another") {
                app.powerpivot.form = Some(Form::new_relationship(&table));
            }
            if tool(ui, "Manage Relationships", "List, edit, activate and delete relationships") {
                app.powerpivot.form = Some(Form::ManageRelationships);
            }
            if tool(ui, "Detect Relationships", "Create relationships between same-named key columns")
                && let Some(r) = run(app, "powerpivot.detectRelationships", json!({}))
            {
                let n = r["created"].as_array().map(Vec::len).unwrap_or(0);
                toast(app, if n == 0 { "No new relationships found".to_string() } else { format!("Created {n} relationship(s)") });
            }
            ui.separator();
            if tool(ui, "New Date Table", "Build a calendar table from a date column and relate it") {
                app.powerpivot.form = Some(Form::date_table(m.map(|m| m.as_ref()), &table));
            }
            ui.separator();
            let removable = !table.is_empty();
            if ui
                .add_enabled(removable, egui::Button::new("Remove Table").min_size(vec2(0.0, 26.0)))
                .on_hover_text("Remove the table from the data model (the worksheet keeps it)")
                .clicked()
                && run(app, "powerpivot.removeTable", json!({ "table": table })).is_some()
            {
                app.powerpivot.table.clear();
                open(app);
            }
            let sel = app.powerpivot.selected.clone();
            let deletable = match &sel {
                Selected::Measure(_) => true,
                Selected::Column(c) => m.and_then(|m| m.table(&table)).and_then(|t| t.column(*c)).is_some_and(|c| c.expression.is_some()),
                Selected::Nothing => false,
            };
            if ui
                .add_enabled(deletable, egui::Button::new("Delete Selected").min_size(vec2(0.0, 26.0)))
                .on_hover_text("Delete the selected measure or calculated column")
                .clicked()
            {
                delete_selected(app, m.map(|m| m.as_ref()));
            }
        }
    });
}

pub(crate) fn delete_selected(app: &mut SheetApp, m: Option<&Model>) {
    let table = app.powerpivot.table.clone();
    match app.powerpivot.selected.clone() {
        Selected::Measure(n) => {
            if run(app, "powerpivot.deleteMeasure", json!({ "name": n })).is_some() {
                app.powerpivot.selected = Selected::Nothing;
            }
        }
        Selected::Column(c) => {
            let name = m.and_then(|m| m.table(&table)).and_then(|t| t.column(c)).filter(|c| c.expression.is_some()).map(|c| c.name.clone());
            if let Some(name) = name
                && run(app, "powerpivot.deleteColumn", json!({ "table": table, "name": name })).is_some()
            {
                app.powerpivot.selected = Selected::Nothing;
            }
        }
        Selected::Nothing => {}
    }
}

fn status_bar(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, m: Option<&Rc<Model>>) {
    ui.horizontal(|ui| {
        let Some(m) = m else { return };
        let rows: usize = m.tables.iter().map(|t| t.row_count()).sum();
        let text = format!(
            "{} tables · {} rows · {} relationships · {} measures",
            m.tables.len(),
            group_thousands(&rows.to_string()),
            m.relationships.len(),
            m.measures.len()
        );
        ui.label(RichText::new(text).small().color(t.text_dim));
        if app.powerpivot.view == View::Data
            && let Some(tb) = m.table(&app.powerpivot.table)
        {
            ui.separator();
            ui.label(
                RichText::new(format!("{}: {} rows, {} columns", tb.name, group_thousands(&tb.row_count().to_string()), tb.columns.len()))
                    .small()
                    .color(t.text_dim),
            );
        }
    });
}

/// Draws the open form, if any (called every frame, whether the window is open or not).
pub fn forms(app: &mut SheetApp, ctx: &egui::Context) {
    forms::show(app, ctx);
}
