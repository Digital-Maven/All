//! The Power Pivot forms: measures, calculated columns, relationships, the date table and the
//! data-model PivotTable builder. Each is a window over the app; OK runs a `powerpivot.*`
//! command and closes it, an error keeps it open with the message shown.

use egui::{RichText, Ui, vec2};
use gridcraft_engine::datamodel::pivot::{PivotGrid, PivotSpec, layout};
use gridcraft_engine::datamodel::store::{DataType, Relationship};
use gridcraft_engine::datamodel::{CrossFilter, Model, Value};
use serde_json::{Value as Json, json};

use super::{Selected, display_value, run, toast};
use crate::SheetApp;
use crate::theme::Tokens;

/// Number formats offered for measures: (label, code; empty = General).
const FORMATS: &[(&str, &str)] = &[
    ("General", ""),
    ("Whole number  1,235", "#,##0"),
    ("Decimal  1,234.57", "#,##0.00"),
    ("Currency  ₹1,234.57", "₹#,##0.00"),
    ("Currency  $1,234.57", "$#,##0.00"),
    ("Percentage  12.35%", "0.00%"),
    ("Percentage  12%", "0%"),
];

/// Functions offered by the formula helpers: (name, template).
const FUNCTIONS: &[(&str, &str)] = &[
    ("SUM", "SUM(Table[Column])"),
    ("AVERAGE", "AVERAGE(Table[Column])"),
    ("COUNTROWS", "COUNTROWS(Table)"),
    ("DISTINCTCOUNT", "DISTINCTCOUNT(Table[Column])"),
    ("MIN", "MIN(Table[Column])"),
    ("MAX", "MAX(Table[Column])"),
    ("CALCULATE", "CALCULATE([Measure], Table[Column] = \"value\")"),
    ("DIVIDE", "DIVIDE([Numerator], [Denominator])"),
    ("SUMX", "SUMX(Table, Table[Qty] * Table[Price])"),
    ("AVERAGEX", "AVERAGEX(Table, expression)"),
    ("FILTER", "FILTER(Table, condition)"),
    ("ALL", "ALL(Table)"),
    ("ALLEXCEPT", "ALLEXCEPT(Table, Table[Column])"),
    ("RELATED", "RELATED(Lookup[Column])"),
    ("IF", "IF(condition, then, else)"),
    ("SWITCH", "SWITCH(TRUE(), condition, result, else)"),
    ("TOTALYTD", "TOTALYTD([Measure], Calendar[Date])"),
    ("SAMEPERIODLASTYEAR", "CALCULATE([Measure], SAMEPERIODLASTYEAR(Calendar[Date]))"),
    ("DATEADD", "CALCULATE([Measure], DATEADD(Calendar[Date], -1, MONTH))"),
    ("RANKX", "RANKX(ALL(Table[Column]), [Measure])"),
    ("SELECTEDVALUE", "SELECTEDVALUE(Table[Column])"),
    ("VAR … RETURN", "VAR x = [Measure] RETURN x"),
];

pub enum Form {
    Measure(MeasureForm),
    Column(ColumnForm),
    Relationship(RelForm),
    ManageRelationships,
    ManageMeasures,
    DateTable(DateForm),
    Pivot(Box<PivotForm>),
}

pub struct MeasureForm {
    editing: Option<String>,
    table: String,
    name: String,
    expr: String,
    format: String,
    check: Option<Result<String, String>>,
    error: Option<String>,
}

pub struct ColumnForm {
    editing: Option<String>,
    table: String,
    name: String,
    expr: String,
    check: Option<Result<String, String>>,
    error: Option<String>,
}

pub struct RelForm {
    editing: Option<(String, String, String, String)>,
    ft: String,
    fc: String,
    tt: String,
    tc: String,
    both: bool,
    active: bool,
    error: Option<String>,
}

pub struct DateForm {
    column: String,
    name: String,
    error: Option<String>,
}

/// A value field of the PivotTable builder: a measure, or an implicit measure over a column.
#[derive(Clone, PartialEq)]
struct ValueField {
    /// Measure name, or the column (`Table[Column]`) of an implicit measure.
    source: String,
    measure: bool,
    /// Implicit measures: Sum, Average, Count, Distinct Count, Min, Max.
    agg: usize,
}

const AGGS: &[(&str, &str)] =
    &[("Sum", "SUM"), ("Average", "AVERAGE"), ("Count", "COUNTA"), ("Distinct Count", "DISTINCTCOUNT"), ("Min", "MIN"), ("Max", "MAX")];

impl ValueField {
    fn spec(&self) -> String {
        if self.measure {
            return self.source.clone();
        }
        let (label, f) = AGGS.get(self.agg).copied().unwrap_or(("Sum", "SUM"));
        let col = self.source.rsplit_once('[').map(|(_, c)| c.trim_end_matches(']')).unwrap_or(&self.source);
        format!("{label} of {col} := {f}({})", self.source)
    }
}

