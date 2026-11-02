//! The editor's panes: Queries (left), Query Settings and Applied Steps (right), the formula bar
//! and the status bar.

use egui::{RichText, Ui, vec2};
use gridcraft_engine::query::steps::{LoadTo, QueryDoc, Step};
use serde_json::json;

use super::dialogs::Dialog;
use super::{open, run, toast};
use crate::SheetApp;
use crate::theme::Tokens;

pub fn empty(app: &mut SheetApp, ui: &mut Ui) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new("No queries yet").heading());
        ui.add_space(6.0);
        ui.label("Get data from a table in this workbook, a CSV or text file, another workbook, JSON, a folder, or type it in.");
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 520.0).max(0.0) / 2.0);
            for (label, kind) in [
                ("From Table/Range", "table"),
                ("From Text/CSV", "csv"),
                ("From Workbook", "excel"),
                ("From Folder", "folder"),
                ("Enter Data", "enter"),
            ] {
                if ui.button(label).clicked() {
                    app.powerquery.dialog = Some(Dialog::get_data(kind));
                }
            }
        });
    });
}

fn load_label(l: LoadTo, loaded: bool) -> &'static str {
    match (l, loaded) {
        (LoadTo::Table, true) => "table",
        (LoadTo::Table, false) => "not loaded",
        (LoadTo::Model, _) => "data model",
        (LoadTo::None, _) => "connection only",
    }
}

pub fn queries(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, doc: &QueryDoc) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(format!("Queries [{}]", doc.queries.len())).strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button("+", |ui| super::ops::new_source_items(app, ui));
        });
    });
    ui.add_space(4.0);
    let mut groups: Vec<Option<String>> = Vec::new();
    for q in &doc.queries {
        if !groups.contains(&q.group) {
            groups.push(q.group.clone());
        }
    }
    groups.sort_by_key(|g| g.is_some());
    egui::ScrollArea::vertical().id_salt("pq_queries_scroll").show(ui, |ui| {
        for g in &groups {
            if let Some(name) = g {
                ui.add_space(4.0);
                ui.label(RichText::new(format!("▾ {name}")).color(t.text_dim));
            }
            for q in doc.queries.iter().filter(|q| &q.group == g) {
                let active = q.name == app.powerquery.query;
                let label = RichText::new(&q.name).color(if active { t.on_accent } else { t.text });
                let r = ui.add(egui::Button::new(label).fill(if active { t.accent } else { t.ribbon }).min_size(vec2(ui.available_width(), 24.0)));
                let r = r.on_hover_text(format!(
                    "Loads to: {}{}",
                    load_label(q.load, q.loaded_table.is_some()),
                    if q.description.is_empty() { String::new() } else { format!("\n{}", q.description) }
                ));
                if r.clicked() && !active {
                    app.powerquery.query = q.name.clone();
                    app.powerquery.step = None;
                    app.powerquery.selected.clear();
                    app.powerquery.examples = None;
                }
                let name = q.name.clone();
                let load = q.load;
                r.context_menu(|ui| query_menu(app, ui, &name, load));
            }
        }
        if !doc.parameters.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new("Parameters").color(t.text_dim));
            for p in &doc.parameters {
                let r = ui.add(egui::Button::new(format!("{}  =  {}", p.name, p.value)).frame(false)).on_hover_text(&p.description);
                if r.clicked() {
                    app.powerquery.dialog = Some(Dialog::parameters(app, Some(p.name.clone())));
                }
            }
        }
    });
}

fn query_menu(app: &mut SheetApp, ui: &mut Ui, name: &str, load: LoadTo) {
    if ui.button("Rename…").clicked() {
        app.powerquery.dialog = Some(Dialog::rename_query(name));
        ui.close();
    }
    if ui.button("Duplicate").clicked() {
        if let Some(r) = run(app, "powerquery.duplicateQuery", json!({ "name": name })) {
            open(app, r["name"].as_str());
        }
        ui.close();
    }
    if ui.button("Reference").clicked() {
        if let Some(r) = run(app, "powerquery.referenceQuery", json!({ "name": name })) {
            open(app, r["name"].as_str());
        }
        ui.close();
    }
    if ui.button("Delete").clicked() {
        if run(app, "powerquery.deleteQuery", json!({ "name": name })).is_some() {
            open(app, None);
        }
        ui.close();
    }
    ui.separator();
    let mut enabled = load != LoadTo::None;
    if ui.checkbox(&mut enabled, "Enable Load").clicked() {
        run(app, "powerquery.setQuery", json!({ "name": name, "load": if enabled { "table" } else { "none" } }));
        ui.close();
    }
    if ui.button("Load to Data Model").clicked() {
        if run(app, "powerquery.load", json!({ "query": name, "to": "model" })).is_some() {
            toast(app, format!("{name} is in the data model"));
        }
        ui.close();
    }
    if ui.button("Advanced Editor").clicked() {
        app.powerquery.query = name.to_string();
        app.powerquery.dialog = Some(Dialog::advanced(app));
        ui.close();
    }
    if ui.button("Properties…").clicked() {
        app.powerquery.query = name.to_string();
        app.powerquery.dialog = Some(Dialog::properties(app));
        ui.close();
    }
}

