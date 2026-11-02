//! The Power Query Editor: a full window (like the File view) over the workbook's queries.
//!
//! Left: the Queries pane (queries, groups, parameters). Middle: the formula bar showing the
//! selected step's M, and the preview of the query at that step with column quality,
//! distribution and profile. Right: Query Settings (name, applied steps). The ribbon has Home,
//! Transform, Add Column and View tabs; every action adds an M step through
//! `powerquery.addStep`, so the Advanced Editor always shows the real query.

use std::collections::HashMap;
use std::rc::Rc;

use egui::{RichText, Ui};
use gridcraft_engine::query::profile::ColumnProfile;
use gridcraft_engine::query::steps::{QueryDef, QueryDoc, Step, steps_of};
use gridcraft_engine::query::{Table, Value};
use serde_json::{Value as Json, json};

use crate::SheetApp;
use crate::clock::now_ms;
use crate::theme::Tokens;

mod deps;
pub mod dialogs;
mod grid;
pub mod ops;
mod panes;

pub use dialogs::Dialog;

/// View options (the View tab).
#[derive(Clone, Copy)]
pub struct ViewOpts {
    pub formula_bar: bool,
    pub quality: bool,
    pub distribution: bool,
    pub profile: bool,
    /// Profile every row instead of the top 1000.
    pub all_rows: bool,
    pub monospace: bool,
    pub whitespace: bool,
}

impl Default for ViewOpts {
    fn default() -> Self {
        ViewOpts { formula_bar: true, quality: true, distribution: true, profile: false, all_rows: false, monospace: false, whitespace: false }
    }
}

/// Column From Examples in progress: the example texts typed per row.
#[derive(Clone, Default)]
pub struct Examples {
    pub name: String,
    /// Input columns (empty = all).
    pub inputs: Vec<String>,
    pub values: HashMap<usize, String>,
    pub editing: Option<usize>,
    pub suggestions: Vec<gridcraft_engine::query::examples::Suggestion>,
    pub chosen: usize,
    /// The examples the suggestions were computed for.
    pub computed_for: Vec<(usize, String)>,
}

/// The preview of the query at the selected step.
pub struct Preview {
    pub table: Rc<Table>,
    /// A non-table result (a number, text, a list), shown as a single value.
    pub scalar: Option<String>,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct State {
    pub open: bool,
    pub query: String,
    /// The selected step (`None`: the last one).
    pub step: Option<String>,
    pub tab: usize,
    pub selected: Vec<String>,
    pub formula: String,
    formula_for: Option<(String, String)>,
    pub view: ViewOpts,
    pub dialog: Option<Dialog>,
    pub examples: Option<Examples>,
    pub deps: bool,
    cache: Option<(PreviewKey, Rc<Preview>)>,
    profiles: HashMap<(usize, bool), ColumnProfile>,
    doc_cache: Option<(usize, Rc<QueryDoc>)>,
}

type PreviewKey = (u64, usize, String, Option<String>);

impl State {
    /// The workbook's queries (parsed once per workbook version).
    pub fn doc(&mut self, s: &gridcraft_engine::Session) -> Rc<QueryDoc> {
        let Ok(d) = s.doc() else { return Rc::default() };
        let key = std::sync::Arc::as_ptr(&d.wb) as usize;
        if let Some((k, doc)) = &self.doc_cache
            && *k == key
        {
            return doc.clone();
        }
        let doc = Rc::new(gridcraft_engine::cmd::powerquery::document(&d.wb));
        self.doc_cache = Some((key, doc.clone()));
        doc
    }