/// A report filter: a field and the values kept (empty = all).
#[derive(Clone, PartialEq)]
struct FilterField {
    field: String,
    keep: Vec<String>,
}

pub struct PivotForm {
    rows: Vec<String>,
    columns: Vec<String>,
    values: Vec<ValueField>,
    filters: Vec<FilterField>,
    search: String,
    new_sheet: bool,
    at: String,
    preview: Option<(String, Result<PivotGrid, String>)>,
    error: Option<String>,
}

impl Form {
    pub fn new_measure(table: &str) -> Form {
        Form::Measure(MeasureForm {
            editing: None,
            table: table.to_string(),
            name: String::new(),
            expr: String::new(),
            format: String::new(),
            check: None,
            error: None,
        })
    }

    pub fn edit_measure(m: &Model, name: &str) -> Form {
        let ms = m.measure(name);
        Form::Measure(MeasureForm {
            editing: Some(name.to_string()),
            table: ms.and_then(|x| m.tables.get(x.table)).map(|t| t.name.clone()).unwrap_or_default(),
            name: name.to_string(),
            expr: ms.map(|x| x.expression.clone()).unwrap_or_default(),
            format: ms.and_then(|x| x.format.clone()).unwrap_or_default(),
            check: None,
            error: None,
        })
    }

    pub fn new_column(table: &str) -> Form {
        Form::Column(ColumnForm { editing: None, table: table.to_string(), name: String::new(), expr: String::new(), check: None, error: None })
    }

    pub fn edit_column(table: &str, name: &str, expr: &str) -> Form {
        Form::Column(ColumnForm {
            editing: Some(name.to_string()),
            table: table.to_string(),
            name: name.to_string(),
            expr: expr.to_string(),
            check: None,
            error: None,
        })
    }

    pub fn new_relationship(table: &str) -> Form {
        Form::Relationship(RelForm {
            editing: None,
            ft: table.to_string(),
            fc: String::new(),
            tt: String::new(),
            tc: String::new(),
            both: false,
            active: true,
            error: None,
        })
    }

    pub fn edit_relationship(m: &Model, r: &Relationship) -> Form {
        let tn = |t: usize| m.tables.get(t).map(|x| x.name.clone()).unwrap_or_default();
        let cn = |t: usize, c: usize| m.tables.get(t).and_then(|x| x.column(c)).map(|c| c.name.clone()).unwrap_or_default();
        let names = (tn(r.from_table), cn(r.from_table, r.from_column), tn(r.to_table), cn(r.to_table, r.to_column));
        Form::Relationship(RelForm {
            ft: names.0.clone(),
            fc: names.1.clone(),
            tt: names.2.clone(),
            tc: names.3.clone(),
            editing: Some(names),
            both: r.cross_filter == CrossFilter::Both,
            active: r.active,
            error: None,
        })
    }

    pub fn date_table(m: Option<&Model>, table: &str) -> Form {
        // Suggest the first numeric column whose name mentions a date.
        let guess = m
            .and_then(|m| {
                let ordered = m.table(table).into_iter().chain(m.tables.iter());
                ordered.flat_map(|t| t.columns.iter().map(move |c| (t, c))).find(|(_, c)| {
                    let n = c.name.to_lowercase();
                    c.data_type() == DataType::Number && (n.contains("date") || n.contains("day") || n.contains("time"))
                })
            })
            .map(|(t, c)| format!("{}[{}]", quote(&t.name), c.name))
            .unwrap_or_default();
        Form::DateTable(DateForm { column: guess, name: "Calendar".into(), error: None })
    }

    /// The PivotTable builder with fields already placed (`values` are measure names).
    pub fn pivot_with(rows: Vec<String>, columns: Vec<String>, values: Vec<String>) -> Form {
        let mut f = Form::pivot();
        if let Form::Pivot(p) = &mut f {
            p.rows = rows;
            p.columns = columns;
            p.values = values.into_iter().map(|v| ValueField { source: v, measure: true, agg: 0 }).collect();
        }
        f
    }

    pub fn pivot() -> Form {
        Form::Pivot(Box::new(PivotForm {
            rows: vec![],
            columns: vec![],
            values: vec![],
            filters: vec![],
            search: String::new(),
            new_sheet: true,
            at: String::new(),
            preview: None,
            error: None,
        }))
    }
}

fn quote(t: &str) -> String {
    gridcraft_engine::datamodel::model::quote_table(t)
}

/// Gives a combo box (or other unlabelled control) its accessible name.
pub(crate) fn name(r: &egui::Response, label: &str) {
    let label = label.to_string();
    r.widget_info(move || egui::WidgetInfo::labeled(egui::Role::ComboBox, true, label.clone()));
}

pub(crate) fn combo(ui: &mut Ui, id: &str, label: &str, selected: &mut String, options: &[String], width: f32) {
    let shown = if selected.is_empty() { "Choose…".to_string() } else { selected.clone() };
    let r = craft_ui::ComboBox::from_id_salt(id).width(width).selected_text(shown).show_ui(ui, |ui| {
        for o in options {
            ui.selectable_value(selected, o.clone(), o);
        }
    });
    name(&r.response, label);
}