pub fn settings(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, steps: &[Step]) {
    ui.label(RichText::new("Query Settings").strong().size(14.0));
    ui.add_space(6.0);
    ui.label(RichText::new("PROPERTIES").small().color(t.text_dim));
    ui.horizontal(|ui| {
        ui.label("Name");
        let r = ui.add(egui::Button::new(app.powerquery.query.clone()).frame(false)).on_hover_text("Rename the query");
        if r.clicked() {
            let q = app.powerquery.query.clone();
            app.powerquery.dialog = Some(Dialog::rename_query(&q));
        }
    });
    if ui.small_button("All Properties").clicked() {
        app.powerquery.dialog = Some(Dialog::properties(app));
    }
    ui.add_space(10.0);
    ui.label(RichText::new("APPLIED STEPS").small().color(t.text_dim));
    let selected = app.powerquery.step.clone().or_else(|| steps.last().map(|s| s.name.clone()));
    let last = steps.len().saturating_sub(1);
    egui::ScrollArea::vertical().id_salt("pq_steps").show(ui, |ui| {
        for (i, s) in steps.iter().enumerate() {
            let active = selected.as_deref() == Some(s.name.as_str());
            ui.horizontal(|ui| {
                let label = RichText::new(&s.name).color(if active { t.on_accent } else { t.text });
                let w = ui.available_width() - 28.0;
                let r = ui.add(egui::Button::new(label).fill(if active { t.accent } else { t.ribbon }).min_size(vec2(w.max(60.0), 24.0)));
                let r = r.on_hover_text(&s.expr);
                if r.clicked() {
                    app.powerquery.step = if i == last { None } else { Some(s.name.clone()) };
                    app.powerquery.selected.clear();
                }
                if r.double_clicked() {
                    app.powerquery.dialog = Some(Dialog::rename_step(&s.name));
                }
                let name = s.name.clone();
                r.context_menu(|ui| step_menu(app, ui, &name, i, last));
                if ui.small_button("✕").on_hover_text("Delete this step").clicked() {
                    let q = app.powerquery.query.clone();
                    run(app, "powerquery.deleteStep", json!({ "query": q, "step": name }));
                    app.powerquery.step = None;
                }
            });
        }
    });
}

fn step_menu(app: &mut SheetApp, ui: &mut Ui, name: &str, i: usize, last: usize) {
    let q = app.powerquery.query.clone();
    if ui.button("Rename").clicked() {
        app.powerquery.dialog = Some(Dialog::rename_step(name));
        ui.close();
    }
    if ui.button("Delete").clicked() {
        run(app, "powerquery.deleteStep", json!({ "query": q, "step": name }));
        app.powerquery.step = None;
        ui.close();
    }
    if i > 0 && ui.button("Delete Until End").clicked() {
        run(app, "powerquery.deleteStep", json!({ "query": q, "step": name, "untilEnd": true }));
        app.powerquery.step = None;
        ui.close();
    }
    if ui.button("Insert Step After").clicked() {
        app.powerquery.step = if i == last { None } else { Some(name.to_string()) };
        super::add_step(app, "Custom", "{prev}");
        ui.close();
    }
    if i > 1 && ui.button("Move Before").clicked() {
        run(app, "powerquery.moveStep", json!({ "query": q, "step": name, "delta": -1 }));
        ui.close();
    }
    if i > 0 && i < last && ui.button("Move After").clicked() {
        run(app, "powerquery.moveStep", json!({ "query": q, "step": name, "delta": 1 }));
        ui.close();
    }
    if ui.button("View Native Query (M)").clicked() {
        app.powerquery.dialog = Some(Dialog::advanced(app));
        ui.close();
    }
}

pub fn formula_bar(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, steps: &[Step]) {
    let sel = app.powerquery.step.clone().or_else(|| steps.last().map(|s| s.name.clone())).unwrap_or_default();
    let key = Some((app.powerquery.query.clone(), sel.clone()));
    let expr = steps.iter().find(|s| s.name == sel).map(|s| s.expr.clone()).unwrap_or_default();
    if app.powerquery.formula_for != key {
        app.powerquery.formula = format!("= {expr}");
        app.powerquery.formula_for = key;
    }
    egui::Frame::NONE.fill(t.window).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let cancel = ui.add(egui::Button::new("✕")).on_hover_text("Cancel").clicked();
            let ok = ui.add(egui::Button::new("✓")).on_hover_text("Apply (Enter)").clicked();
            if ui.add(egui::Button::new(RichText::new("fx").italics())).on_hover_text("Add a step after this one").clicked() {
                super::add_step(app, "Custom", "{prev}");
            }
            let r = ui.add(egui::TextEdit::singleline(&mut app.powerquery.formula).desired_width(f32::INFINITY).font(egui::FontId::monospace(13.0)));
            super::labeled(&r, "Step formula");
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if cancel {
                app.powerquery.formula_for = None;
            }
            if ok || enter {
                let text = app.powerquery.formula.trim().trim_start_matches('=').trim().to_string();
                if text != expr {
                    let q = app.powerquery.query.clone();
                    if run(app, "powerquery.editStep", json!({ "query": q, "step": sel, "expr": text })).is_none() {
                        app.powerquery.formula_for = Some((String::new(), String::new()));
                    }
                }
            }
        });
    });
}

pub fn status_bar(app: &mut SheetApp, ui: &mut Ui, t: &Tokens) {
    ui.horizontal(|ui| {
        let Some(p) = app.powerquery.preview(&app.session) else { return };
        let text = match (&p.error, &p.scalar) {
            (Some(_), _) => "Error".to_string(),
            (_, Some(_)) => "1 value".to_string(),
            _ => format!("{} COLUMNS, {} ROWS", p.table.columns.len(), p.table.rows.len()),
        };
        ui.label(RichText::new(text).small().color(t.text_dim));
        ui.separator();
        let basis =
            if app.powerquery.view.all_rows { "Column profiling based on entire data set" } else { "Column profiling based on top 1000 rows" };
        ui.label(RichText::new(basis).small().color(t.text_dim));
    });
}