    /// The preview of the current query at the selected step.
    pub fn preview(&mut self, s: &gridcraft_engine::Session) -> Option<Rc<Preview>> {
        let d = s.doc().ok()?;
        let key: PreviewKey = (d.uid, std::sync::Arc::as_ptr(&d.wb) as usize, self.query.clone(), self.step.clone());
        if let Some((k, p)) = &self.cache
            && *k == key
        {
            return Some(p.clone());
        }
        let doc = self.doc(s);
        let r = gridcraft_engine::cmd::powerquery::evaluate_query(&d.wb, &doc, &self.query, self.step.as_deref());
        let p = Rc::new(match r {
            Ok(Value::Table(t)) => Preview { table: t, scalar: None, error: None },
            Ok(Value::List(l)) => {
                Preview { table: Rc::new(Table::new(vec!["List".into()], l.iter().map(|v| vec![v.clone()]).collect())), scalar: None, error: None }
            }
            Ok(Value::Record(r)) => Preview {
                table: Rc::new(Table::new(
                    vec!["Name".into(), "Value".into()],
                    r.names.iter().zip(r.values.iter()).map(|(n, v)| vec![Value::text(n), v.clone()]).collect(),
                )),
                scalar: None,
                error: None,
            },
            Ok(v) => Preview { table: Rc::new(Table::default()), scalar: Some(v.display()), error: None },
            Err(e) => Preview { table: Rc::new(Table::default()), scalar: None, error: Some(e) },
        });
        self.cache = Some((key, p.clone()));
        self.profiles.clear();
        Some(p)
    }

    pub fn profile(&mut self, t: &Table, c: usize) -> ColumnProfile {
        let all = self.view.all_rows;
        self.profiles.entry((c, all)).or_insert_with(|| gridcraft_engine::query::profile::profile(t, c, if all { usize::MAX } else { 1000 })).clone()
    }

    pub fn refresh_preview(&mut self) {
        self.cache = None;
        self.profiles.clear();
        self.doc_cache = None;
    }

    pub fn current<'a>(&self, doc: &'a QueryDoc) -> Option<&'a QueryDef> {
        doc.query(&self.query)
    }
}

/// Gives a text box its accessible name.
pub(crate) fn labeled(r: &egui::Response, label: &str) {
    let label = label.to_string();
    r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::TextInput, true, label.clone()));
}

pub(crate) fn toast(app: &mut SheetApp, msg: impl Into<String>) {
    app.toast = Some((msg.into(), now_ms()));
}

pub(crate) fn run(app: &mut SheetApp, id: &str, params: Json) -> Option<Json> {
    match app.run(id, params) {
        Ok(v) => Some(v),
        Err(e) => {
            toast(app, e);
            None
        }
    }
}

/// The steps of the current query.
pub fn steps(app: &mut SheetApp) -> Vec<Step> {
    let doc = app.powerquery.doc(&app.session);
    app.powerquery.current(&doc).and_then(|q| steps_of(&q.m).ok()).unwrap_or_default()
}

/// Adds a step after the selected one. `expr` uses `{prev}` for the previous step.
pub fn add_step(app: &mut SheetApp, name: &str, expr: &str) {
    let query = app.powerquery.query.clone();
    let after = app.powerquery.step.clone();
    let mut p = json!({ "query": query, "name": name, "expr": expr });
    if let Some(a) = &after {
        p["after"] = Json::String(a.clone());
    }
    if let Some(r) = run(app, "powerquery.addStep", p) {
        // Appended at the end: keep following the last step; inserted: select the new one.
        let last = steps(app).last().map(|s| s.name.clone());
        let new = r["step"].as_str().map(str::to_string);
        app.powerquery.step = if new == last { None } else { new };
    }
}

/// Opens the editor on `query` (or the first query).
pub fn open(app: &mut SheetApp, query: Option<&str>) {
    app.powerquery.open = true;
    app.powerpivot.open = false;
    app.backstage = None;
    let doc = app.powerquery.doc(&app.session);
    if let Some(q) = query {
        app.powerquery.query = q.to_string();
        app.powerquery.step = None;
    } else if doc.query(&app.powerquery.query).is_none() {
        app.powerquery.query = doc.queries.first().map(|q| q.name.clone()).unwrap_or_default();
        app.powerquery.step = None;
    }
    app.powerquery.selected.clear();
}