/// Inserts `text` at the end of `expr`, with a space when needed.
fn append(expr: &mut String, text: &str) {
    if !expr.is_empty() && !expr.ends_with([' ', '(', ',']) {
        expr.push(' ');
    }
    expr.push_str(text);
}

/// The "insert function" and "insert field" helpers under a formula box.
fn formula_helpers(ui: &mut Ui, id: &str, m: &Model, expr: &mut String, measures: bool) {
    ui.horizontal(|ui| {
        let mut pick = String::new();
        let r = craft_ui::ComboBox::from_id_salt((id, "fn")).width(170.0).selected_text("Insert function…").show_ui(ui, |ui| {
            for (name, template) in FUNCTIONS {
                if ui.selectable_label(false, *name).on_hover_text(*template).clicked() {
                    pick = (*template).to_string();
                }
            }
        });
        name(&r.response, "Insert function");
        let mut field = String::new();
        let r = craft_ui::ComboBox::from_id_salt((id, "field")).width(220.0).selected_text("Insert field…").show_ui(ui, |ui| {
            for t in &m.tables {
                ui.label(RichText::new(&t.name).strong());
                for c in &t.columns {
                    let f = format!("{}[{}]", quote(&t.name), c.name);
                    if ui.selectable_label(false, format!("   {f}")).clicked() {
                        field = f;
                    }
                }
            }
            if measures && !m.measures.is_empty() {
                ui.label(RichText::new("Measures").strong());
                for ms in &m.measures {
                    let f = format!("[{}]", ms.name);
                    if ui.selectable_label(false, format!("   {f}")).clicked() {
                        field = f;
                    }
                }
            }
        });
        name(&r.response, "Insert field");
        if !pick.is_empty() {
            append(expr, &pick);
        }
        if !field.is_empty() {
            append(expr, &field);
        }
    });
}

pub(crate) fn error_line(ui: &mut Ui, t: &Tokens, e: &Option<String>) {
    if let Some(e) = e {
        ui.colored_label(t.danger, e);
    }
}

pub(crate) fn window(title: &str) -> egui::Window<'static> {
    egui::Window::new(title.to_string()).collapsible(false).resizable(true).pivot(egui::Align2::CENTER_CENTER).default_pos(egui::pos2(0.0, 0.0))
}

/// Centres a form in the screen and keeps it inside it.
pub(crate) fn fit(w: egui::Window<'static>, ctx: &egui::Context, size: egui::Vec2) -> egui::Window<'static> {
    let screen = ctx.content_rect();
    let size = vec2(size.x.min(screen.width() - 40.0), size.y.min(screen.height() - 60.0));
    w.default_pos(screen.center()).default_size(size).max_height(screen.height() - 40.0).max_width(screen.width() - 20.0)
}

pub(super) fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut form) = app.powerpivot.form.take() else { return };
    let model = app.powerpivot.model(&app.session);
    let m = match model {
        Ok(m) => m,
        Err(e) => {
            toast(app, e);
            return;
        }
    };
    let t = Tokens::get(ctx);
    let keep = match &mut form {
        Form::Measure(f) => measure(app, ctx, &t, &m, f),
        Form::Column(f) => column(app, ctx, &t, &m, f),
        Form::Relationship(f) => relationship(app, ctx, &t, &m, f),
        Form::ManageRelationships => manage_relationships(app, ctx, &t, &m),
        Form::ManageMeasures => manage_measures(app, ctx, &t, &m),
        Form::DateTable(f) => date_table(app, ctx, &t, &m, f),
        Form::Pivot(f) => pivot(app, ctx, &t, &m, f),
    };
    // A form that opened another one (Manage → Edit) leaves that one in place.
    if keep && app.powerpivot.form.is_none() {
        app.powerpivot.form = Some(form);
    }
}

pub(crate) fn ok_cancel(ui: &mut Ui, ok_label: &str) -> (bool, bool) {
    let mut ok = false;
    let mut cancel = false;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            cancel = ui.button("Cancel").clicked();
            ok = ui.button(RichText::new(ok_label).strong()).clicked();
        });
    });
    (ok, cancel || ui.input(|i| i.key_pressed(egui::Key::Escape)))
}

