//! The Power Pivot tab: the data model's entry points. Manage opens the Power Pivot window; the
//! other buttons run `powerpivot.*` commands or open their forms over the sheet.

use super::controls::sep;
use super::{Icon, SheetApp, Ui, big_button, json};
use crate::powerpivot::{self, Form};
use crate::widgets::small_button;

pub(super) fn power_pivot(app: &mut SheetApp, ui: &mut Ui) {
    // Data Model.
    if big_button(ui, Icon::Table, "Manage", "Open the Power Pivot window: tables, measures, relationships and the diagram", false).clicked() {
        powerpivot::open(app);
    }
    if big_button(ui, Icon::Plus, "Add to\nData Model", "Add the table at the active cell to the data model", false).clicked()
        && powerpivot::run(app, "powerpivot.addTable", json!({})).is_some()
    {
        powerpivot::toast(app, "Added to the data model");
    }
    ui.vertical(|ui| {
        if small_button(ui, Icon::Table, "Add All Tables", "Add every workbook table that isn't in the model yet", false).clicked()
            && let Some(r) = powerpivot::run(app, "powerpivot.addAllTables", json!({}))
        {
            let n = r["added"].as_array().map(Vec::len).unwrap_or(0);
            powerpivot::toast(app, format!("Added {n} table(s) to the data model"));
        }
        if small_button(ui, Icon::Calc, "Refresh All", "Reload the model from the workbook tables and rewrite its PivotTables", false).clicked()
            && let Some(r) = powerpivot::run(app, "powerpivot.refresh", json!({}))
        {
            powerpivot::toast(app, format!("Refreshed {} report(s)", r["refreshed"]));
        }
    });
    sep(ui);
    // Calculations.
    let table = app.powerpivot.table.clone();
    let table = if table.is_empty() { default_table(app) } else { table };
    if big_button(ui, Icon::Sum, "New\nMeasure", "A DAX formula PivotTables evaluate for each cell", false).clicked() {
        app.powerpivot.form = Some(Form::new_measure(&table));
    }
    ui.vertical(|ui| {
        if small_button(ui, Icon::Function, "Manage Measures", "List, edit and delete measures", false).clicked() {
            app.powerpivot.form = Some(Form::ManageMeasures);
        }
        if small_button(ui, Icon::Fx, "Calculated Column", "A DAX formula evaluated for each row of a table", false).clicked() {
            app.powerpivot.form = Some(Form::new_column(&table));
        }
    });
    sep(ui);
    // Relationships.
    if big_button(ui, Icon::Link, "Relation-\nships", "Create, edit, activate and delete relationships between tables", false).clicked() {
        app.powerpivot.form = Some(Form::ManageRelationships);
    }
    ui.vertical(|ui| {
        if small_button(ui, Icon::Link, "Create Relationship", "Relate a column to the key column of another table", false).clicked() {
            app.powerpivot.form = Some(Form::new_relationship(&table));
        }
        if small_button(ui, Icon::Find, "Detect", "Create relationships between same-named key columns", false).clicked()
            && let Some(r) = powerpivot::run(app, "powerpivot.detectRelationships", json!({}))
        {
            let n = r["created"].as_array().map(Vec::len).unwrap_or(0);
            powerpivot::toast(app, if n == 0 { "No new relationships found".to_string() } else { format!("Created {n} relationship(s)") });
        }
        if small_button(ui, Icon::PageLayout, "Diagram View", "See the tables and their relationships", false).clicked() {
            powerpivot::open(app);
            app.powerpivot.view = powerpivot::View::Diagram;
        }
    });
    sep(ui);
    // Reports.
    if big_button(ui, Icon::PivotTable, "PivotTable", "A PivotTable over the whole data model, with measures", false).clicked() {
        app.powerpivot.form = Some(Form::pivot());
    }
    if big_button(ui, Icon::Calc, "Date\nTable", "Build a calendar table for time intelligence", false).clicked() {
        let m = app.powerpivot.model(&app.session).ok();
        app.powerpivot.form = Some(Form::date_table(m.as_deref(), &table));
    }
}

/// The model table to suggest in forms: the active cell's table when it is in the model, else the
/// model's first table.
fn default_table(app: &mut SheetApp) -> String {
    let Ok(m) = app.powerpivot.model(&app.session) else { return String::new() };
    let active = app.session.doc().ok().and_then(|d| d.wb.active().and_then(|sh| sh.table_at(d.selection.active).map(|t| t.name.clone())));
    active.filter(|t| m.table(t).is_some()).or_else(|| m.tables.first().map(|t| t.name.clone())).unwrap_or_default()
}
