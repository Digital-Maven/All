//! The editor's dialogs. Each OK adds an M step (or creates/edits a query) through the
//! `powerquery.*` commands; an error keeps the dialog open with the message.

use egui::{RichText, Ui, vec2};
use gridcraft_engine::query::steps::{field, text_list};
use gridcraft_engine::query::value::quote_text;
use gridcraft_engine::query::{Table, Value};
use serde_json::{Value as Json, json};

use super::{add_step, open, ops, toast};
use crate::SheetApp;
use crate::powerpivot::forms::{combo, error_line, fit, name as a11y, ok_cancel, window};
use crate::theme::Tokens;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeepRemove {
    KeepTop,
    KeepBottom,
    KeepRange,
    RemoveTop,
    RemoveBottom,
    Alternate,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitMode {
    Delimiter,
    Chars,
    Positions,
    LowerUpper,
    UpperLower,
    DigitNon,
    NonDigit,
}

pub struct Dialog {
    pub kind: Kind,
    pub error: Option<String>,
}

pub enum Kind {
    GetData {
        kind: String,
        path: String,
        delimiter: String,
        table: String,
        items: Vec<(String, String)>,
        item: String,
        columns: String,
        rows: String,
        name: String,
    },
    Param {
        op: usize,
        add: bool,
        value: String,
    },
    Rows {
        kind: KeepRemove,
        a: String,
        b: String,
        c: String,
    },
    Split {
        col: String,
        mode: SplitMode,
        delim: String,
        custom: String,
        at: usize,
        n: String,
    },
    GroupBy {
        groups: Vec<String>,
        aggs: Vec<(String, usize, String)>,
    },
    Replace {
        cols: Vec<String>,
        find: String,
        replace: String,
        entire: bool,
    },
    ReplaceErrors {
        cols: Vec<String>,
        value: String,
    },
    Merge {
        as_new: bool,
        other: String,
        left: Vec<String>,
        right: Vec<String>,
        kind: usize,
    },
    Append {
        as_new: bool,
        picked: Vec<String>,
    },
    Parameters {
        selected: Option<String>,
        name: String,
        kind: String,
        value: String,
        description: String,
    },
    LoadTo {
        to: usize,
    },
    Advanced {
        m: String,
    },
    ChooseColumns {
        keep: Vec<(String, bool)>,
        search: String,
    },
    RenameColumn {
        old: String,
        new: String,
    },
    RenameQuery {
        old: String,
        new: String,
    },
    RenameStep {
        old: String,
        new: String,
    },
    Properties {
        name: String,
        description: String,
        group: String,
    },
    Custom {
        name: String,
        formula: String,
        ty: usize,
    },
    Conditional {
        name: String,
        rules: Vec<(String, usize, String, String)>,
        otherwise: String,
    },
    Pivot {
        col: String,
        values: String,
        agg: usize,
    },
    Index {
        start: String,
        step: String,
    },
    MergeColumns {
        cols: Vec<String>,
        sep: usize,
        custom: String,
        name: String,
        add: bool,
    },
    Expand {
        col: String,
        names: Vec<(String, bool)>,
        prefix: bool,
        record: bool,
    },
    Filter {
        col: String,
        values: Vec<(Value, bool)>,
        search: String,
        op: usize,
        value: String,
    },
}

fn d(kind: Kind) -> Dialog {
    Dialog { kind, error: None }
}

fn preview_table(app: &mut SheetApp) -> std::rc::Rc<Table> {
    app.powerquery.preview(&app.session).map(|p| p.table.clone()).unwrap_or_default()
}

impl Dialog {
    pub fn get_data(kind: &str) -> Dialog {
        d(Kind::GetData {
            kind: kind.into(),
            path: String::new(),
            delimiter: "Auto".into(),
            table: String::new(),
            items: vec![],
            item: String::new(),
            columns: "Column1, Column2".into(),
            rows: String::new(),
            name: String::new(),
        })
    }
    pub fn param(op: usize, add: bool) -> Dialog {
        let default = ops::OPS.get(op).and_then(|o| o.param).map(|p| p.1.to_string()).unwrap_or_default();
        d(Kind::Param { op, add, value: default })
    }
    pub fn rows(kind: KeepRemove) -> Dialog {
        d(Kind::Rows { kind, a: "1".into(), b: "1".into(), c: "1".into() })
    }
    pub fn split(col: &str, mode: SplitMode) -> Dialog {
        d(Kind::Split {
            col: col.into(),
            mode,
            delim: "Comma".into(),
            custom: String::new(),
            at: 2,
            n: if mode == SplitMode::Positions { "0, 3".into() } else { "1".into() },
        })
    }
    pub fn group_by(app: &mut SheetApp) -> Dialog {
        let g = app.powerquery.selected.clone();
        d(Kind::GroupBy { groups: if g.is_empty() { vec![String::new()] } else { g }, aggs: vec![("Count".into(), 0, String::new())] })
    }
    pub fn replace(cols: Vec<String>) -> Dialog {
        d(Kind::Replace { cols, find: String::new(), replace: String::new(), entire: false })
    }
    pub fn replace_errors(cols: Vec<String>) -> Dialog {
        d(Kind::ReplaceErrors { cols, value: "null".into() })
    }
    pub fn merge(app: &mut SheetApp, as_new: bool) -> Dialog {
        d(Kind::Merge { as_new, other: String::new(), left: app.powerquery.selected.clone(), right: vec![], kind: 0 })
    }
    pub fn append(as_new: bool) -> Dialog {
        d(Kind::Append { as_new, picked: vec![] })
    }
    pub fn parameters(app: &mut SheetApp, new: Option<String>) -> Dialog {
        let doc = app.powerquery.doc(&app.session);
        let sel = match &new {
            Some(n) if !n.is_empty() => Some(n.clone()),
            Some(_) => None,
            None => doc.parameters.first().map(|p| p.name.clone()),
        };
        let p = sel.as_ref().and_then(|n| doc.parameters.iter().find(|p| &p.name == n));
        d(Kind::Parameters {
            selected: sel.clone(),
            name: p.map(|p| p.name.clone()).unwrap_or_else(|| "Parameter1".into()),
            kind: p.map(|p| p.kind.clone()).unwrap_or_else(|| "Text".into()),
            value: p.map(|p| p.value.clone()).unwrap_or_else(|| "\"\"".into()),
            description: p.map(|p| p.description.clone()).unwrap_or_default(),
        })
    }
    pub fn load_to(app: &mut SheetApp) -> Dialog {
        let doc = app.powerquery.doc(&app.session);
        let to = match app.powerquery.current(&doc).map(|q| q.load) {
            Some(gridcraft_engine::query::steps::LoadTo::Model) => 1,
            Some(gridcraft_engine::query::steps::LoadTo::None) => 2,
            _ => 0,
        };
        d(Kind::LoadTo { to })
    }
    pub fn advanced(app: &mut SheetApp) -> Dialog {
        let doc = app.powerquery.doc(&app.session);
        d(Kind::Advanced { m: app.powerquery.current(&doc).map(|q| q.m.clone()).unwrap_or_default() })
    }
    pub fn choose_columns(app: &mut SheetApp) -> Dialog {
        let t = preview_table(app);
        d(Kind::ChooseColumns { keep: t.columns.iter().map(|c| (c.clone(), true)).collect(), search: String::new() })
    }
    pub fn rename_column(c: &str) -> Dialog {
        d(Kind::RenameColumn { old: c.into(), new: c.into() })
    }
    pub fn rename_query(q: &str) -> Dialog {
        d(Kind::RenameQuery { old: q.into(), new: q.into() })
    }
    pub fn rename_step(s: &str) -> Dialog {
        d(Kind::RenameStep { old: s.into(), new: s.into() })
    }
    pub fn properties(app: &mut SheetApp) -> Dialog {
        let doc = app.powerquery.doc(&app.session);
        let q = app.powerquery.current(&doc);
        d(Kind::Properties {
            name: app.powerquery.query.clone(),
            description: q.map(|q| q.description.clone()).unwrap_or_default(),
            group: q.and_then(|q| q.group.clone()).unwrap_or_default(),
        })
    }
    pub fn custom_column(_app: &mut SheetApp) -> Dialog {
        d(Kind::Custom { name: "Custom".into(), formula: String::new(), ty: 0 })
    }
    pub fn conditional(app: &mut SheetApp) -> Dialog {
        let first = app.powerquery.selected.first().cloned().or_else(|| preview_table(app).columns.first().cloned()).unwrap_or_default();
        d(Kind::Conditional { name: "Custom".into(), rules: vec![(first, 0, String::new(), String::new())], otherwise: String::new() })
    }
    pub fn pivot(app: &mut SheetApp, col: &str) -> Dialog {
        let t = preview_table(app);
        let values = t.columns.iter().find(|c| *c != col).cloned().unwrap_or_default();
        d(Kind::Pivot { col: col.into(), values, agg: 5 })
    }
    pub fn index() -> Dialog {
        d(Kind::Index { start: "1".into(), step: "1".into() })
    }
    pub fn merge_columns(cols: Vec<String>, add: bool) -> Dialog {
        d(Kind::MergeColumns { cols, sep: 0, custom: String::new(), name: "Merged".into(), add })
    }
    pub fn expand(app: &mut SheetApp, col: &str) -> Dialog {
        let t = preview_table(app);
        let mut names: Vec<String> = Vec::new();
        let mut record = false;
        if let Some(c) = t.column_index(col) {
            for r in t.rows.iter().take(200) {
                match r.get(c) {
                    Some(Value::Table(nt)) => {
                        for n in &nt.columns {
                            if !names.contains(n) {
                                names.push(n.clone());
                            }
                        }
                    }
                    Some(Value::Record(rec)) => {
                        record = true;
                        for n in &rec.names {
                            if !names.contains(n) {
                                names.push(n.clone());
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        d(Kind::Expand { col: col.into(), names: names.into_iter().map(|n| (n, true)).collect(), prefix: true, record })
    }
    pub fn filter(t: &Table, col: &str) -> Dialog {
        let mut vals: Vec<Value> = Vec::new();
        if let Some(c) = t.column_index(col) {
            let mut seen = std::collections::HashSet::new();
            for r in t.rows.iter().take(5000) {
                let v = r.get(c).cloned().unwrap_or_default();
                if !v.is_error() && seen.insert(gridcraft_engine::query::funcs::table::key_of(&[&v])) {
                    vals.push(v);
                }
                if vals.len() >= 1000 {
                    break;
                }
            }
        }
        vals.sort_by(gridcraft_engine::query::value::compare);
        d(Kind::Filter { col: col.into(), values: vals.into_iter().map(|v| (v, true)).collect(), search: String::new(), op: 0, value: String::new() })
    }
}

/// An M literal for typed text: numbers stay numbers, `null` is null, the rest is text.
fn literal(s: &str) -> String {
    let t = s.trim();
    if t == "null" {
        return "null".into();
    }
    if t.parse::<f64>().is_ok() && !t.starts_with('0') || t == "0" || t.starts_with("0.") {
        return t.to_string();
    }
    quote_text(s)
}

pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut dlg) = app.powerquery.dialog.take() else { return };
    let t = Tokens::get(ctx);
    let keep = match &mut dlg.kind {
        Kind::GetData { .. } => get_data(app, ctx, &t, &mut dlg),
        _ => edit_dialog(app, ctx, &t, &mut dlg),
    };
    if keep && app.powerquery.dialog.is_none() {
        app.powerquery.dialog = Some(dlg);
    }
}

/// Runs a command; an error goes to the dialog.
fn exec(app: &mut SheetApp, dlg: &mut Dialog, id: &str, p: Json) -> Option<Json> {
    match app.run(id, p) {
        Ok(v) => Some(v),
        Err(e) => {
            dlg.error = Some(e);
            None
        }
    }
}

fn path_row(app: &mut SheetApp, ui: &mut Ui, path: &mut String, folder: bool) {
    ui.horizontal(|ui| {
        ui.label(if folder { "Folder path" } else { "File path" });
        super::labeled(
            &ui.add(egui::TextEdit::singleline(path).desired_width(380.0).hint_text(if folder { "/Users/me/Reports" } else { "/Users/me/data.csv" })),
            "path",
        );
        if let Some(pick) = &app.services.pick_open
            && ui.button("Browse…").clicked()
            && let Some(p) = pick()
        {
            *path = if folder { std::path::Path::new(&p).parent().map(|x| x.to_string_lossy().to_string()).unwrap_or(p) } else { p };
        }
    });
}

fn get_data(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, dlg: &mut Dialog) -> bool {
    let mut keep = true;
    let tables: Vec<String> =
        app.session.doc().map(|d| d.wb.sheets.iter().flat_map(|s| s.tables.iter().map(|t| t.name.clone())).collect()).unwrap_or_default();
    let err = dlg.error.clone();
    let Kind::GetData { kind, path, delimiter, table, items, item, columns, rows, name } = &mut dlg.kind else { return false };
    let title = match kind.as_str() {
        "table" => "From Table/Range",
        "csv" => "From Text/CSV",
        "excel" => "From Workbook",
        "json" => "From JSON",
        "folder" => "From Folder",
        "text" => "From Text File",
        "web" => "From Web",
        "enter" => "Enter Data",
        _ => "Blank Query",
    };
    let mut source: Option<Json> = None;
    let mut cancel = false;
    let mut list_items = false;
    fit(window(title), ctx, vec2(620.0, 420.0)).show(ctx, |ui| {
        match kind.as_str() {
            "table" => {
                if tables.is_empty() {
                    ui.label("This workbook has no tables. Select your data and use Insert › Table first.");
                } else {
                    combo(ui, "pq_gd_table", "Table", table, &tables, 300.0);
                }
            }
            "csv" | "json" | "text" => {
                path_row(app, ui, path, false);
                if kind == "csv" {
                    ui.horizontal(|ui| {
                        ui.label("Delimiter");
                        let opts: Vec<String> = ["Auto", "Comma", "Semicolon", "Tab", "Pipe", "Space"].iter().map(|s| s.to_string()).collect();
                        combo(ui, "pq_gd_delim", "Delimiter", delimiter, &opts, 140.0);
                    });
                }
            }
            "folder" => {
                path_row(app, ui, path, true);
                ui.label(
                    RichText::new("Lists every file in the folder and its subfolders. Use Combine Files to append them.").small().color(t.text_dim),
                );
            }
            "excel" => {
                path_row(app, ui, path, false);
                if ui.button("Show sheets and tables").clicked() {
                    list_items = true;
                }
                if !items.is_empty() {
                    ui.label("Navigator: pick a sheet or table");
                    egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                        for (n, k) in items.iter() {
                            ui.radio_value(item, format!("{n}\u{1}{k}"), format!("{n}  ({k})"));
                        }
                    });
                }
            }
            "web" => {
                ui.horizontal(|ui| {
                    ui.label("URL");
                    super::labeled(&ui.add(egui::TextEdit::singleline(path).desired_width(420.0)), "path");
                });
                ui.label(
                    RichText::new(
                        "Web sources need GridCraft to download data, which it can't do yet; the query is created so you can use it when that lands.",
                    )
                    .small()
                    .color(t.text_dim),
                );
            }
            "enter" => {
                ui.horizontal(|ui| {
                    ui.label("Name");
                    super::labeled(&ui.text_edit_singleline(name), "name");
                });
                ui.horizontal(|ui| {
                    ui.label("Column names");
                    super::labeled(&ui.add(egui::TextEdit::singleline(columns).desired_width(420.0)), "columns");
                });
                ui.label("Rows (one per line; separate values with commas or tabs — paste from a sheet works)");
                super::labeled(
                    &ui.add(egui::TextEdit::multiline(rows).desired_rows(8).desired_width(f32::INFINITY).font(egui::FontId::monospace(12.0))),
                    "rows",
                );
            }
            _ => {
                ui.label("Creates an empty query; write it in the Advanced Editor or the formula bar.");
            }
        }
        error_line(ui, t, &err);
        let (ok, c) = ok_cancel(ui, "OK");
        cancel = c;
        if ok {
            source = Some(match kind.as_str() {
                "table" => json!({ "kind": "table", "table": table }),
                "csv" => {
                    let d = match delimiter.as_str() {
                        "Comma" => Some(","),
                        "Semicolon" => Some(";"),
                        "Tab" => Some("\t"),
                        "Pipe" => Some("|"),
                        "Space" => Some(" "),
                        _ => None,
                    };
                    let mut s = json!({ "kind": "csv", "path": path });
                    if let Some(d) = d {
                        s["delimiter"] = Json::String(d.into());
                    }
                    s
                }
                "excel" => {
                    let mut s = json!({ "kind": "excel", "path": path });
                    if let Some((n, k)) = item.split_once('\u{1}') {
                        s["item"] = Json::String(n.into());
                        s["itemKind"] = Json::String(k.into());
                    }
                    s
                }
                "json" | "folder" | "text" => json!({ "kind": kind, "path": path }),
                "web" => json!({ "kind": "web", "url": path }),
                "enter" => {
                    let cols: Vec<String> = columns.split([',', '\t']).map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect();
                    let data: Vec<Vec<Json>> = rows
                        .lines()
                        .filter(|l| !l.trim().is_empty())
                        .map(|l| {
                            let sep = if l.contains('\t') { '\t' } else { ',' };
                            l.split(sep)
                                .map(|v| v.trim())
                                .map(|v| {
                                    v.parse::<f64>()
                                        .ok()
                                        .and_then(serde_json::Number::from_f64)
                                        .map(Json::Number)
                                        .unwrap_or_else(|| Json::String(v.to_string()))
                                })
                                .collect()
                        })
                        .collect();
                    json!({ "kind": "enter", "columns": cols, "rows": data })
                }
                _ => json!({ "kind": "blank" }),
            });
        }
    });
    if list_items {
        let doc = app.powerquery.doc(&app.session);
        let m = format!("Excel.Workbook(File.Contents({}), null, true)", quote_text(path));
        let r = app.session.doc().ok().map(|d| gridcraft_engine::cmd::powerquery::evaluate_m(&d.wb, &doc, &m));
        match r {
            Some(Ok(Value::Table(tb))) => {
                *items = tb
                    .rows
                    .iter()
                    .map(|r| (r.first().map(Value::display).unwrap_or_default(), r.get(3).map(Value::display).unwrap_or_default()))
                    .collect();
                dlg.error = None;
            }
            Some(Err(e)) => dlg.error = Some(e),
            _ => {}
        }
    }
    if cancel {
        keep = false;
    }
    if let Some(src) = source {
        let mut p = json!({ "source": src });
        if !name.trim().is_empty() {
            p["name"] = Json::String(name.trim().to_string());
        }
        if let Some(r) = exec(app, dlg, "powerquery.newQuery", p) {
            open(app, r["name"].as_str());
            keep = false;
        }
    }
    keep
}

const SEPARATORS: &[(&str, &str)] =
    &[("None", ""), ("Colon", ":"), ("Comma", ","), ("Equals Sign", "="), ("Semicolon", ";"), ("Space", " "), ("Tab", "\t"), ("Custom", "")];
const AGG_OPS: &[&str] = &["Count Rows", "Count Distinct Rows", "Sum", "Average", "Median", "Min", "Max", "All Rows"];
const COND_OPS: &[&str] = &[
    "equals",
    "does not equal",
    "is greater than",
    "is greater than or equal to",
    "is less than",
    "is less than or equal to",
    "begins with",
    "does not begin with",
    "ends with",
    "does not end with",
    "contains",
    "does not contain",
];
const JOIN_KINDS: &[(&str, &str)] = &[
    ("Left Outer (all from first, matching from second)", "LeftOuter"),
    ("Right Outer (all from second, matching from first)", "RightOuter"),
    ("Full Outer (all rows from both)", "FullOuter"),
    ("Inner (only matching rows)", "Inner"),
    ("Left Anti (rows only in first)", "LeftAnti"),
    ("Right Anti (rows only in second)", "RightAnti"),
];
const PIVOT_AGGS: &[(&str, &str)] = &[
    ("Count (All)", "List.Count"),
    ("Count (Not Blank)", "List.NonNullCount"),
    ("Minimum", "List.Min"),
    ("Maximum", "List.Max"),
    ("Median", "List.Median"),
    ("Sum", "List.Sum"),
    ("Average", "List.Average"),
    ("Don't Aggregate", ""),
];
const CUSTOM_TYPES: &[(&str, &str)] = &[
    ("Any", ""),
    ("Text", "type text"),
    ("Decimal Number", "type number"),
    ("Whole Number", "Int64.Type"),
    ("Date", "type date"),
    ("True/False", "type logical"),
];

fn index_combo(ui: &mut Ui, id: &str, label: &str, sel: &mut usize, options: &[&str], width: f32) {
    let shown = options.get(*sel).copied().unwrap_or_default();
    let r = craft_ui::ComboBox::from_id_salt(id).width(width).selected_text(shown).show_ui(ui, |ui| {
        for (i, o) in options.iter().enumerate() {
            ui.selectable_value(sel, i, *o);
        }
    });
    a11y(&r.response, label);
}

fn cond_expr(col: &str, op: usize, value: &str) -> String {
    let f = field(col);
    let v = literal(value);
    let tv = quote_text(value);
    match op {
        0 => format!("{f} = {v}"),
        1 => format!("{f} <> {v}"),
        2 => format!("{f} > {v}"),
        3 => format!("{f} >= {v}"),
        4 => format!("{f} < {v}"),
        5 => format!("{f} <= {v}"),
        6 => format!("Text.StartsWith(Text.From({f}), {tv})"),
        7 => format!("not Text.StartsWith(Text.From({f}), {tv})"),
        8 => format!("Text.EndsWith(Text.From({f}), {tv})"),
        9 => format!("not Text.EndsWith(Text.From({f}), {tv})"),
        10 => format!("Text.Contains(Text.From({f}), {tv})"),
        _ => format!("not Text.Contains(Text.From({f}), {tv})"),
    }
}

/// The dialogs that edit the current query.
fn edit_dialog(app: &mut SheetApp, ctx: &egui::Context, t: &Tokens, dlg: &mut Dialog) -> bool {
    let table = preview_table(app);
    let cols: Vec<String> = table.columns.clone();
    let doc = app.powerquery.doc(&app.session);
    let queries: Vec<String> = doc.queries.iter().map(|q| q.name.clone()).collect();
    let err = dlg.error.clone();
    let mut keep = true;
    let mut action: Option<Commit> = None;
    let title = title_of(&dlg.kind);
    fit(window(title), ctx, vec2(560.0, 360.0)).show(ctx, |ui| {
        body(app, ui, t, &mut dlg.kind, &cols, &queries, &table);
        error_line(ui, t, &err);
        let ok_label = match &dlg.kind {
            Kind::Advanced { .. } => "Done",
            Kind::Merge { .. } | Kind::Append { .. } => "OK",
            _ => "OK",
        };
        let (ok, cancel) = ok_cancel(ui, ok_label);
        if cancel {
            keep = false;
        }
        if ok {
            action = Some(commit(&dlg.kind, &table));
        }
    });
    if let Some(a) = action
        && a(app, dlg)
    {
        keep = false;
    }
    let _ = doc;
    keep
}

fn title_of(k: &Kind) -> &'static str {
    match k {
        Kind::GetData { .. } => "Get Data",
        Kind::Param { .. } => "Enter a Value",
        Kind::Rows { kind, .. } => match kind {
            KeepRemove::KeepTop => "Keep Top Rows",
            KeepRemove::KeepBottom => "Keep Bottom Rows",
            KeepRemove::KeepRange => "Keep Range of Rows",
            KeepRemove::RemoveTop => "Remove Top Rows",
            KeepRemove::RemoveBottom => "Remove Bottom Rows",
            KeepRemove::Alternate => "Remove Alternate Rows",
        },
        Kind::Split { .. } => "Split Column",
        Kind::GroupBy { .. } => "Group By",
        Kind::Replace { .. } => "Replace Values",
        Kind::ReplaceErrors { .. } => "Replace Errors",
        Kind::Merge { .. } => "Merge",
        Kind::Append { .. } => "Append",
        Kind::Parameters { .. } => "Manage Parameters",
        Kind::LoadTo { .. } => "Load To",
        Kind::Advanced { .. } => "Advanced Editor",
        Kind::ChooseColumns { .. } => "Choose Columns",
        Kind::RenameColumn { .. } => "Rename Column",
        Kind::RenameQuery { .. } => "Rename Query",
        Kind::RenameStep { .. } => "Rename Step",
        Kind::Properties { .. } => "Query Properties",
        Kind::Custom { .. } => "Custom Column",
        Kind::Conditional { .. } => "Add Conditional Column",
        Kind::Pivot { .. } => "Pivot Column",
        Kind::Index { .. } => "Add Index Column",
        Kind::MergeColumns { .. } => "Merge Columns",
        Kind::Expand { .. } => "Expand",
        Kind::Filter { .. } => "Filter Rows",
    }
}

#[allow(clippy::too_many_arguments)]
fn body(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, k: &mut Kind, cols: &[String], queries: &[String], table: &Table) {
    match k {
        Kind::Param { op, value, .. } => {
            let prompt = ops::OPS.get(*op).and_then(|o| o.param).map(|p| p.0).unwrap_or("Value");
            ui.label(prompt);
            super::labeled(&ui.text_edit_singleline(value), "value");
        }
        Kind::Rows { kind, a, b, c } => match kind {
            KeepRemove::KeepRange => {
                ui.horizontal(|ui| {
                    ui.label("First row");
                    super::labeled(&ui.text_edit_singleline(a), "a");
                });
                ui.horizontal(|ui| {
                    ui.label("Number of rows");
                    super::labeled(&ui.text_edit_singleline(b), "b");
                });
            }
            KeepRemove::Alternate => {
                ui.label("Specify the pattern of rows to remove and keep.");
                ui.horizontal(|ui| {
                    ui.label("First row to remove");
                    super::labeled(&ui.text_edit_singleline(a), "a");
                });
                ui.horizontal(|ui| {
                    ui.label("Number of rows to remove");
                    super::labeled(&ui.text_edit_singleline(b), "b");
                });
                ui.horizontal(|ui| {
                    ui.label("Number of rows to keep");
                    super::labeled(&ui.text_edit_singleline(c), "c");
                });
            }
            _ => {
                ui.horizontal(|ui| {
                    ui.label("Number of rows");
                    super::labeled(&ui.text_edit_singleline(a), "a");
                });
            }
        },
        Kind::Split { col, mode, delim, custom, at, n } => {
            ui.label(format!("Split column {col}"));
            match mode {
                SplitMode::Delimiter => {
                    let opts: Vec<String> =
                        ["Comma", "Colon", "Equals Sign", "Semicolon", "Space", "Tab", "Custom"].iter().map(|s| s.to_string()).collect();
                    ui.horizontal(|ui| {
                        ui.label("Select or enter delimiter");
                        combo(ui, "pq_split_delim", "Delimiter", delim, &opts, 160.0);
                        if delim == "Custom" {
                            super::labeled(&ui.text_edit_singleline(custom), "custom");
                        }
                    });
                    ui.label("Split at");
                    ui.radio_value(at, 0, "Left-most delimiter");
                    ui.radio_value(at, 1, "Right-most delimiter");
                    ui.radio_value(at, 2, "Each occurrence of the delimiter");
                }
                SplitMode::Chars => {
                    ui.horizontal(|ui| {
                        ui.label("Number of characters");
                        super::labeled(&ui.text_edit_singleline(n), "n");
                    });
                    ui.radio_value(at, 0, "Once, as far left as possible");
                    ui.radio_value(at, 2, "Repeatedly");
                }
                SplitMode::Positions => {
                    ui.horizontal(|ui| {
                        ui.label("Positions");
                        super::labeled(&ui.text_edit_singleline(n), "n");
                    });
                    ui.label(RichText::new("Zero-based character positions, e.g. 0, 3, 7").small().color(t.text_dim));
                }
                _ => {
                    ui.label("Splits where the kind of character changes.");
                }
            }
        }
        Kind::GroupBy { groups, aggs } => {
            ui.label("Specify the columns to group by and one or more outputs.");
            let mut remove_g = None;
            for (i, g) in groups.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    combo(ui, &format!("pq_gb_g{i}"), "Group by column", g, cols, 220.0);
                    if i > 0 && ui.small_button("×").clicked() {
                        remove_g = Some(i);
                    }
                });
            }
            if let Some(i) = remove_g {
                groups.remove(i);
            }
            if ui.small_button("Add grouping").clicked() {
                groups.push(String::new());
            }
            ui.separator();
            let mut remove_a = None;
            for (i, (name, op, col)) in aggs.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    super::labeled(&ui.add(egui::TextEdit::singleline(name).desired_width(120.0).hint_text("New column name")), "name");
                    index_combo(ui, &format!("pq_gb_op{i}"), "Operation", op, AGG_OPS, 150.0);
                    if *op >= 2 && *op <= 6 {
                        combo(ui, &format!("pq_gb_c{i}"), "Column", col, cols, 160.0);
                    }
                    if i > 0 && ui.small_button("×").clicked() {
                        remove_a = Some(i);
                    }
                });
            }
            if let Some(i) = remove_a {
                aggs.remove(i);
            }
            if ui.small_button("Add aggregation").clicked() {
                aggs.push((format!("Aggregation{}", aggs.len()), 2, String::new()));
            }
        }
        Kind::Replace { cols: c, find, replace, entire } => {
            ui.label(format!("Replace one value with another in {}", c.join(", ")));
            ui.horizontal(|ui| {
                ui.label("Value To Find");
                super::labeled(&ui.text_edit_singleline(find), "find");
            });
            ui.horizontal(|ui| {
                ui.label("Replace With");
                super::labeled(&ui.text_edit_singleline(replace), "replace");
            });
            ui.checkbox(entire, "Match entire cell contents");
        }
        Kind::ReplaceErrors { value, .. } => {
            ui.horizontal(|ui| {
                ui.label("Value");
                super::labeled(&ui.text_edit_singleline(value), "value");
            });
        }
        Kind::Merge { as_new, other, left, right, kind } => merge_body(app, ui, t, *as_new, other, left, right, kind, queries, table),
        Kind::Append { picked, .. } => {
            ui.label("Choose the queries to append to the current one (in order).");
            for q in queries.iter().filter(|q| **q != app.powerquery.query) {
                let mut on = picked.contains(q);
                if ui.checkbox(&mut on, q).changed() {
                    if on {
                        picked.push(q.clone());
                    } else {
                        picked.retain(|x| x != q);
                    }
                }
            }
        }
        Kind::Parameters { selected, name, kind, value, description } => {
            let doc = app.powerquery.doc(&app.session);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(160.0);
                    for p in &doc.parameters {
                        if ui.selectable_label(selected.as_ref() == Some(&p.name), &p.name).clicked() {
                            *selected = Some(p.name.clone());
                            *name = p.name.clone();
                            *kind = p.kind.clone();
                            *value = p.value.clone();
                            *description = p.description.clone();
                        }
                    }
                    if ui.button("New").clicked() {
                        *selected = None;
                        *name = format!("Parameter{}", doc.parameters.len() + 1);
                        *value = "\"\"".into();
                        description.clear();
                    }
                    if let Some(s) = selected.clone()
                        && ui.button("Delete").clicked()
                        && super::run(app, "powerquery.deleteParameter", json!({ "name": s })).is_some()
                    {
                        *selected = None;
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    egui::Grid::new("pq_params").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                        ui.label("Name");
                        super::labeled(&ui.text_edit_singleline(name), "name");
                        ui.end_row();
                        ui.label("Description");
                        super::labeled(&ui.text_edit_singleline(description), "description");
                        ui.end_row();
                        ui.label("Type");
                        let opts: Vec<String> =
                            ["Text", "Decimal Number", "Whole Number", "Date", "True/False", "Any"].iter().map(|s| s.to_string()).collect();
                        combo(ui, "pq_param_type", "Type", kind, &opts, 160.0);
                        ui.end_row();
                        ui.label("Current Value (M)");
                        super::labeled(&ui.text_edit_singleline(value), "value");
                        ui.end_row();
                    });
                    ui.label(
                        RichText::new(
                            "Text in quotes (\"East\"), numbers as is (100), dates as #date(2024, 1, 31). Queries use a parameter by its name.",
                        )
                        .small()
                        .color(t.text_dim),
                    );
                });
            });
        }
        Kind::LoadTo { to } => {
            ui.label("Select how you want to use this query's data.");
            ui.radio_value(to, 0, "Table on a new worksheet");
            ui.radio_value(to, 1, "Data Model only (Power BI style: use it in Power Pivot measures and PivotTables)");
            ui.radio_value(to, 2, "Only Create Connection");
        }
        Kind::Advanced { m } => {
            ui.label(RichText::new(&app.powerquery.query).strong());
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                super::labeled(&ui.add(egui::TextEdit::multiline(m).code_editor().desired_rows(18).desired_width(f32::INFINITY)), "m");
            });
            match gridcraft_engine::query::m::parse(m) {
                Ok(_) => {
                    ui.colored_label(t.error_check_green, "✓ No syntax errors have been detected.");
                }
                Err(e) => {
                    ui.colored_label(t.danger, e.message());
                }
            }
        }
        Kind::ChooseColumns { keep, search } => {
            super::labeled(&ui.add(egui::TextEdit::singleline(search).hint_text("Search columns")), "search");
            let all = keep.iter().all(|(_, k)| *k);
            let mut a = all;
            if ui.checkbox(&mut a, "(Select All Columns)").changed() {
                for (_, k) in keep.iter_mut() {
                    *k = a;
                }
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                let q = search.to_lowercase();
                for (c, k) in keep.iter_mut().filter(|(c, _)| q.is_empty() || c.to_lowercase().contains(&q)) {
                    ui.checkbox(k, c.as_str());
                }
            });
        }
        Kind::RenameColumn { new, .. } | Kind::RenameQuery { new, .. } | Kind::RenameStep { new, .. } => {
            ui.label("New name");
            {
                let r = ui.text_edit_singleline(new);
                super::labeled(&r, "New name");
                r.request_focus();
            }
        }
        Kind::Properties { name, description, group } => {
            egui::Grid::new("pq_props").num_columns(2).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                ui.label("Name");
                super::labeled(&ui.text_edit_singleline(name), "name");
                ui.end_row();
                ui.label("Description");
                super::labeled(&ui.text_edit_multiline(description), "Description");
                ui.end_row();
                ui.label("Group");
                super::labeled(&ui.text_edit_singleline(group), "group");
                ui.end_row();
            });
        }
        Kind::Custom { name, formula, ty } => {
            ui.horizontal(|ui| {
                ui.label("New column name");
                super::labeled(&ui.text_edit_singleline(name), "name");
                ui.label("Type");
                let labels: Vec<&str> = CUSTOM_TYPES.iter().map(|x| x.0).collect();
                index_combo(ui, "pq_custom_ty", "Type", ty, &labels, 140.0);
            });
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.label("Custom column formula");
                    super::labeled(
                        &ui.add(egui::TextEdit::multiline(formula).code_editor().desired_rows(8).desired_width(360.0).hint_text("[Units] * [Price]")),
                        "formula",
                    );
                });
                ui.vertical(|ui| {
                    ui.label("Available columns");
                    egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
                        for c in cols {
                            if ui.selectable_label(false, c).on_hover_text("Insert").clicked() {
                                formula.push_str(&field(c));
                            }
                        }
                    });
                });
            });
            if !formula.trim().is_empty() {
                match gridcraft_engine::query::m::parse(&format!("each {formula}")) {
                    Ok(_) => {
                        ui.colored_label(t.error_check_green, "✓ No syntax errors have been detected.");
                    }
                    Err(e) => {
                        ui.colored_label(t.danger, e.message());
                    }
                }
            }
        }
        Kind::Conditional { name, rules, otherwise } => {
            ui.horizontal(|ui| {
                ui.label("New column name");
                super::labeled(&ui.text_edit_singleline(name), "name");
            });
            let mut remove = None;
            for (i, (col, op, value, out)) in rules.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(if i == 0 { "If" } else { "Else If" });
                    combo(ui, &format!("pq_cond_c{i}"), "Column", col, cols, 140.0);
                    index_combo(ui, &format!("pq_cond_o{i}"), "Operator", op, COND_OPS, 150.0);
                    super::labeled(&ui.add(egui::TextEdit::singleline(value).desired_width(90.0).hint_text("Value")), "value");
                    ui.label("Then");
                    super::labeled(&ui.add(egui::TextEdit::singleline(out).desired_width(90.0).hint_text("Output")), "out");
                    if i > 0 && ui.small_button("×").clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                rules.remove(i);
            }
            if ui.small_button("Add Clause").clicked() {
                let c = rules.last().map(|r| r.0.clone()).unwrap_or_default();
                rules.push((c, 0, String::new(), String::new()));
            }
            ui.horizontal(|ui| {
                ui.label("Else");
                super::labeled(&ui.add(egui::TextEdit::singleline(otherwise).desired_width(120.0).hint_text("null")), "otherwise");
            });
        }
        Kind::Pivot { col, values, agg } => {
            ui.label(format!("Use the names in column {col} to create new columns."));
            let others: Vec<String> = cols.iter().filter(|c| *c != col).cloned().collect();
            ui.horizontal(|ui| {
                ui.label("Values Column");
                combo(ui, "pq_pivot_v", "Values column", values, &others, 200.0);
            });
            ui.horizontal(|ui| {
                ui.label("Aggregate Value Function");
                let labels: Vec<&str> = PIVOT_AGGS.iter().map(|x| x.0).collect();
                index_combo(ui, "pq_pivot_a", "Aggregate", agg, &labels, 180.0);
            });
        }
        Kind::Index { start, step } => {
            ui.horizontal(|ui| {
                ui.label("Starting Index");
                super::labeled(&ui.text_edit_singleline(start), "start");
            });
            ui.horizontal(|ui| {
                ui.label("Increment");
                super::labeled(&ui.text_edit_singleline(step), "step");
            });
        }
        Kind::MergeColumns { cols: c, sep, custom, name, .. } => {
            ui.label(format!("Merge {} in the selected order.", c.join(", ")));
            ui.horizontal(|ui| {
                ui.label("Separator");
                let labels: Vec<&str> = SEPARATORS.iter().map(|x| x.0).collect();
                index_combo(ui, "pq_mc_sep", "Separator", sep, &labels, 140.0);
                if *sep == SEPARATORS.len() - 1 {
                    super::labeled(&ui.text_edit_singleline(custom), "custom");
                }
            });
            ui.horizontal(|ui| {
                ui.label("New column name (optional)");
                super::labeled(&ui.text_edit_singleline(name), "name");
            });
        }
        Kind::Expand { names, prefix, .. } => {
            if names.is_empty() {
                ui.label("This column holds no tables or records to expand.");
            }
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (n, on) in names.iter_mut() {
                    ui.checkbox(on, n.as_str());
                }
            });
            ui.checkbox(prefix, "Use original column name as prefix");
        }
        Kind::Filter { col, values, search, op, value } => {
            ui.label(RichText::new(format!("Filter {col}")).strong());
            ui.horizontal(|ui| {
                if ui.button("Sort Ascending").clicked() {
                    app.powerquery.selected = vec![col.clone()];
                    ops::sort(app, false);
                }
                if ui.button("Sort Descending").clicked() {
                    app.powerquery.selected = vec![col.clone()];
                    ops::sort(app, true);
                }
                if ui.button("Remove Empty").clicked() {
                    let f = field(col);
                    add_step(app, "Filtered Rows", &format!("Table.SelectRows({{prev}}, each {f} <> null and {f} <> \"\")"));
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Condition");
                let labels: Vec<&str> = std::iter::once("(none — use the list)").chain(COND_OPS.iter().copied()).collect();
                index_combo(ui, "pq_filter_op", "Condition", op, &labels, 190.0);
                if *op > 0 {
                    super::labeled(&ui.text_edit_singleline(value), "value");
                }
            });
            if *op == 0 {
                super::labeled(&ui.add(egui::TextEdit::singleline(search).hint_text("Search")), "search");
                let all = values.iter().all(|(_, k)| *k);
                let mut a = all;
                if ui.checkbox(&mut a, "(Select All)").changed() {
                    for (_, k) in values.iter_mut() {
                        *k = a;
                    }
                }
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    let q = search.to_lowercase();
                    for (v, k) in values.iter_mut().filter(|(v, _)| q.is_empty() || v.display().to_lowercase().contains(&q)) {
                        let label = if v.is_null() {
                            "(null)".to_string()
                        } else if v.is_empty_cell() {
                            "(blank)".to_string()
                        } else {
                            v.display()
                        };
                        ui.checkbox(k, label);
                    }
                });
            }
        }
        Kind::GetData { .. } => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn merge_body(
    app: &mut SheetApp,
    ui: &mut Ui,
    t: &Tokens,
    as_new: bool,
    other: &mut String,
    left: &mut Vec<String>,
    right: &mut Vec<String>,
    kind: &mut usize,
    queries: &[String],
    table: &Table,
) {
    ui.label(format!("Select tables and matching columns to create a merged {}.", if as_new { "query" } else { "table" }));
    let pick = |ui: &mut Ui, id: &str, tb: &Table, keys: &mut Vec<String>| {
        egui::ScrollArea::horizontal().id_salt(id).show(ui, |ui| {
            egui::Grid::new(id).striped(true).show(ui, |ui| {
                for c in &tb.columns {
                    let i = keys.iter().position(|k| k == c);
                    let label = match i {
                        Some(i) => format!("[{}] {c}", i + 1),
                        None => c.clone(),
                    };
                    if ui.selectable_label(i.is_some(), label).on_hover_text("Click to use as a key (in order)").clicked() {
                        match i {
                            Some(i) => {
                                keys.remove(i);
                            }
                            None => keys.push(c.clone()),
                        }
                    }
                }
                ui.end_row();
                for r in tb.rows.iter().take(5) {
                    for v in r {
                        ui.label(RichText::new(v.display()).small());
                    }
                    ui.end_row();
                }
            });
        });
    };
    ui.label(RichText::new(&app.powerquery.query).strong());
    pick(ui, "pq_merge_l", table, left);
    ui.add_space(6.0);
    combo(ui, "pq_merge_other", "Second table", other, queries, 260.0);
    let doc = app.powerquery.doc(&app.session);
    let other_t = app.session.doc().ok().and_then(|d| gridcraft_engine::cmd::powerquery::evaluate_query(&d.wb, &doc, other, None).ok());
    if let Some(Value::Table(ot)) = &other_t {
        pick(ui, "pq_merge_r", ot, right);
        // How many rows match.
        let lk: Vec<usize> = left.iter().filter_map(|c| table.column_index(c)).collect();
        let rk: Vec<usize> = right.iter().filter_map(|c| ot.column_index(c)).collect();
        if !lk.is_empty() && lk.len() == rk.len() {
            let keys: std::collections::HashSet<String> = (0..ot.rows.len())
                .map(|r| gridcraft_engine::query::funcs::table::key_of(&rk.iter().map(|c| ot.cell(r, *c)).collect::<Vec<_>>()))
                .collect();
            let hits = (0..table.rows.len())
                .filter(|r| keys.contains(&gridcraft_engine::query::funcs::table::key_of(&lk.iter().map(|c| table.cell(*r, *c)).collect::<Vec<_>>())))
                .count();
            ui.colored_label(t.error_check_green, format!("The selection matches {hits} of {} rows from the first table.", table.rows.len()));
        }
    }
    ui.horizontal(|ui| {
        ui.label("Join Kind");
        let labels: Vec<&str> = JOIN_KINDS.iter().map(|x| x.0).collect();
        index_combo(ui, "pq_merge_kind", "Join kind", kind, &labels, 360.0);
    });
}