fn measure(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model, f: &mut MeasureForm) -> bool {
    let mut keep = true;
    let title = if f.editing.is_some() { "Edit Measure" } else { "New Measure" };
    let tables: Vec<String> = m.tables.iter().map(|t| t.name.clone()).collect();
    fit(window(title), ctx, vec2(560.0, 420.0)).show(ctx, |ui| {
        egui::Grid::new("pp_measure_grid").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
            ui.label("Table");
            combo(ui, "pp_m_table", "Table", &mut f.table, &tables, 260.0);
            ui.end_row();
            ui.label("Measure name");
            ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(300.0).hint_text("Total Sales"));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.label("Formula");
        ui.add(
            egui::TextEdit::multiline(&mut f.expr)
                .font(egui::FontId::monospace(13.0))
                .desired_rows(6)
                .desired_width(f32::INFINITY)
                .hint_text("SUM(Sales[Amount])"),
        );
        formula_helpers(ui, "pp_m", m, &mut f.expr, true);
        ui.horizontal(|ui| {
            if ui.button("✓ Check Formula").on_hover_text("Evaluate the formula over the whole model").clicked() {
                f.check = Some(
                    m.evaluate(&f.expr)
                        .map(|v| display_value(&v, Some(&f.format).filter(|s| !s.is_empty()).map(String::as_str)))
                        .map_err(|e| e.to_string()),
                );
            }
            match &f.check {
                Some(Ok(v)) => {
                    ui.colored_label(t.error_check_green, format!("No errors · value over all data: {v}"));
                }
                Some(Err(e)) => {
                    ui.colored_label(t.danger, e);
                }
                None => {}
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Format");
            let shown = FORMATS.iter().find(|(_, c)| *c == f.format).map(|(l, _)| (*l).to_string()).unwrap_or_else(|| f.format.clone());
            let r = craft_ui::ComboBox::from_id_salt("pp_m_fmt").width(220.0).selected_text(shown).show_ui(ui, |ui| {
                for (label, code) in FORMATS {
                    ui.selectable_value(&mut f.format, (*code).to_string(), *label);
                }
            });
            name(&r.response, "Format");
            ui.add(egui::TextEdit::singleline(&mut f.format).desired_width(120.0).hint_text("custom code"));
        });
        error_line(ui, t, &f.error);
        let (ok, cancel) = ok_cancel(ui, "OK");
        if cancel {
            keep = false;
        }
        if ok {
            let name = f.name.trim().to_string();
            let params = match &f.editing {
                Some(old) => {
                    ("powerpivot.editMeasure", json!({ "name": old, "newName": name, "expression": f.expr, "table": f.table, "format": f.format }))
                }
                None => ("powerpivot.newMeasure", json!({ "table": f.table, "name": name, "expression": f.expr, "format": f.format })),
            };
            match app.run(params.0, params.1) {
                Ok(_) => {
                    app.powerpivot.selected = Selected::Measure(name);
                    app.powerpivot.calc_area = true;
                    app.powerpivot.table = f.table.clone();
                    keep = false;
                }
                Err(e) => f.error = Some(e),
            }
        }
    });
    keep
}

fn column(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model, f: &mut ColumnForm) -> bool {
    let mut keep = true;
    let title = if f.editing.is_some() { "Edit Calculated Column" } else { "Add Calculated Column" };
    let tables: Vec<String> = m.tables.iter().map(|t| t.name.clone()).collect();
    fit(window(title), ctx, vec2(560.0, 420.0)).show(ctx, |ui| {
        egui::Grid::new("pp_col_grid").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
            ui.label("Table");
            if f.editing.is_some() {
                ui.label(&f.table);
            } else {
                combo(ui, "pp_c_table", "Table", &mut f.table, &tables, 260.0);
            }
            ui.end_row();
            ui.label("Column name");
            ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(300.0).hint_text("Margin"));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.label("Formula (evaluated for each row; use RELATED to read a lookup table)");
        ui.add(
            egui::TextEdit::multiline(&mut f.expr)
                .font(egui::FontId::monospace(13.0))
                .desired_rows(4)
                .desired_width(f32::INFINITY)
                .hint_text("Sales[Amount] - Sales[Cost]"),
        );
        formula_helpers(ui, "pp_c", m, &mut f.expr, true);
        ui.horizontal(|ui| {
            if ui.button("✓ Check Formula").on_hover_text("Show the first rows' values").clicked() {
                let q = format!("EVALUATE TOPN(5, SELECTCOLUMNS({}, \"Value\", {}))", quote(&f.table), f.expr);
                f.check = Some(
                    m.query(&q)
                        .map(|r| r.rows.iter().filter_map(|row| row.first()).map(|v| display_value(v, None)).collect::<Vec<_>>().join(", "))
                        .map_err(|e| e.to_string()),
                );
            }
            match &f.check {
                Some(Ok(v)) => {
                    ui.colored_label(t.error_check_green, format!("No errors · first rows: {v}"));
                }
                Some(Err(e)) => {
                    ui.colored_label(t.danger, e);
                }
                None => {}
            }
        });
        error_line(ui, t, &f.error);
        let (ok, cancel) = ok_cancel(ui, "OK");
        if cancel {
            keep = false;
        }
        if ok {
            let expr = f.expr.trim().trim_start_matches('=').trim().to_string();
            let r = match &f.editing {
                Some(old) => app.run("powerpivot.editColumn", json!({ "table": f.table, "name": old, "newName": f.name, "expression": expr })),
                None => app.run("powerpivot.addColumn", json!({ "table": f.table, "name": f.name, "expression": expr })),
            };
            match r {
                Ok(_) => {
                    app.powerpivot.table = f.table.clone();
                    keep = false;
                }
                Err(e) => f.error = Some(e),
            }
        }
    });
    keep
}

