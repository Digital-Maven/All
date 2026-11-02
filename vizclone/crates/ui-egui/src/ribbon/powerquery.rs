//! The Power Query tab: get data from tables, files and folders, open the Power Query Editor,
//! refresh queries.

use super::controls::sep;
use super::{Icon, SheetApp, Ui, big_button, json};
use crate::powerquery::{self, Dialog};
use crate::widgets::small_button;

pub(super) fn power_query(app: &mut SheetApp, ui: &mut Ui) {
    let r = big_button(ui, Icon::Table, "Get\nData", "Get data from a table, a file, a workbook, JSON, a folder or typed in", true);
    egui::Popup::menu(&r).show(|ui| {
        powerquery::ops::new_source_items(app, ui);
    });
    ui.vertical(|ui| {
        if small_button(ui, Icon::Table, "From Table/Range", "Use the table at the active cell", false).clicked() {
            let table = app.session.doc().ok().and_then(|d| d.wb.active().and_then(|sh| sh.table_at(d.selection.active).map(|t| t.name.clone())));
            match table {
                Some(t) => {
                    if let Some(r) = powerquery::run(app, "powerquery.newQuery", json!({ "source": { "kind": "table", "table": t } })) {
                        powerquery::open(app, r["name"].as_str());
                    }
                }
                None => app.powerquery.dialog = Some(Dialog::get_data("table")),
            }
        }
        if small_button(ui, Icon::Paste, "From Text/CSV", "A CSV or delimited text file", false).clicked() {
            app.powerquery.dialog = Some(Dialog::get_data("csv"));
        }
        if small_button(ui, Icon::Find, "From Folder", "Every file in a folder (Combine Files appends them)", false).clicked() {
            app.powerquery.dialog = Some(Dialog::get_data("folder"));
        }
    });
    sep(ui);
    if big_button(ui, Icon::Function, "Launch Power\nQuery Editor", "Edit queries step by step", false).clicked() {
        powerquery::open(app, None);
    }
    ui.vertical(|ui| {
        if small_button(ui, Icon::Calc, "Refresh All", "Reload every loaded query", false).clicked()
            && powerquery::run(app, "powerquery.refresh", json!({})).is_some()
        {
            powerquery::toast(app, "Queries refreshed");
        }
        if small_button(ui, Icon::Name, "Manage Parameters", "Named values queries can use", false).clicked() {
            app.powerquery.dialog = Some(Dialog::parameters(app, None));
        }
        if small_button(ui, Icon::Link, "Query Dependencies", "How queries, sources and loads connect", false).clicked() {
            powerquery::open(app, None);
            app.powerquery.deps = true;
        }
    });
    sep(ui);
    let doc = app.powerquery.doc(&app.session);
    let r = big_button(ui, Icon::Script, "Queries &\nConnections", "The workbook's queries", true);
    egui::Popup::menu(&r).show(|ui| {
        if doc.queries.is_empty() {
            ui.label("No queries yet");
        }
        for q in &doc.queries {
            if ui.button(&q.name).clicked() {
                powerquery::open(app, Some(&q.name));
                ui.close();
            }
        }
    });
}