fn tab_button(ui: &mut Ui, t: &Tokens, label: &str, active: bool) -> bool {
    let text = if active { RichText::new(label).strong().color(t.accent) } else { RichText::new(label).color(t.text_dim) };
    let r = ui.add(egui::Button::new(text).frame(false).min_size(egui::vec2(56.0, 26.0)));
    if active {
        let rect = r.rect;
        ui.painter().line_segment(
            [egui::pos2(rect.left() + 6.0, rect.bottom() + 1.0), egui::pos2(rect.right() - 6.0, rect.bottom() + 1.0)],
            egui::Stroke::new(2.5, t.accent),
        );
    }
    r.clicked()
}

/// Shows the editor in place of the ribbon, grid and sheet tabs.
pub fn show(app: &mut SheetApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) && app.powerquery.dialog.is_none() && app.powerquery.examples.is_none() {
        app.powerquery.open = false;
        return;
    }
    let doc = app.powerquery.doc(&app.session);
    if doc.query(&app.powerquery.query).is_none() {
        app.powerquery.query = doc.queries.first().map(|q| q.name.clone()).unwrap_or_default();
        app.powerquery.step = None;
    }
    let steps = steps(app);
    if let Some(s) = &app.powerquery.step
        && !steps.iter().any(|x| &x.name == s)
    {
        app.powerquery.step = None;
    }
    egui::Panel::top("pq_ribbon").frame(egui::Frame::NONE.fill(t.window).inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 6 })).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                crate::icons::paint(ui.painter(), r, crate::icons::Icon::Pq(crate::icons::Pq::NewSource), t.accent);
                ui.label(RichText::new("Power Query Editor").strong().size(15.0).color(t.text));
                ui.add_space(20.0);
                for (i, name) in ["Home", "Transform", "Add Column", "View"].iter().enumerate() {
                    if tab_button(ui, &t, name, app.powerquery.tab == i) {
                        app.powerquery.tab = i;
                    }
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::widgets::small_button(
                        ui,
                        crate::icons::Icon::Back,
                        "Close",
                        "Back to the workbook (Esc). Changes are already saved in the queries.",
                        false,
                    )
                    .clicked()
                    {
                        app.powerquery.open = false;
                    }
                    if crate::widgets::small_button(
                        ui,
                        crate::icons::Icon::Pq(crate::icons::Pq::CloseLoad),
                        "Close & Load",
                        "Load the query where it goes (a table, the data model, or connection only) and close",
                        false,
                    )
                    .clicked()
                    {
                        let q = app.powerquery.query.clone();
                        if !q.is_empty() && run(app, "powerquery.load", json!({ "query": q })).is_some() {
                            app.powerquery.open = false;
                            toast(app, format!("Loaded {q}"));
                        }
                    }
                });
            });
            ui.add_space(2.0);
            ui.separator();
            ops::ribbon(app, ui);
        },
    );
    egui::Panel::bottom("pq_status").exact_size(26.0).frame(egui::Frame::NONE.fill(t.status_bar).inner_margin(egui::Margin::symmetric(12, 4))).show(
        ui,
        |ui| {
            panes::status_bar(app, ui, &t);
        },
    );
    egui::Panel::left("pq_queries")
        .resizable(true)
        .default_size(230.0)
        .min_size(150.0)
        .max_size(420.0)
        .frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(8))
        .show(ui, |ui| {
            panes::queries(app, ui, &t, &doc);
        });
    egui::Panel::right("pq_settings")
        .resizable(true)
        .default_size(250.0)
        .min_size(170.0)
        .max_size(420.0)
        .frame(egui::Frame::NONE.fill(t.ribbon).inner_margin(8))
        .show(ui, |ui| {
            panes::settings(app, ui, &t, &steps);
        });
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.grid_bg)).show(ui, |ui| {
        if doc.queries.is_empty() {
            panes::empty(app, ui);
            return;
        }
        if app.powerquery.view.formula_bar {
            panes::formula_bar(app, ui, &t, &steps);
        }
        grid::show(app, ui, &t);
    });
    if app.powerquery.deps {
        deps::show(app, ui.ctx(), &doc);
    }
}

/// Draws the editor's dialogs (also when the editor is closed: Get Data opens from the ribbon).
pub fn dialogs(app: &mut SheetApp, ctx: &egui::Context) {
    dialogs::show(app, ctx);
}