fn relationship(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model, f: &mut RelForm) -> bool {
    let mut keep = true;
    let title = if f.editing.is_some() { "Edit Relationship" } else { "Create Relationship" };
    let tables: Vec<String> = m.tables.iter().map(|t| t.name.clone()).collect();
    let cols = |tn: &str| -> Vec<String> { m.table(tn).map(|t| t.columns.iter().map(|c| c.name.clone()).collect()).unwrap_or_default() };
    fit(window(title), ctx, vec2(560.0, 420.0)).show(ctx, |ui| {
        ui.label(RichText::new("Relate a column of a data table (many side) to the key column of a lookup table (one side). The engine works out which side is which from the data.").small().color(t.text_dim));
        ui.add_space(8.0);
        egui::Grid::new("pp_rel_grid").num_columns(3).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
            ui.label("");
            ui.label(RichText::new("Table").strong());
            ui.label(RichText::new("Column").strong());
            ui.end_row();
            ui.label("Data table");
            combo(ui, "pp_r_ft", "Data table", &mut f.ft, &tables, 200.0);
            combo(ui, "pp_r_fc", "Data table column", &mut f.fc, &cols(&f.ft), 200.0);
            ui.end_row();
            ui.label("Lookup table");
            combo(ui, "pp_r_tt", "Lookup table", &mut f.tt, &tables, 200.0);
            combo(ui, "pp_r_tc", "Lookup table column", &mut f.tc, &cols(&f.tt), 200.0);
            ui.end_row();
        });
        // Choosing the lookup table picks its same-named column.
        if f.tc.is_empty() && !f.fc.is_empty() && cols(&f.tt).iter().any(|c| c.eq_ignore_ascii_case(&f.fc)) {
            f.tc = cols(&f.tt).into_iter().find(|c| c.eq_ignore_ascii_case(&f.fc)).unwrap_or_default();
        }
        ui.add_space(6.0);
        ui.checkbox(&mut f.active, "Active (inactive relationships are used only through USERELATIONSHIP)");
        ui.checkbox(&mut f.both, "Filter in both directions");
        error_line(ui, t, &f.error);
        let (ok, cancel) = ok_cancel(ui, "OK");
        if cancel {
            keep = false;
        }
        if ok {
            let from = format!("{}[{}]", quote(&f.ft), f.fc);
            let to = format!("{}[{}]", quote(&f.tt), f.tc);
            let cross = if f.both { "both" } else { "single" };
            let same_ends = f.editing.as_ref().is_some_and(|(a, b, c, d)| a == &f.ft && b == &f.fc && c == &f.tt && d == &f.tc);
            let r = if same_ends {
                app.run("powerpivot.editRelationship", json!({ "from": from, "to": to, "active": f.active, "crossFilter": cross }))
            } else {
                let deleted = match &f.editing {
                    Some((a, b, c, d)) => app.run("powerpivot.deleteRelationship", json!({ "from": format!("{}[{}]", quote(a), b), "to": format!("{}[{}]", quote(c), d) })).map(|_| ()),
                    None => Ok(()),
                };
                deleted.and_then(|_| app.run("powerpivot.createRelationship", json!({ "from": from, "to": to, "active": f.active, "crossFilter": cross })))
            };
            match r {
                Ok(_) => keep = false,
                Err(e) => f.error = Some(e),
            }
        }
    });
    keep
}

fn rel_ends(m: &Model, r: &Relationship) -> (String, String) {
    (m.column_name(r.from_table, r.from_column), m.column_name(r.to_table, r.to_column))
}

fn manage_relationships(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model) -> bool {
    let mut keep = true;
    fit(window("Manage Relationships"), ctx, vec2(720.0, 420.0)).show(ctx, |ui| {
        if m.relationships.is_empty() {
            ui.label(
                RichText::new("No relationships yet. Create one, or use Detect Relationships to relate same-named key columns.").color(t.text_dim),
            );
        }
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            egui::Grid::new("pp_rels").num_columns(6).striped(true).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
                for h in ["Active", "Data table [column]", "", "Lookup table [column]", "Filter", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for r in &m.relationships {
                    let (from, to) = rel_ends(m, r);
                    let mut active = r.active;
                    if ui.checkbox(&mut active, "Active").changed() {
                        run(app, "powerpivot.editRelationship", json!({ "from": from, "to": to, "active": active }));
                    }
                    ui.label(&from);
                    ui.label("→");
                    ui.label(&to);
                    let both = r.cross_filter == CrossFilter::Both;
                    if ui.button(if both { "Both" } else { "Single" }).clicked() {
                        run(
                            app,
                            "powerpivot.editRelationship",
                            json!({ "from": from, "to": to, "crossFilter": if both { "single" } else { "both" } }),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Edit…").clicked() {
                            app.powerpivot.form = Some(Form::edit_relationship(m, r));
                        }
                        if ui.button("Delete").clicked() {
                            run(app, "powerpivot.deleteRelationship", json!({ "from": from, "to": to }));
                        }
                    });
                    ui.end_row();
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("New…").clicked() {
                app.powerpivot.form = Some(Form::new_relationship(&app.powerpivot.table.clone()));
            }
            if ui.button("Detect").clicked()
                && let Some(r) = run(app, "powerpivot.detectRelationships", json!({}))
            {
                let n = r["created"].as_array().map(Vec::len).unwrap_or(0);
                toast(app, if n == 0 { "No new relationships found".to_string() } else { format!("Created {n} relationship(s)") });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    keep = false;
                }
            });
        });
    });
    keep
}