type Commit = Box<dyn FnOnce(&mut SheetApp, &mut Dialog) -> bool>;

/// What OK does: returns true when the dialog can close.
fn commit(k: &Kind, table: &Table) -> Commit {
    match k {
        Kind::Param { op, add, value } => {
            let (op, add, value) = (*op, *add, value.clone());
            Box::new(move |app, _| {
                ops::apply_op(app, op, add, Some(value));
                true
            })
        }
        Kind::Rows { kind, a, b, c } => {
            let n = |s: &str| s.trim().parse::<i64>().unwrap_or(1).max(0);
            let (a, b, c) = (n(a), n(b), n(c));
            let (name, e) = match kind {
                KeepRemove::KeepTop => ("Kept First Rows", format!("Table.FirstN({{prev}}, {a})")),
                KeepRemove::KeepBottom => ("Kept Last Rows", format!("Table.LastN({{prev}}, {a})")),
                KeepRemove::KeepRange => ("Kept Range of Rows", format!("Table.Range({{prev}}, {}, {b})", (a - 1).max(0))),
                KeepRemove::RemoveTop => ("Removed Top Rows", format!("Table.Skip({{prev}}, {a})")),
                KeepRemove::RemoveBottom => ("Removed Bottom Rows", format!("Table.RemoveLastN({{prev}}, {a})")),
                KeepRemove::Alternate => ("Removed Alternate Rows", format!("Table.AlternateRows({{prev}}, {}, {b}, {c})", (a - 1).max(0))),
            };
            Box::new(move |app, _| {
                add_step(app, name, &e);
                true
            })
        }
        Kind::Split { col, mode, delim, custom, at, n } => {
            let d = match delim.as_str() {
                "Comma" => ",",
                "Colon" => ":",
                "Equals Sign" => "=",
                "Semicolon" => ";",
                "Space" => " ",
                "Tab" => "\t",
                _ => custom.as_str(),
            };
            let splitter = match mode {
                SplitMode::Delimiter => match at {
                    0 => format!("Splitter.SplitTextByEachDelimiter({{{}}}, QuoteStyle.Csv, false)", quote_text(d)),
                    1 => format!("Splitter.SplitTextByEachDelimiter({{{}}}, QuoteStyle.Csv, true)", quote_text(d)),
                    _ => format!("Splitter.SplitTextByDelimiter({}, QuoteStyle.Csv)", quote_text(d)),
                },
                SplitMode::Chars => {
                    let k = n.trim().parse::<usize>().unwrap_or(1).max(1);
                    if *at == 0 {
                        format!("Splitter.SplitTextByPositions({{0, {k}}})")
                    } else {
                        let longest = table
                            .column_index(col)
                            .map(|c| {
                                table.rows.iter().take(1000).map(|r| r.get(c).map(|v| v.display().chars().count()).unwrap_or(0)).max().unwrap_or(0)
                            })
                            .unwrap_or(0);
                        let parts = longest.div_ceil(k).clamp(1, 200);
                        format!("Splitter.SplitTextByLengths({{{}}})", vec![k.to_string(); parts].join(", "))
                    }
                }
                SplitMode::Positions => format!("Splitter.SplitTextByPositions({{{}}})", n),
                SplitMode::LowerUpper => "Splitter.SplitTextByCharacterTransition({\"a\"..\"z\"}, {\"A\"..\"Z\"})".into(),
                SplitMode::UpperLower => "Splitter.SplitTextByCharacterTransition({\"A\"..\"Z\"}, {\"a\"..\"z\"})".into(),
                SplitMode::DigitNon => "Splitter.SplitTextByCharacterTransition({\"0\"..\"9\"}, (c) => not List.Contains({\"0\"..\"9\"}, c))".into(),
                SplitMode::NonDigit => "Splitter.SplitTextByCharacterTransition((c) => not List.Contains({\"0\"..\"9\"}, c), {\"0\"..\"9\"})".into(),
            };
            // As many columns as the longest split in the preview.
            let parts = split_count(table, col, &splitter);
            let names: Vec<String> = (1..=parts).map(|i| format!("{col}.{i}")).collect();
            let e = format!("Table.SplitColumn({{prev}}, {}, {splitter}, {})", quote_text(col), text_list(&names));
            Box::new(move |app, _| {
                add_step(app, "Split Column by Delimiter", &e);
                true
            })
        }
        Kind::GroupBy { groups, aggs } => {
            let g: Vec<String> = groups.iter().filter(|g| !g.is_empty()).cloned().collect();
            let specs: Vec<String> = aggs
                .iter()
                .map(|(name, op, col)| {
                    let f = field(col);
                    let (e, ty) = match op {
                        0 => ("Table.RowCount(_)".to_string(), "Int64.Type"),
                        1 => ("Table.RowCount(Table.Distinct(_))".to_string(), "Int64.Type"),
                        2 => (format!("List.Sum({f})"), "type number"),
                        3 => (format!("List.Average({f})"), "type number"),
                        4 => (format!("List.Median({f})"), "type number"),
                        5 => (format!("List.Min({f})"), "type any"),
                        6 => (format!("List.Max({f})"), "type any"),
                        _ => ("_".to_string(), "type table"),
                    };
                    format!("{{{}, each {e}, {ty}}}", quote_text(name))
                })
                .collect();
            let e = format!("Table.Group({{prev}}, {}, {{{}}})", text_list(&g), specs.join(", "));
            Box::new(move |app, dlg| {
                if g.is_empty() {
                    dlg.error = Some("Choose a column to group by.".into());
                    return false;
                }
                add_step(app, "Grouped Rows", &e);
                true
            })
        }
        Kind::Replace { cols, find, replace, entire } => {
            let e = if *entire {
                format!("Table.ReplaceValue({{prev}}, {}, {}, Replacer.ReplaceValue, {})", literal(find), literal(replace), text_list(cols))
            } else {
                format!("Table.ReplaceValue({{prev}}, {}, {}, Replacer.ReplaceText, {})", quote_text(find), quote_text(replace), text_list(cols))
            };
            Box::new(move |app, _| {
                add_step(app, "Replaced Value", &e);
                true
            })
        }
        Kind::ReplaceErrors { cols, value } => {
            let pairs: Vec<String> = cols.iter().map(|c| format!("{{{}, {}}}", quote_text(c), literal(value))).collect();
            let e = format!("Table.ReplaceErrorValues({{prev}}, {{{}}})", pairs.join(", "));
            Box::new(move |app, _| {
                add_step(app, "Replaced Errors", &e);
                true
            })
        }
        Kind::Merge { as_new, other, left, right, kind } => {
            let (as_new, other, left, right) = (*as_new, other.clone(), left.clone(), right.clone());
            let jk = JOIN_KINDS.get(*kind).map(|x| x.1).unwrap_or("LeftOuter");
            Box::new(move |app, dlg| {
                if other.is_empty() || left.is_empty() || left.len() != right.len() {
                    dlg.error = Some("Pick the second table and the same number of key columns on both sides.".into());
                    return false;
                }
                let other_id = gridcraft_engine::query::steps::ident(&other);
                let col = other.clone();
                if as_new {
                    let cur = gridcraft_engine::query::steps::ident(&app.powerquery.query);
                    let m = format!(
                        "let\n    Source = Table.NestedJoin({cur}, {}, {other_id}, {}, {}, JoinKind.{jk})\nin\n    Source",
                        text_list(&left),
                        text_list(&right),
                        quote_text(&col)
                    );
                    if let Some(r) = exec(app, dlg, "powerquery.newQuery", json!({ "name": "Merge1", "source": { "kind": "m", "m": m } })) {
                        open(app, r["name"].as_str());
                    } else {
                        return false;
                    }
                } else {
                    add_step(
                        app,
                        "Merged Queries",
                        &format!(
                            "Table.NestedJoin({{prev}}, {}, {other_id}, {}, {}, JoinKind.{jk})",
                            text_list(&left),
                            text_list(&right),
                            quote_text(&col)
                        ),
                    );
                }
                // Then choose the columns to bring in.
                app.powerquery.refresh_preview();
                app.powerquery.dialog = Some(Dialog::expand(app, &col));
                true
            })
        }
        Kind::Append { as_new, picked } => {
            let (as_new, picked) = (*as_new, picked.clone());
            Box::new(move |app, dlg| {
                if picked.is_empty() {
                    dlg.error = Some("Choose at least one query to append.".into());
                    return false;
                }
                let ids: Vec<String> = picked.iter().map(|q| gridcraft_engine::query::steps::ident(q)).collect();
                if as_new {
                    let cur = gridcraft_engine::query::steps::ident(&app.powerquery.query);
                    let m = format!("let\n    Source = Table.Combine({{{cur}, {}}})\nin\n    Source", ids.join(", "));
                    match exec(app, dlg, "powerquery.newQuery", json!({ "name": "Append1", "source": { "kind": "m", "m": m } })) {
                        Some(r) => open(app, r["name"].as_str()),
                        None => return false,
                    }
                } else {
                    add_step(app, "Appended Query", &format!("Table.Combine({{{{prev}}, {}}})", ids.join(", ")));
                }
                true
            })
        }
        Kind::Parameters { selected, name, kind, value, description } => {
            let (sel, name, kind, value, description) = (selected.clone(), name.clone(), kind.clone(), value.clone(), description.clone());
            Box::new(move |app, dlg| {
                let mut p = json!({ "name": sel.clone().unwrap_or_else(|| name.clone()), "value": value, "kind": kind, "description": description });
                if sel.as_ref().is_some_and(|s| *s != name) {
                    p["newName"] = Json::String(name.clone());
                }
                exec(app, dlg, "powerquery.setParameter", p).is_some()
            })
        }
        Kind::LoadTo { to } => {
            let to = ["table", "model", "none"].get(*to).copied().unwrap_or("table");
            Box::new(move |app, dlg| {
                let q = app.powerquery.query.clone();
                if exec(app, dlg, "powerquery.setQuery", json!({ "name": q, "load": to })).is_none() {
                    return false;
                }
                if exec(app, dlg, "powerquery.load", json!({ "query": q, "to": to })).is_none() {
                    return false;
                }
                toast(app, format!("Loaded {q}"));
                app.powerquery.open = false;
                true
            })
        }
        Kind::Advanced { m } => {
            let m = m.clone();
            Box::new(move |app, dlg| {
                let q = app.powerquery.query.clone();
                let ok = exec(app, dlg, "powerquery.setQuery", json!({ "name": q, "m": m })).is_some();
                if ok {
                    app.powerquery.step = None;
                }
                ok
            })
        }
        Kind::ChooseColumns { keep, .. } => {
            let cols: Vec<String> = keep.iter().filter(|(_, k)| *k).map(|(c, _)| c.clone()).collect();
            Box::new(move |app, _| {
                add_step(app, "Removed Other Columns", &format!("Table.SelectColumns({{prev}}, {})", text_list(&cols)));
                true
            })
        }
        Kind::RenameColumn { old, new } => {
            let e = format!("Table.RenameColumns({{prev}}, {{{{{}, {}}}}})", quote_text(old), quote_text(new.trim()));
            let (old, new) = (old.clone(), new.trim().to_string());
            Box::new(move |app, _| {
                if old != new && !new.is_empty() {
                    add_step(app, "Renamed Columns", &e);
                    app.powerquery.selected = vec![new];
                }
                true
            })
        }
        Kind::RenameQuery { old, new } => {
            let (old, new) = (old.clone(), new.trim().to_string());
            Box::new(move |app, dlg| {
                if exec(app, dlg, "powerquery.setQuery", json!({ "name": old, "newName": new })).is_some() {
                    app.powerquery.query = new;
                    true
                } else {
                    false
                }
            })
        }
        Kind::RenameStep { old, new } => {
            let (old, new) = (old.clone(), new.trim().to_string());
            Box::new(move |app, dlg| {
                let q = app.powerquery.query.clone();
                let ok = exec(app, dlg, "powerquery.renameStep", json!({ "query": q, "step": old, "newName": new })).is_some();
                if ok && app.powerquery.step.as_deref() == Some(old.as_str()) {
                    app.powerquery.step = Some(new);
                }
                ok
            })
        }
        Kind::Properties { name, description, group } => {
            let (name, description, group) = (name.clone(), description.clone(), group.clone());
            Box::new(move |app, dlg| {
                let q = app.powerquery.query.clone();
                let ok = exec(app, dlg, "powerquery.setQuery", json!({ "name": q, "newName": name, "description": description, "group": group }))
                    .is_some();
                if ok {
                    app.powerquery.query = name;
                }
                ok
            })
        }
        Kind::Custom { name, formula, ty } => {
            let ty = CUSTOM_TYPES.get(*ty).map(|x| x.1).unwrap_or("");
            let e = format!(
                "Table.AddColumn({{prev}}, {}, each {}{})",
                quote_text(name),
                formula.trim(),
                if ty.is_empty() { String::new() } else { format!(", {ty}") }
            );
            let ok = !formula.trim().is_empty();
            Box::new(move |app, dlg| {
                if !ok {
                    dlg.error = Some("Write a formula, for example [Units] * [Price].".into());
                    return false;
                }
                add_step(app, "Added Custom", &e);
                true
            })
        }
        Kind::Conditional { name, rules, otherwise } => {
            let mut e = String::new();
            for (col, op, value, out) in rules.iter().filter(|r| !r.0.is_empty()) {
                e.push_str(&format!("if {} then {} else ", cond_expr(col, *op, value), literal(out)));
            }
            e.push_str(&if otherwise.trim().is_empty() { "null".to_string() } else { literal(otherwise) });
            let step = format!("Table.AddColumn({{prev}}, {}, each {e})", quote_text(name));
            Box::new(move |app, _| {
                add_step(app, "Added Conditional Column", &step);
                true
            })
        }
        Kind::Pivot { col, values, agg } => {
            let f = PIVOT_AGGS.get(*agg).map(|x| x.1).unwrap_or("List.Sum");
            let e = format!(
                "Table.Pivot({{prev}}, List.Distinct(List.Transform({{prev}}{}, Text.From)), {}, {}{})",
                field(col),
                quote_text(col),
                quote_text(values),
                if f.is_empty() { String::new() } else { format!(", {f}") }
            );
            Box::new(move |app, _| {
                add_step(app, "Pivoted Column", &e);
                true
            })
        }
        Kind::Index { start, step } => {
            let e = format!(
                "Table.AddIndexColumn({{prev}}, \"Index\", {}, {}, Int64.Type)",
                start.trim().parse::<f64>().unwrap_or(1.0),
                step.trim().parse::<f64>().unwrap_or(1.0)
            );
            Box::new(move |app, _| {
                add_step(app, "Added Index", &e);
                true
            })
        }
        Kind::MergeColumns { cols, sep, custom, name, add } => {
            let s = if *sep == SEPARATORS.len() - 1 { custom.clone() } else { SEPARATORS.get(*sep).map(|x| x.1.to_string()).unwrap_or_default() };
            let name = if name.trim().is_empty() { "Merged".to_string() } else { name.trim().to_string() };
            let e = if *add {
                let parts: Vec<String> = cols.iter().map(|c| format!("Text.From({})", field(c))).collect();
                format!(
                    "Table.AddColumn({{prev}}, {}, each Text.Combine({{{}}}, {}), type text)",
                    quote_text(&name),
                    parts.join(", "),
                    quote_text(&s)
                )
            } else {
                let types: Vec<String> = cols.iter().map(|c| format!("{{{}, type text}}", quote_text(c))).collect();
                format!(
                    "Table.CombineColumns(Table.TransformColumnTypes({{prev}}, {{{}}}), {}, Combiner.CombineTextByDelimiter({}, QuoteStyle.None), {})",
                    types.join(", "),
                    text_list(cols),
                    quote_text(&s),
                    quote_text(&name)
                )
            };
            let step = if *add { "Inserted Merged Column" } else { "Merged Columns" };
            Box::new(move |app, _| {
                add_step(app, step, &e);
                true
            })
        }
        Kind::Expand { col, names, prefix, record } => {
            let picked: Vec<String> = names.iter().filter(|(_, on)| *on).map(|(n, _)| n.clone()).collect();
            let new: Vec<String> = picked.iter().map(|n| if *prefix { format!("{col}.{n}") } else { n.clone() }).collect();
            let f = if *record { "ExpandRecordColumn" } else { "ExpandTableColumn" };
            let e = format!("Table.{f}({{prev}}, {}, {}, {})", quote_text(col), text_list(&picked), text_list(&new));
            let name = format!("Expanded {col}");
            Box::new(move |app, _| {
                add_step(app, &name, &e);
                true
            })
        }
        Kind::Filter { col, values, op, value, .. } => {
            let f = field(col);
            let cond = if *op > 0 {
                Some(cond_expr(col, op - 1, value))
            } else if values.iter().all(|(_, k)| *k) {
                None
            } else {
                let kept: Vec<String> = values.iter().filter(|(_, k)| *k).map(|(v, _)| v.m_literal()).collect();
                Some(if kept.len() <= 4 {
                    if kept.is_empty() { "false".to_string() } else { kept.iter().map(|v| format!("{f} = {v}")).collect::<Vec<_>>().join(" or ") }
                } else {
                    format!("List.Contains({{{}}}, {f})", kept.join(", "))
                })
            };
            Box::new(move |app, _| {
                if let Some(c) = cond {
                    add_step(app, "Filtered Rows", &format!("Table.SelectRows({{prev}}, each {c})"));
                }
                true
            })
        }
        Kind::GetData { .. } => Box::new(|_, _| true),
    }
}

/// The number of parts the splitter makes, at most, over the preview rows.
fn split_count(t: &Table, col: &str, splitter: &str) -> usize {
    let Some(c) = t.column_index(col) else { return 2 };
    let host = gridcraft_engine::query::NoHost;
    let ev = gridcraft_engine::query::Evaluator::new(&host);
    let Ok(e) = gridcraft_engine::query::m::parse(splitter) else { return 2 };
    let Ok(f) = ev.eval(&e, &Default::default()) else { return 2 };
    let mut n = 1;
    for r in t.rows.iter().take(1000) {
        let s = r.get(c).map(|v| v.display()).unwrap_or_default();
        if let Ok(Value::List(l)) = ev.apply1(&f, Value::text(s)) {
            n = n.max(l.len());
        }
    }
    n.clamp(1, 500)
}