fn manage_measures(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model) -> bool {
    let mut keep = true;
    let rows: Vec<(String, String, String)> = m
        .measures
        .iter()
        .map(|ms| (ms.name.clone(), m.tables.get(ms.table).map(|t| t.name.clone()).unwrap_or_default(), ms.expression.clone()))
        .collect();
    fit(window("Manage Measures"), ctx, vec2(760.0, 460.0)).show(ctx, |ui| {
        if rows.is_empty() {
            ui.label(RichText::new("No measures yet.").color(t.text_dim));
        }
        egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
            egui::Grid::new("pp_measures").num_columns(5).striped(true).spacing(vec2(12.0, 6.0)).show(ui, |ui| {
                for h in ["Measure", "Table", "Formula", "Value", ""] {
                    ui.label(RichText::new(h).strong());
                }
                ui.end_row();
                for (name, table, expr) in &rows {
                    ui.label(RichText::new(name).strong());
                    ui.label(table);
                    ui.add(egui::Label::new(RichText::new(expr).monospace()).truncate()).on_hover_text(expr);
                    let v = app.powerpivot.measure_value(m, name);
                    ui.label(v);
                    ui.horizontal(|ui| {
                        if ui.button("Edit…").clicked() {
                            app.powerpivot.form = Some(Form::edit_measure(m, name));
                        }
                        if ui.button("Delete").clicked() {
                            run(app, "powerpivot.deleteMeasure", json!({ "name": name }));
                        }
                    });
                    ui.end_row();
                }
            });
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("New…").clicked() {
                app.powerpivot.form = Some(Form::new_measure(&app.powerpivot.table.clone()));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    keep = false;
                }
            });
        });
    });
    keep
}

fn date_table(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model, f: &mut DateForm) -> bool {
    let mut keep = true;
    let options: Vec<String> = m
        .tables
        .iter()
        .flat_map(|tb| tb.columns.iter().filter(|c| c.data_type() == DataType::Number).map(move |c| format!("{}[{}]", quote(&tb.name), c.name)))
        .collect();
    fit(window("New Date Table"), ctx, vec2(480.0, 260.0)).show(ctx, |ui| {
        ui.label(RichText::new("Builds a calendar of whole years covering the dates in a column (Date, Year, Quarter, Month, Year Month, Day, Weekday), adds it to the model on a new sheet and relates it to that column. Use it with TOTALYTD, SAMEPERIODLASTYEAR and DATEADD.").small().color(t.text_dim));
        ui.add_space(8.0);
        egui::Grid::new("pp_date_grid").num_columns(2).spacing(vec2(10.0, 8.0)).show(ui, |ui| {
            ui.label("Date column");
            combo(ui, "pp_d_col", "Date column", &mut f.column, &options, 280.0);
            ui.end_row();
            ui.label("Table name");
            ui.text_edit_singleline(&mut f.name);
            ui.end_row();
        });
        error_line(ui, t, &f.error);
        let (ok, cancel) = ok_cancel(ui, "Create");
        if cancel {
            keep = false;
        }
        if ok {
            match app.run("powerpivot.newDateTable", json!({ "column": f.column, "name": f.name })) {
                Ok(r) => {
                    toast(app, format!("Created {} with {} days", f.name, r["rows"]));
                    keep = false;
                }
                Err(e) => f.error = Some(e),
            }
        }
    });
    keep
}

// ---------------------------------------------------------------- PivotTable builder

impl PivotForm {
    fn spec(&self) -> PivotSpec {
        PivotSpec {
            rows: self.rows.clone(),
            columns: self.columns.clone(),
            values: self.values.iter().map(ValueField::spec).collect(),
            filters: self.filters.iter().filter_map(filter_dax).collect(),
        }
    }
}

fn dax_literal(v: &str) -> String {
    if v.parse::<f64>().is_ok() { v.to_string() } else { format!("\"{}\"", v.replace('"', "\"\"")) }
}

fn filter_dax(f: &FilterField) -> Option<String> {
    if f.keep.is_empty() {
        return None;
    }
    Some(format!("{} IN {{{}}}", f.field, f.keep.iter().map(|v| dax_literal(v)).collect::<Vec<_>>().join(", ")))
}

fn field_values(m: &Model, field: &str) -> Vec<String> {
    let Ok(gridcraft_engine::datamodel::dax::Expr::Column { table, column }) = gridcraft_engine::datamodel::dax::parse(field) else { return vec![] };
    let Some(col) = m.table(&table).and_then(|t| t.column_index(&column).and_then(|c| t.column(c))) else { return vec![] };
    let mut vals: Vec<Value> = (0..col.cardinality() as u32).map(|id| col.dict_value(id).clone()).collect();
    vals.sort_by(gridcraft_engine::datamodel::value::sort_order);
    vals.iter().take(500).map(Value::to_text).collect()
}

fn chip_list(ui: &mut Ui, t: &Tokens, title: &str, items: &mut Vec<String>) {
    ui.label(RichText::new(title).strong());
    egui::Frame::NONE.fill(t.input_bg).stroke(egui::Stroke::new(1.0, t.input_border)).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
        ui.set_min_size(vec2(220.0, 70.0));
        let mut remove = None;
        let mut up = None;
        for (i, it) in items.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(it);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✕").clicked() {
                        remove = Some(i);
                    }
                    if i > 0 && ui.small_button("↑").clicked() {
                        up = Some(i);
                    }
                });
            });
        }
        if items.is_empty() {
            ui.label(RichText::new("—").color(t.text_dim));
        }
        if let Some(i) = remove {
            items.remove(i);
        }
        if let Some(i) = up {
            items.swap(i, i - 1);
        }
    });
}

fn pivot(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, m: &Model, f: &mut PivotForm) -> bool {
    let mut keep = true;
    fit(window("PivotTable from Data Model"), ctx, vec2(1080.0, 640.0)).show(ctx, |ui| {
        let list_h = (ctx.content_rect().height() - 360.0).clamp(160.0, 480.0);
        ui.horizontal_top(|ui| {
            // Field list.
            ui.vertical(|ui| {
                ui.set_width(300.0);
                ui.label(RichText::new("Fields").strong());
                ui.add(egui::TextEdit::singleline(&mut f.search).hint_text("Search fields").desired_width(290.0));
                let q = f.search.to_lowercase();
                egui::ScrollArea::vertical().id_salt("pp_fields").max_height(list_h).show(ui, |ui| {
                    for tb in &m.tables {
                        let ti = m.table_index(&tb.name);
                        egui::CollapsingHeader::new(RichText::new(tb.name.clone()).strong())
                            .id_salt(("pp_ft", &tb.name))
                            .default_open(m.tables.len() <= 4)
                            .show(ui, |ui| {
                                for ms in m.measures.iter().filter(|ms| Some(ms.table) == ti && (q.is_empty() || ms.name.to_lowercase().contains(&q)))
                                {
                                    let on = f.values.iter().any(|v| v.measure && v.source == ms.name);
                                    let mut c = on;
                                    if ui.checkbox(&mut c, format!("Σ {}", ms.name)).on_hover_text(&ms.expression).changed() {
                                        if c {
                                            f.values.push(ValueField { source: ms.name.clone(), measure: true, agg: 0 });
                                        } else {
                                            f.values.retain(|v| !(v.measure && v.source == ms.name));
                                        }
                                    }
                                }
                                for col in tb.columns.iter().filter(|c| q.is_empty() || c.name.to_lowercase().contains(&q)) {
                                    let field = format!("{}[{}]", quote(&tb.name), col.name);
                                    ui.horizontal(|ui| {
                                        ui.add(egui::Label::new(&col.name).truncate()).on_hover_text(&field);
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.small_button("Σ").on_hover_text("Add to Values (Sum; change the summary below)").clicked() {
                                                let agg = if col.data_type() == DataType::Number { 0 } else { 2 };
                                                f.values.push(ValueField { source: field.clone(), measure: false, agg });
                                            }
                                            if ui.small_button("F").on_hover_text("Add to Filters").clicked()
                                                && !f.filters.iter().any(|x| x.field == field)
                                            {
                                                f.filters.push(FilterField { field: field.clone(), keep: vec![] });
                                            }
                                            if ui.small_button("C").on_hover_text("Add to Columns").clicked() && !f.columns.contains(&field) {
                                                f.columns.push(field.clone());
                                            }
                                            if ui.small_button("R").on_hover_text("Add to Rows").clicked() && !f.rows.contains(&field) {
                                                f.rows.push(field.clone());
                                            }
                                        });
                                    });
                                }
                            });
                    }
                });
            });
            ui.add_space(14.0);
            // Areas.
            ui.vertical(|ui| {
                ui.set_width(250.0);
                egui::ScrollArea::vertical().id_salt("pp_areas").max_height(list_h + 30.0).show(ui, |ui| {
                    ui.label(RichText::new("Filters").strong());
                    let mut remove = None;
                    for (i, flt) in f.filters.iter_mut().enumerate() {
                        let all = field_values(m, &flt.field);
                        let summary = if flt.keep.is_empty() { "(All)".to_string() } else { format!("{} selected", flt.keep.len()) };
                        ui.horizontal(|ui| {
                            ui.label(&flt.field);
                            let r = craft_ui::ComboBox::from_id_salt(("pp_flt", i)).width(110.0).selected_text(summary).show_ui(ui, |ui| {
                                if ui.button("Select all").clicked() {
                                    flt.keep.clear();
                                }
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
                            name(&r.response, &format!("Filter values of {}", flt.field));
                            if ui.small_button("✕").clicked() {
                                remove = Some(i);
                            }
                        });
                    }
                    if f.filters.is_empty() {
                        ui.label(RichText::new("—").color(t.text_dim));
                    }
                    if let Some(i) = remove {
                        f.filters.remove(i);
                    }
                    ui.add_space(6.0);
                    chip_list(ui, t, "Columns", &mut f.columns);
                    ui.add_space(6.0);
                    chip_list(ui, t, "Rows", &mut f.rows);
                    ui.add_space(6.0);
                    ui.label(RichText::new("Values").strong());
                    egui::Frame::NONE.fill(t.input_bg).stroke(egui::Stroke::new(1.0, t.input_border)).corner_radius(4.0).inner_margin(6.0).show(
                        ui,
                        |ui| {
                            ui.set_min_size(vec2(220.0, 70.0));
                            let mut remove = None;
                            for (i, v) in f.values.iter_mut().enumerate() {
                                ui.horizontal(|ui| {
                                    if v.measure {
                                        ui.label(format!("Σ {}", v.source));
                                    } else {
                                        let shown = AGGS.get(v.agg).map(|a| a.0).unwrap_or("Sum");
                                        let r = craft_ui::ComboBox::from_id_salt(("pp_agg", i)).width(90.0).selected_text(shown).show_ui(ui, |ui| {
                                            for (k, (label, _)) in AGGS.iter().enumerate() {
                                                ui.selectable_value(&mut v.agg, k, *label);
                                            }
                                        });
                                        name(&r.response, &format!("Summarize {} by", v.source));
                                        ui.add(egui::Label::new(&v.source).truncate());
                                    }
                                    if ui.small_button("✕").clicked() {
                                        remove = Some(i);
                                    }
                                });
                            }
                            if f.values.is_empty() {
                                ui.label(RichText::new("Tick a measure, or press Σ next to a column").color(t.text_dim));
                            }
                            if let Some(i) = remove {
                                f.values.remove(i);
                            }
                        },
                    );
                });
            });
            ui.add_space(14.0);
            // Live preview.
            ui.vertical(|ui| {
                ui.label(RichText::new("Preview").strong());
                let spec = f.spec();
                let key = format!("{:?}|{:?}|{:?}|{:?}", spec.rows, spec.columns, spec.values, spec.filters);
                if f.preview.as_ref().is_none_or(|(k, _)| *k != key) {
                    let r = if spec.values.is_empty() {
                        Err("Add a value to see the PivotTable.".to_string())
                    } else {
                        layout(m, &spec).map_err(|e| e.to_string())
                    };
                    f.preview = Some((key, r));
                }
                match f.preview.as_ref().map(|(_, r)| r) {
                    Some(Ok(g)) => preview_grid(ui, t, g, list_h),
                    Some(Err(e)) => {
                        ui.label(RichText::new(e).color(t.text_dim));
                    }
                    None => {}
                }
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            ui.radio_value(&mut f.new_sheet, true, "New worksheet");
            ui.radio_value(&mut f.new_sheet, false, "Existing worksheet at");
            ui.add_enabled(!f.new_sheet, egui::TextEdit::singleline(&mut f.at).hint_text("Sheet1!H2").desired_width(140.0));
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
    keep
}

fn preview_grid(ui: &mut Ui, t: &Tokens, g: &PivotGrid, h: f32) {
    egui::ScrollArea::both().id_salt("pp_preview").max_height(h).auto_shrink([false, true]).show(ui, |ui| {
        let last = g.grid.len().saturating_sub(1);
        egui::Grid::new("pp_preview_grid").striped(true).spacing(vec2(14.0, 4.0)).show(ui, |ui| {
            for (r, line) in g.grid.iter().enumerate().take(200) {
                for (c, v) in line.iter().enumerate() {
                    let fmt = g.value_columns.get(c).copied().flatten().and_then(|k| g.value_formats.get(k).cloned().flatten());
                    let text = if r >= g.header_rows { display_value(v, fmt.as_deref()) } else { v.to_text() };
                    let rt = RichText::new(text);
                    let rt = if r < g.header_rows || r == last { rt.strong() } else { rt };
                    if matches!(v, Value::Number(_)) && r >= g.header_rows {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(rt));
                    } else {
                        ui.label(rt.color(if r < g.header_rows { t.accent } else { t.text }));
                    }
                }
                ui.end_row();
            }
        });
        if g.grid.len() > 200 {
            ui.label(RichText::new(format!("… {} more rows", g.grid.len() - 200)).color(t.text_dim));
        }
    });
}
