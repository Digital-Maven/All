//! The editor's ribbon and the M each action adds.
//!
//! Column operations (format, extract, number, date and time transforms) come from one catalog,
//! [`OPS`]: on the Transform tab they change the selected columns in place
//! (`Table.TransformColumns`), on the Add Column tab they add a new column (`Table.AddColumn`).

use egui::Ui;
use gridcraft_engine::query::MType;
use gridcraft_engine::query::steps::{field, text_list};
use gridcraft_engine::query::value::quote_text;
use serde_json::json;

use super::dialogs::{Dialog, KeepRemove, SplitMode};
use super::{add_step, open, run, toast};
use crate::SheetApp;
use crate::icons::Icon;
use crate::widgets::{icon_tool, ribbon_group};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Text,
    Number,
    Date,
    Time,
    Duration,
}

/// A column operation. `f` uses `X` for the value and `P` for the parameter.
pub struct ColOp {
    pub menu: &'static str,
    pub label: &'static str,
    pub kind: Kind,
    pub f: &'static str,
    pub ty: &'static str,
    /// The prompt, default and whether the parameter is text (quoted).
    pub param: Option<(&'static str, &'static str, bool)>,
}

const fn op(menu: &'static str, label: &'static str, kind: Kind, f: &'static str, ty: &'static str) -> ColOp {
    ColOp { menu, label, kind, f, ty, param: None }
}

/// An operation with a parameter: (prompt, default, text?).
const fn opp(
    menu: &'static str,
    label: &'static str,
    kind: Kind,
    f: &'static str,
    ty: &'static str,
    param: (&'static str, &'static str, bool),
) -> ColOp {
    ColOp { menu, label, kind, f, ty, param: Some(param) }
}

use Kind::{Date, Duration as Dur, Number as Num, Text as Txt, Time};

pub const OPS: &[ColOp] = &[
    op("Format", "lowercase", Txt, "Text.Lower(X)", "type text"),
    op("Format", "UPPERCASE", Txt, "Text.Upper(X)", "type text"),
    op("Format", "Capitalize Each Word", Txt, "Text.Proper(X)", "type text"),
    op("Format", "Trim", Txt, "Text.Trim(X)", "type text"),
    op("Format", "Clean", Txt, "Text.Clean(X)", "type text"),
    opp("Format", "Add Prefix", Txt, "P & X", "type text", ("Prefix", "", true)),
    opp("Format", "Add Suffix", Txt, "X & P", "type text", ("Suffix", "", true)),
    op("Extract", "Length", Txt, "Text.Length(X)", "Int64.Type"),
    opp("Extract", "First Characters", Txt, "Text.Start(X, P)", "type text", ("Count", "1", false)),
    opp("Extract", "Last Characters", Txt, "Text.End(X, P)", "type text", ("Count", "1", false)),
    opp("Extract", "Range", Txt, "Text.Middle(X, P)", "type text", ("Starting index, number of characters", "0, 1", false)),
    opp("Extract", "Text Before Delimiter", Txt, "Text.BeforeDelimiter(X, P)", "type text", ("Delimiter", " ", true)),
    opp("Extract", "Text After Delimiter", Txt, "Text.AfterDelimiter(X, P)", "type text", ("Delimiter", " ", true)),
    opp(
        "Extract",
        "Text Between Delimiters",
        Txt,
        "Text.BetweenDelimiters(X, P)",
        "type text",
        ("Start and end delimiters (M)", "\"(\", \")\"", false),
    ),
    op("Extract", "Digits Only", Txt, "Text.Select(X, {\"0\"..\"9\"})", "type text"),
    op("Parse", "JSON", Txt, "Json.Document(X)", "type any"),
    opp("Standard", "Add", Num, "X + P", "type number", ("Value", "1", false)),
    opp("Standard", "Multiply", Num, "X * P", "type number", ("Value", "1", false)),
    opp("Standard", "Subtract", Num, "X - P", "type number", ("Value", "1", false)),
    opp("Standard", "Divide", Num, "X / P", "type number", ("Value", "1", false)),
    opp("Standard", "Integer-Divide", Num, "Number.IntegerDivide(X, P)", "Int64.Type", ("Value", "1", false)),
    opp("Standard", "Modulo", Num, "Number.Mod(X, P)", "type number", ("Value", "2", false)),
    opp("Standard", "Percentage", Num, "X * P / 100", "type number", ("Percentage", "10", false)),
    opp("Standard", "Percent Of", Num, "X / P * 100", "type number", ("Value", "100", false)),
    op("Scientific", "Absolute Value", Num, "Number.Abs(X)", "type number"),
    opp("Scientific", "Power", Num, "Number.Power(X, P)", "type number", ("Power", "2", false)),
    op("Scientific", "Square Root", Num, "Number.Sqrt(X)", "type number"),
    op("Scientific", "Exponent", Num, "Number.Exp(X)", "type number"),
    op("Scientific", "Logarithm Base-10", Num, "Number.Log10(X)", "type number"),
    op("Scientific", "Natural Logarithm", Num, "Number.Ln(X)", "type number"),
    op("Rounding", "Round Up", Num, "Number.RoundUp(X)", "Int64.Type"),
    op("Rounding", "Round Down", Num, "Number.RoundDown(X)", "Int64.Type"),
    opp("Rounding", "Round…", Num, "Number.Round(X, P)", "type number", ("Decimal places", "2", false)),
    op("Information", "Is Even", Num, "Number.IsEven(X)", "type logical"),
    op("Information", "Is Odd", Num, "Number.IsOdd(X)", "type logical"),
    op("Information", "Sign", Num, "Number.Sign(X)", "Int64.Type"),
    op("Date", "Date Only", Date, "DateTime.Date(X)", "type date"),
    op("Date", "Parse", Date, "Date.From(X)", "type date"),
    op("Date", "Year", Date, "Date.Year(X)", "Int64.Type"),
    op("Date", "Start of Year", Date, "Date.StartOfYear(X)", "type date"),
    op("Date", "End of Year", Date, "Date.EndOfYear(X)", "type date"),
    op("Date", "Month", Date, "Date.Month(X)", "Int64.Type"),
    op("Date", "Name of Month", Date, "Date.MonthName(X)", "type text"),
    op("Date", "Start of Month", Date, "Date.StartOfMonth(X)", "type date"),
    op("Date", "End of Month", Date, "Date.EndOfMonth(X)", "type date"),
    op("Date", "Days in Month", Date, "Date.DaysInMonth(X)", "Int64.Type"),
    op("Date", "Quarter of Year", Date, "Date.QuarterOfYear(X)", "Int64.Type"),
    op("Date", "Start of Quarter", Date, "Date.StartOfQuarter(X)", "type date"),
    op("Date", "End of Quarter", Date, "Date.EndOfQuarter(X)", "type date"),
    op("Date", "Week of Year", Date, "Date.WeekOfYear(X)", "Int64.Type"),
    op("Date", "Week of Month", Date, "Date.WeekOfMonth(X)", "Int64.Type"),
    op("Date", "Start of Week", Date, "Date.StartOfWeek(X)", "type date"),
    op("Date", "End of Week", Date, "Date.EndOfWeek(X)", "type date"),
    op("Date", "Day", Date, "Date.Day(X)", "Int64.Type"),
    op("Date", "Day of Week", Date, "Date.DayOfWeek(X)", "Int64.Type"),
    op("Date", "Day of Year", Date, "Date.DayOfYear(X)", "Int64.Type"),
    op("Date", "Name of Day", Date, "Date.DayOfWeekName(X)", "type text"),
    op("Date", "Year-Month", Date, "Date.ToText(X, \"yyyy-MM\")", "type text"),
    op("Date", "Age", Date, "DateTime.Date(DateTime.LocalNow()) - X", "type duration"),
    op("Time", "Time Only", Time, "DateTime.Time(X)", "type time"),
    op("Time", "Hour", Time, "Time.Hour(X)", "Int64.Type"),
    op("Time", "Minute", Time, "Time.Minute(X)", "Int64.Type"),
    op("Time", "Second", Time, "Time.Second(X)", "Int64.Type"),
    op("Duration", "Days", Dur, "Duration.Days(X)", "Int64.Type"),
    op("Duration", "Hours", Dur, "Duration.Hours(X)", "Int64.Type"),
    op("Duration", "Minutes", Dur, "Duration.Minutes(X)", "Int64.Type"),
    op("Duration", "Seconds", Dur, "Duration.Seconds(X)", "Int64.Type"),
    op("Duration", "Total Years", Dur, "Duration.TotalDays(X) / 365", "type number"),
    op("Duration", "Total Days", Dur, "Duration.TotalDays(X)", "type number"),
    op("Duration", "Total Hours", Dur, "Duration.TotalHours(X)", "type number"),
    op("Duration", "Total Minutes", Dur, "Duration.TotalMinutes(X)", "type number"),
];

/// The selected columns, or a toast asking for one.
pub fn need_columns(app: &mut SheetApp) -> Option<Vec<String>> {
    let s = app.powerquery.selected.clone();
    if s.is_empty() {
        toast(app, "Select a column first (click its header; Ctrl-click for more).");
        None
    } else {
        Some(s)
    }
}

/// Applies a catalog operation: in place (`add == false`) or as a new column.
pub fn apply_op(app: &mut SheetApp, i: usize, add: bool, param: Option<String>) {
    let Some(o) = OPS.get(i) else { return };
    if o.param.is_some() && param.is_none() {
        app.powerquery.dialog = Some(Dialog::param(i, add));
        return;
    }
    let Some(cols) = need_columns(app) else { return };
    let p = match (o.param, &param) {
        (Some((_, _, true)), Some(v)) => quote_text(v),
        (_, Some(v)) => v.clone(),
        _ => String::new(),
    };
    let body = o.f.replace('P', "\u{1}");
    if add {
        let c = cols.first().cloned().unwrap_or_default();
        let e = body.replace('X', &field(&c)).replace('\u{1}', &p);
        let name = o.label.trim_end_matches('…').to_string();
        add_step(
            app,
            &format!("Inserted {}", o.label.trim_end_matches('…')),
            &format!("Table.AddColumn({{prev}}, {}, each {e}, {})", quote_text(&name), o.ty),
        );
    } else {
        let e = body.replace('X', "_").replace('\u{1}', &p);
        let specs: Vec<String> = cols.iter().map(|c| format!("{{{}, each {e}, {}}}", quote_text(c), o.ty)).collect();
        add_step(app, &step_name(o), &format!("Table.TransformColumns({{prev}}, {{{}}})", specs.join(", ")));
    }
}

fn step_name(o: &ColOp) -> String {
    match o.label {
        "lowercase" => "Lowercased Text".into(),
        "UPPERCASE" => "Uppercased Text".into(),
        "Capitalize Each Word" => "Capitalized Each Word".into(),
        "Trim" => "Trimmed Text".into(),
        "Clean" => "Cleaned Text".into(),
        "Add Prefix" => "Added Prefix".into(),
        "Add Suffix" => "Added Suffix".into(),
        l if o.menu == "Extract" => format!("Extracted {l}"),
        l if o.menu == "Date" || o.menu == "Time" || o.menu == "Duration" => format!("Extracted {l}"),
        l => format!("Calculated {}", l.trim_end_matches('…')),
    }
}

/// A submenu of catalog operations of one menu (`Format`, `Extract`, `Date`, …).
pub fn op_menu(app: &mut SheetApp, ui: &mut Ui, menu: &str, add: bool) {
    ribbon_menu(ui, menu, |ui| {
        for (i, o) in OPS.iter().enumerate().filter(|(_, o)| o.menu == menu) {
            if ui.button(o.label).clicked() {
                apply_op(app, i, add, None);
                ui.close();
            }
        }
    });
}

pub fn change_type(app: &mut SheetApp, cols: &[String], ty: MType) {
    let pairs: Vec<String> = cols.iter().map(|c| format!("{{{}, {}}}", quote_text(c), ty.m())).collect();
    add_step(app, "Changed Type", &format!("Table.TransformColumnTypes({{prev}}, {{{}}})", pairs.join(", ")));
}

pub fn type_menu(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Data Type", |ui| {
        for ty in MType::COLUMN_TYPES {
            if ui.button(ty.name()).clicked() {
                if let Some(c) = need_columns(app) {
                    change_type(app, &c, ty);
                }
                ui.close();
            }
        }
    });
}

pub fn sort(app: &mut SheetApp, desc: bool) {
    let Some(c) = need_columns(app) else { return };
    let keys: Vec<String> = c.iter().map(|x| format!("{{{}, Order.{}}}", quote_text(x), if desc { "Descending" } else { "Ascending" })).collect();
    add_step(app, "Sorted Rows", &format!("Table.Sort({{prev}}, {{{}}})", keys.join(", ")));
}

pub fn remove_columns(app: &mut SheetApp, other: bool) {
    let Some(c) = need_columns(app) else { return };
    if other {
        add_step(app, "Removed Other Columns", &format!("Table.SelectColumns({{prev}}, {})", text_list(&c)));
    } else {
        add_step(app, "Removed Columns", &format!("Table.RemoveColumns({{prev}}, {})", text_list(&c)));
    }
    app.powerquery.selected.clear();
}

/// Moves the selected columns (Move Left/Right/To Beginning/To End).
pub fn move_columns(app: &mut SheetApp, to: &str) {
    let Some(sel) = need_columns(app) else { return };
    let Some(p) = app.powerquery.preview(&app.session) else { return };
    let mut cols = p.table.columns.clone();
    match to {
        "start" | "end" => {
            cols.retain(|c| !sel.contains(c));
            if to == "start" {
                let mut v = sel.clone();
                v.extend(cols);
                cols = v;
            } else {
                cols.extend(sel.iter().cloned());
            }
        }
        _ => {
            for s in &sel {
                if let Some(i) = cols.iter().position(|c| c == s) {
                    let j = if to == "left" { i.saturating_sub(1) } else { (i + 1).min(cols.len() - 1) };
                    cols.swap(i, j);
                }
            }
        }
    }
    add_step(app, "Reordered Columns", &format!("Table.ReorderColumns({{prev}}, {})", text_list(&cols)));
}

fn statistics(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Statistics", |ui| {
        for (label, f) in [
            ("Sum", "List.Sum"),
            ("Minimum", "List.Min"),
            ("Maximum", "List.Max"),
            ("Median", "List.Median"),
            ("Average", "List.Average"),
            ("Standard Deviation", "List.StandardDeviation"),
            ("Count Values", "List.NonNullCount"),
            ("Count Distinct Values", "List.Distinct"),
        ] {
            if ui.button(label).clicked() {
                if let Some(c) = need_columns(app).and_then(|c| c.first().cloned()) {
                    let e = if f == "List.Distinct" {
                        format!("List.Count(List.Distinct({{prev}}{}))", field(&c))
                    } else {
                        format!("{f}({{prev}}{})", field(&c))
                    };
                    add_step(app, label, &e);
                }
                ui.close();
            }
        }
    });
}

fn split_menu(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Split Column", |ui| {
        for (label, mode) in [
            ("By Delimiter…", SplitMode::Delimiter),
            ("By Number of Characters…", SplitMode::Chars),
            ("By Positions…", SplitMode::Positions),
            ("By Lowercase to Uppercase", SplitMode::LowerUpper),
            ("By Uppercase to Lowercase", SplitMode::UpperLower),
            ("By Digit to Non-Digit", SplitMode::DigitNon),
            ("By Non-Digit to Digit", SplitMode::NonDigit),
        ] {
            if ui.button(label).clicked() {
                if let Some(c) = need_columns(app).and_then(|c| c.first().cloned()) {
                    app.powerquery.dialog = Some(Dialog::split(&c, mode));
                }
                ui.close();
            }
        }
    });
}

fn rows_menus(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Keep Rows", |ui| {
        for (label, k) in
            [("Keep Top Rows…", KeepRemove::KeepTop), ("Keep Bottom Rows…", KeepRemove::KeepBottom), ("Keep Range of Rows…", KeepRemove::KeepRange)]
        {
            if ui.button(label).clicked() {
                app.powerquery.dialog = Some(Dialog::rows(k));
                ui.close();
            }
        }
        if ui.button("Keep Duplicates").clicked() {
            if let Some(c) = need_columns(app) {
                add_step(
                    app,
                    "Kept Duplicates",
                    &format!(
                        "let columnNames = {}, addCount = Table.Group({{prev}}, columnNames, {{{{\"Count\", Table.RowCount, type number}}}}), selectDuplicates = Table.SelectRows(addCount, each [Count] > 1), removeCount = Table.RemoveColumns(selectDuplicates, \"Count\") in Table.Join({{prev}}, columnNames, removeCount, columnNames, JoinKind.Inner)",
                        text_list(&c)
                    ),
                );
            }
            ui.close();
        }
        if ui.button("Keep Errors").clicked() {
            let cols = app.powerquery.selected.clone();
            add_step(
                app,
                "Kept Errors",
                &if cols.is_empty() {
                    "Table.SelectRowsWithErrors({prev})".to_string()
                } else {
                    format!("Table.SelectRowsWithErrors({{prev}}, {})", text_list(&cols))
                },
            );
            ui.close();
        }
    });
    ribbon_menu(ui, "Remove Rows", |ui| {
        for (label, k) in [
            ("Remove Top Rows…", KeepRemove::RemoveTop),
            ("Remove Bottom Rows…", KeepRemove::RemoveBottom),
            ("Remove Alternate Rows…", KeepRemove::Alternate),
        ] {
            if ui.button(label).clicked() {
                app.powerquery.dialog = Some(Dialog::rows(k));
                ui.close();
            }
        }
        if ui.button("Remove Duplicates").clicked() {
            let cols = app.powerquery.selected.clone();
            add_step(
                app,
                "Removed Duplicates",
                &if cols.is_empty() { "Table.Distinct({prev})".to_string() } else { format!("Table.Distinct({{prev}}, {})", text_list(&cols)) },
            );
            ui.close();
        }
        if ui.button("Remove Blank Rows").clicked() {
            add_step(
                app,
                "Removed Blank Rows",
                "Table.SelectRows({prev}, each not List.IsEmpty(List.RemoveItems(Record.FieldValues(_), {\"\", null})))",
            );
            ui.close();
        }
        if ui.button("Remove Errors").clicked() {
            let cols = app.powerquery.selected.clone();
            add_step(
                app,
                "Removed Errors",
                &if cols.is_empty() {
                    "Table.RemoveRowsWithErrors({prev})".to_string()
                } else {
                    format!("Table.RemoveRowsWithErrors({{prev}}, {})", text_list(&cols))
                },
            );
            ui.close();
        }
    });
}

fn headers_menu(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Use First Row as Headers", |ui| {
        if ui.button("Use First Row as Headers").clicked() {
            add_step(app, "Promoted Headers", "Table.PromoteHeaders({prev}, [PromoteAllScalars=true])");
            ui.close();
        }
        if ui.button("Use Headers as First Row").clicked() {
            add_step(app, "Demoted Headers", "Table.DemoteHeaders({prev})");
            ui.close();
        }
    });
}

fn unpivot_menu(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Unpivot Columns", |ui| {
        if ui.button("Unpivot Columns").clicked() || ui.button("Unpivot Only Selected Columns").clicked() {
            if let Some(c) = need_columns(app) {
                add_step(app, "Unpivoted Columns", &format!("Table.Unpivot({{prev}}, {}, \"Attribute\", \"Value\")", text_list(&c)));
            }
            ui.close();
        }
        if ui.button("Unpivot Other Columns").clicked() {
            if let Some(c) = need_columns(app) {
                add_step(
                    app,
                    "Unpivoted Other Columns",
                    &format!("Table.UnpivotOtherColumns({{prev}}, {}, \"Attribute\", \"Value\")", text_list(&c)),
                );
            }
            ui.close();
        }
    });
}

fn move_menu(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Move", |ui| {
        for (label, to) in [("Left", "left"), ("Right", "right"), ("To Beginning", "start"), ("To End", "end")] {
            if ui.button(label).clicked() {
                move_columns(app, to);
                ui.close();
            }
        }
    });
}

fn fill_menu(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Fill", |ui| {
        for (label, f) in [("Down", "FillDown"), ("Up", "FillUp")] {
            if ui.button(label).clicked() {
                if let Some(c) = need_columns(app) {
                    add_step(app, &format!("Filled {label}"), &format!("Table.{f}({{prev}}, {})", text_list(&c)));
                }
                ui.close();
            }
        }
    });
}

fn index_menu(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_menu(ui, "Index Column", |ui| {
        if ui.button("From 0").clicked() {
            add_step(app, "Added Index", "Table.AddIndexColumn({prev}, \"Index\", 0, 1, Int64.Type)");
            ui.close();
        }
        if ui.button("From 1").clicked() {
            add_step(app, "Added Index", "Table.AddIndexColumn({prev}, \"Index\", 1, 1, Int64.Type)");
            ui.close();
        }
        if ui.button("Custom…").clicked() {
            app.powerquery.dialog = Some(Dialog::index());
            ui.close();
        }
    });
}

pub fn detect_types(app: &mut SheetApp) {
    let Some(p) = app.powerquery.preview(&app.session) else { return };
    let cols: Vec<String> = if app.powerquery.selected.is_empty() { p.table.columns.clone() } else { app.powerquery.selected.clone() };
    let mut pairs = Vec::new();
    for c in &cols {
        let Some(i) = p.table.column_index(c) else { continue };
        let vals: Vec<_> = p.table.rows.iter().take(200).map(|r| r.get(i).cloned().unwrap_or_default()).collect();
        let ty = gridcraft_engine::query::funcs::convert::detect(&vals);
        if ty != MType::Any {
            pairs.push(format!("{{{}, {}}}", quote_text(c), ty.m()));
        }
    }
    if pairs.is_empty() {
        toast(app, "No types to detect.");
    } else {
        add_step(app, "Changed Type", &format!("Table.TransformColumnTypes({{prev}}, {{{}}})", pairs.join(", ")));
    }
}

/// Combine Files (folder queries): read every file the same way and append the results.
pub fn combine_files(app: &mut SheetApp) {
    let Some(p) = app.powerquery.preview(&app.session) else { return };
    let t = p.table.clone();
    let (Some(_), Some(ext_c)) = (t.column_index("Content"), t.column_index("Extension")) else {
        toast(app, "Combine Files works on a folder's file list (Content, Name, Extension columns).");
        return;
    };
    let ext = t.rows.first().and_then(|r| r.get(ext_c)).map(|v| v.display().to_lowercase()).unwrap_or_default();
    let read = match ext.as_str() {
        ".xlsx" | ".xlsm" => "Table.PromoteHeaders(Excel.Workbook([Content], null, true){0}[Data], [PromoteAllScalars=true])",
        ".json" => "Table.FromRecords(Json.Document([Content]))",
        _ => "Table.PromoteHeaders(Csv.Document([Content]), [PromoteAllScalars=true])",
    };
    add_step(app, "Filtered Files", &format!("Table.SelectRows({{prev}}, each Text.Lower([Extension]) = {})", quote_text(&ext)));
    add_step(app, "Transform File", &format!("Table.AddColumn({{prev}}, \"Transform File\", each {read})"));
    add_step(app, "Removed Other Columns", "Table.SelectColumns({prev}, {\"Name\", \"Transform File\"})");
    app.powerquery.refresh_preview();
    let Some(p) = app.powerquery.preview(&app.session) else { return };
    let cols = p.table.column_index("Transform File").and_then(|c| p.table.rows.first().and_then(|r| r.get(c)).cloned());
    match cols {
        Some(gridcraft_engine::query::Value::Table(nt)) => {
            add_step(app, "Expanded Table Column", &format!("Table.ExpandTableColumn({{prev}}, \"Transform File\", {})", text_list(&nt.columns)));
            super::ops::detect_types(app);
        }
        _ => toast(app, "The first file couldn't be read; check the Transform File step."),
    }
}

fn big(ui: &mut Ui, label: &str, tip: &str) -> bool {
    icon_tool(ui, icon_for(label), label, tip, false, false).clicked()
}

/// A ribbon dropdown: an icon button whose menu holds `add`. Inside a menu (the column header's
/// right-click menu reuses these) it is an ordinary submenu instead.
fn ribbon_menu(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui)) {
    if egui::containers::menu::is_in_menu(ui) {
        ui.menu_button(label, add);
        return;
    }
    let r = icon_tool(ui, icon_for(label), label, menu_tip(label), true, false);
    egui::Popup::menu(&r).show(add);
}

/// Hover descriptions of the ribbon's dropdowns.
fn menu_tip(label: &str) -> &'static str {
    match label {
        "New Source" => "Get data from a table, CSV, workbook, JSON, folder, web page or a blank query",
        "Manage" => "Duplicate, reference or delete this query",
        "Remove Columns" => "Remove the selected columns, or every other column",
        "Keep Rows" => "Keep the top, bottom or a range of rows",
        "Remove Rows" => "Remove top, bottom, alternate, duplicate, blank or error rows",
        "Split Column" => "Split the selected column by a delimiter, a number of characters or positions",
        "Data Type" => "Set the type of the selected columns",
        "Use First Row as Headers" => "Promote the first row to headers, or demote the headers to a row",
        "Replace Values" => "Replace values or errors in the selected columns",
        "Merge Queries" => "Join another query's columns by matching key columns",
        "Append Queries" => "Add the rows of other queries under this one",
        "Manage Parameters" => "Values queries can refer to by name",
        "Fill" => "Fill empty cells down or up from their neighbours",
        "Unpivot Columns" => "Turn columns into attribute / value rows",
        "Move" => "Move the selected columns left, right, to the beginning or the end",
        "Statistics" => "Sum, minimum, maximum, average, count … of the selected column",
        "Format" => "Lowercase, UPPERCASE, Capitalize, Trim, Clean, add a prefix or suffix",
        "Extract" => "Length, first or last characters, a range, or text between delimiters",
        "Parse" => "Read JSON or XML text into structured values",
        "Standard" => "Add, subtract, multiply, divide, modulo, percentage …",
        "Scientific" => "Absolute value, power, square root, exponent, logarithms, factorial",
        "Rounding" => "Round up, round down or round to a number of digits",
        "Information" => "Is even, is odd, sign",
        "Date" => "Year, month, quarter, week, day, age, start or end of a period …",
        "Time" => "Hour, minute, second, start or end of hour",
        "Duration" => "Days, hours, minutes, seconds, total years …",
        "Column From Examples" => "Type a few example results; the transformation is worked out for you",
        "Index Column" => "A row number column, from 0, from 1 or custom",
        "Go to Column" => "Select a column by name",
        _ => "",
    }
}

/// The ribbon icon of a command.
fn icon_for(label: &str) -> Icon {
    use crate::icons::Pq;
    match label {
        "New Source" => Icon::Pq(Pq::NewSource),
        "Enter Data" => Icon::Pq(Pq::EnterData),
        "Refresh Preview" => Icon::Pq(Pq::Refresh),
        "Refresh All" => Icon::Pq(Pq::RefreshAll),
        "Advanced Editor" => Icon::Pq(Pq::AdvancedEditor),
        "Manage" => Icon::Pq(Pq::Manage),
        "Choose Columns" => Icon::Pq(Pq::ChooseColumns),
        "Remove Columns" => Icon::Pq(Pq::RemoveColumns),
        "Keep Rows" => Icon::Pq(Pq::KeepRows),
        "Remove Rows" => Icon::Pq(Pq::RemoveRows),
        "Sort A→Z" => Icon::SortAsc,
        "Sort Z→A" => Icon::SortDesc,
        "Split Column" => Icon::Pq(Pq::SplitColumn),
        "Group By" => Icon::Pq(Pq::GroupBy),
        "Data Type" => Icon::Pq(Pq::DataType),
        "Use First Row as Headers" => Icon::Pq(Pq::Headers),
        "Replace Values" => Icon::Pq(Pq::Replace),
        "Merge Queries" => Icon::Pq(Pq::MergeQueries),
        "Append Queries" => Icon::Pq(Pq::AppendQueries),
        "Combine Files" => Icon::Pq(Pq::CombineFiles),
        "Manage Parameters" => Icon::Pq(Pq::Parameters),
        "Close & Load To…" => Icon::Pq(Pq::CloseLoad),
        "Transpose" => Icon::Pq(Pq::Transpose),
        "Reverse Rows" => Icon::Pq(Pq::ReverseRows),
        "Count Rows" => Icon::Pq(Pq::CountRows),
        "Detect Data Type" => Icon::Pq(Pq::DetectType),
        "Rename" => Icon::Pq(Pq::Rename),
        "Fill" => Icon::FillDown,
        "Pivot Column" => Icon::Pq(Pq::PivotColumn),
        "Unpivot Columns" => Icon::Pq(Pq::Unpivot),
        "Move" => Icon::Pq(Pq::MoveColumns),
        "Convert to List" => Icon::Pq(Pq::ToList),
        "Format" => Icon::Pq(Pq::Format),
        "Merge Columns" => Icon::Merge,
        "Extract" => Icon::Pq(Pq::Extract),
        "Parse" => Icon::Pq(Pq::Parse),
        "Statistics" => Icon::Sum,
        "Standard" => Icon::Pq(Pq::Standard),
        "Scientific" => Icon::Pq(Pq::Scientific),
        "Rounding" => Icon::DecDec,
        "Information" => Icon::Info,
        "Date" => Icon::Calendar,
        "Time" => Icon::Clock,
        "Duration" => Icon::Pq(Pq::Duration),
        "Expand" => Icon::Pq(Pq::ExpandColumn),
        "Column From Examples" => Icon::Pq(Pq::FromExamples),
        "Custom Column" => Icon::Pq(Pq::CustomColumn),
        "Conditional Column" => Icon::Pq(Pq::Conditional),
        "Index Column" => Icon::Pq(Pq::IndexColumn),
        "Duplicate Column" => Icon::Copy,
        "Query Dependencies" => Icon::Pq(Pq::Dependencies),
        "Go to Column" => Icon::Pq(Pq::GoToColumn),
        _ => Icon::Table,
    }
}

/// The editor's ribbon for the active tab.
pub fn ribbon(app: &mut SheetApp, ui: &mut Ui) {
    // One row of groups; a narrow window scrolls it sideways (wheel or the thin bar).
    egui::ScrollArea::horizontal().id_salt("pq_ribbon_scroll").show(ui, |ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
            match app.powerquery.tab {
                0 => home(app, ui),
                1 => transform(app, ui),
                2 => add_column(app, ui),
                _ => view(app, ui),
            }
        });
    });
}

fn home(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_group(ui, "New Query", |ui| {
        ribbon_menu(ui, "New Source", |ui| {
            new_source_items(app, ui);
        });
        if big(ui, "Enter Data", "Type or paste a small table") {
            app.powerquery.dialog = Some(Dialog::get_data("enter"));
        }
    });
    ribbon_group(ui, "Query", |ui| {
        if big(ui, "Refresh Preview", "Recompute the preview") {
            app.powerquery.refresh_preview();
        }
        if big(ui, "Refresh All", "Reload every loaded query") && run(app, "powerquery.refresh", json!({})).is_some() {
            app.powerquery.refresh_preview();
            toast(app, "Refreshed");
        }
        if big(ui, "Advanced Editor", "Edit the whole query in M") {
            app.powerquery.dialog = Some(Dialog::advanced(app));
        }
        ribbon_menu(ui, "Manage", |ui| {
            manage_items(app, ui);
        });
    });
    ribbon_group(ui, "Reduce Rows & Columns", |ui| {
        if big(ui, "Choose Columns", "Pick the columns to keep") {
            app.powerquery.dialog = Some(Dialog::choose_columns(app));
        }
        ribbon_menu(ui, "Remove Columns", |ui| {
            if ui.button("Remove Columns").clicked() {
                remove_columns(app, false);
                ui.close();
            }
            if ui.button("Remove Other Columns").clicked() {
                remove_columns(app, true);
                ui.close();
            }
        });
        rows_menus(app, ui);
        if big(ui, "Sort A→Z", "Sort ascending by the selected columns") {
            sort(app, false);
        }
        if big(ui, "Sort Z→A", "Sort descending by the selected columns") {
            sort(app, true);
        }
    });
    ribbon_group(ui, "Transform", |ui| {
        split_menu(app, ui);
        if big(ui, "Group By", "Summarize rows by the values of columns") {
            app.powerquery.dialog = Some(Dialog::group_by(app));
        }
        type_menu(app, ui);
        headers_menu(app, ui);
        if big(ui, "Replace Values", "Replace a value in the selected columns")
            && let Some(c) = need_columns(app)
        {
            app.powerquery.dialog = Some(Dialog::replace(c));
        }
    });
    ribbon_group(ui, "Combine", |ui| {
        ribbon_menu(ui, "Merge Queries", |ui| {
            if ui.button("Merge Queries").clicked() {
                app.powerquery.dialog = Some(Dialog::merge(app, false));
                ui.close();
            }
            if ui.button("Merge Queries as New").clicked() {
                app.powerquery.dialog = Some(Dialog::merge(app, true));
                ui.close();
            }
        });
        ribbon_menu(ui, "Append Queries", |ui| {
            if ui.button("Append Queries").clicked() {
                app.powerquery.dialog = Some(Dialog::append(false));
                ui.close();
            }
            if ui.button("Append Queries as New").clicked() {
                app.powerquery.dialog = Some(Dialog::append(true));
                ui.close();
            }
        });
        if big(ui, "Combine Files", "Read every file of a folder query and append them") {
            combine_files(app);
        }
    });
    ribbon_group(ui, "Parameters & Load", |ui| {
        ribbon_menu(ui, "Manage Parameters", |ui| {
            if ui.button("Manage Parameters").clicked() {
                app.powerquery.dialog = Some(Dialog::parameters(app, None));
                ui.close();
            }
            if ui.button("New Parameter").clicked() {
                app.powerquery.dialog = Some(Dialog::parameters(app, Some(String::new())));
                ui.close();
            }
        });
        if big(ui, "Close & Load To…", "Choose where the query loads: a table, the data model, or connection only") {
            app.powerquery.dialog = Some(Dialog::load_to(app));
        }
    });
}

/// The New Source / Get Data items.
pub fn new_source_items(app: &mut SheetApp, ui: &mut Ui) {
    for (label, kind) in [
        ("From Table/Range", "table"),
        ("From Text/CSV", "csv"),
        ("From Workbook (Excel)", "excel"),
        ("From JSON", "json"),
        ("From Folder", "folder"),
        ("From Text File (lines)", "text"),
        ("From Web", "web"),
        ("Enter Data", "enter"),
        ("Blank Query", "blank"),
    ] {
        if ui.button(label).clicked() {
            app.powerquery.dialog = Some(Dialog::get_data(kind));
            ui.close();
        }
    }
}

fn manage_items(app: &mut SheetApp, ui: &mut Ui) {
    let q = app.powerquery.query.clone();
    if ui.button("Duplicate").clicked() {
        if let Some(r) = run(app, "powerquery.duplicateQuery", json!({ "name": q })) {
            open(app, r["name"].as_str());
        }
        ui.close();
    }
    if ui.button("Reference").clicked() {
        if let Some(r) = run(app, "powerquery.referenceQuery", json!({ "name": q })) {
            open(app, r["name"].as_str());
        }
        ui.close();
    }
    if ui.button("Delete").clicked() {
        if run(app, "powerquery.deleteQuery", json!({ "name": q })).is_some() {
            open(app, None);
        }
        ui.close();
    }
}

fn transform(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_group(ui, "Table", |ui| {
        if big(ui, "Group By", "Summarize rows by the values of columns") {
            app.powerquery.dialog = Some(Dialog::group_by(app));
        }
        headers_menu(app, ui);
        if big(ui, "Transpose", "Rows become columns") {
            add_step(app, "Transposed Table", "Table.Transpose({prev})");
        }
        if big(ui, "Reverse Rows", "Last row first") {
            add_step(app, "Reversed Rows", "Table.ReverseRows({prev})");
        }
        if big(ui, "Count Rows", "The number of rows") {
            add_step(app, "Counted Rows", "Table.RowCount({prev})");
        }
    });
    ribbon_group(ui, "Any Column", |ui| {
        type_menu(app, ui);
        if big(ui, "Detect Data Type", "Set types from the data") {
            detect_types(app);
        }
        if big(ui, "Rename", "Rename the selected column")
            && let Some(c) = need_columns(app).and_then(|c| c.first().cloned())
        {
            app.powerquery.dialog = Some(Dialog::rename_column(&c));
        }
        ribbon_menu(ui, "Replace Values", |ui| {
            if ui.button("Replace Values").clicked() {
                if let Some(c) = need_columns(app) {
                    app.powerquery.dialog = Some(Dialog::replace(c));
                }
                ui.close();
            }
            if ui.button("Replace Errors").clicked() {
                if let Some(c) = need_columns(app) {
                    app.powerquery.dialog = Some(Dialog::replace_errors(c));
                }
                ui.close();
            }
        });
        fill_menu(app, ui);
        if big(ui, "Pivot Column", "Turn the values of the selected column into columns")
            && let Some(c) = need_columns(app).and_then(|c| c.first().cloned())
        {
            app.powerquery.dialog = Some(Dialog::pivot(app, &c));
        }
        unpivot_menu(app, ui);
        move_menu(app, ui);
        if big(ui, "Convert to List", "The selected column as a list")
            && let Some(c) = need_columns(app).and_then(|c| c.first().cloned())
        {
            add_step(app, &c, &format!("{{prev}}{}", field(&c)));
        }
    });
    ribbon_group(ui, "Text Column", |ui| {
        split_menu(app, ui);
        op_menu(app, ui, "Format", false);
        if big(ui, "Merge Columns", "Join the selected columns' text")
            && let Some(c) = need_columns(app)
        {
            app.powerquery.dialog = Some(Dialog::merge_columns(c, false));
        }
        op_menu(app, ui, "Extract", false);
        op_menu(app, ui, "Parse", false);
    });
    ribbon_group(ui, "Number Column", |ui| {
        statistics(app, ui);
        op_menu(app, ui, "Standard", false);
        op_menu(app, ui, "Scientific", false);
        op_menu(app, ui, "Rounding", false);
        op_menu(app, ui, "Information", false);
    });
    ribbon_group(ui, "Date & Time", |ui| {
        op_menu(app, ui, "Date", false);
        op_menu(app, ui, "Time", false);
        op_menu(app, ui, "Duration", false);
    });
    ribbon_group(ui, "Structured", |ui| {
        if big(ui, "Expand", "Expand the selected table, record or list column")
            && let Some(c) = need_columns(app).and_then(|c| c.first().cloned())
        {
            app.powerquery.dialog = Some(Dialog::expand(app, &c));
        }
    });
}

fn add_column(app: &mut SheetApp, ui: &mut Ui) {
    ribbon_group(ui, "General", |ui| {
        ribbon_menu(ui, "Column From Examples", |ui| {
            if ui.button("From All Columns").clicked() {
                app.powerquery.examples = Some(super::Examples { name: "Custom".into(), ..Default::default() });
                ui.close();
            }
            if ui.button("From Selection").clicked() {
                if let Some(c) = need_columns(app) {
                    app.powerquery.examples = Some(super::Examples { name: "Custom".into(), inputs: c, ..Default::default() });
                }
                ui.close();
            }
        });
        if big(ui, "Custom Column", "A new column computed by an M formula") {
            app.powerquery.dialog = Some(Dialog::custom_column(app));
        }
        if big(ui, "Conditional Column", "If … then … else rules") {
            app.powerquery.dialog = Some(Dialog::conditional(app));
        }
        index_menu(app, ui);
        if big(ui, "Duplicate Column", "Copy the selected column")
            && let Some(c) = need_columns(app).and_then(|c| c.first().cloned())
        {
            add_step(
                app,
                "Duplicated Column",
                &format!("Table.DuplicateColumn({{prev}}, {}, {})", quote_text(&c), quote_text(&format!("{c} - Copy"))),
            );
        }
    });
    ribbon_group(ui, "From Text", |ui| {
        op_menu(app, ui, "Format", true);
        op_menu(app, ui, "Extract", true);
        if big(ui, "Merge Columns", "A new column joining the selected columns' text")
            && let Some(c) = need_columns(app)
        {
            app.powerquery.dialog = Some(Dialog::merge_columns(c, true));
        }
        op_menu(app, ui, "Parse", true);
    });
    ribbon_group(ui, "From Number", |ui| {
        op_menu(app, ui, "Standard", true);
        op_menu(app, ui, "Scientific", true);
        op_menu(app, ui, "Rounding", true);
        op_menu(app, ui, "Information", true);
    });
    ribbon_group(ui, "From Date & Time", |ui| {
        op_menu(app, ui, "Date", true);
        op_menu(app, ui, "Time", true);
        op_menu(app, ui, "Duration", true);
    });
}

fn view(app: &mut SheetApp, ui: &mut Ui) {
    use crate::icons::Pq;
    let toggle = |ui: &mut Ui, icon: Icon, label: &str, tip: &str, on: &mut bool| {
        if icon_tool(ui, icon, label, tip, false, *on).clicked() {
            *on = !*on;
        }
    };
    ribbon_group(ui, "Layout", |ui| {
        let v = &mut app.powerquery.view;
        toggle(ui, Icon::Fx, "Formula Bar", "Show the selected step's M formula above the preview", &mut v.formula_bar);
        toggle(ui, Icon::Pq(Pq::Monospace), "Monospaced", "Show the preview in a fixed-width font", &mut v.monospace);
        toggle(ui, Icon::Pq(Pq::Whitespace), "Show Whitespace", "Mark spaces and line breaks in the preview", &mut v.whitespace);
    });
    ribbon_group(ui, "Data Preview", |ui| {
        let v = &mut app.powerquery.view;
        toggle(ui, Icon::Pq(Pq::Quality), "Column Quality", "Valid, error and empty percentages under each header", &mut v.quality);
        toggle(ui, Icon::ChartBar, "Column Distribution", "Distinct and unique counts with a small histogram under each header", &mut v.distribution);
        toggle(ui, Icon::Pq(Pq::Profile), "Column Profile", "Statistics and value distribution of the selected column", &mut v.profile);
        toggle(ui, Icon::Pq(Pq::AllRows), "Profile Entire Data Set", "Off: column profiling uses the top 1000 rows", &mut v.all_rows);
    });
    ribbon_group(ui, "Dependencies & Columns", |ui| {
        if big(ui, "Query Dependencies", "How queries, sources and loads connect") {
            app.powerquery.deps = true;
        }
        if big(ui, "Advanced Editor", "Edit the whole query in M") {
            app.powerquery.dialog = Some(Dialog::advanced(app));
        }
        ribbon_menu(ui, "Go to Column", |ui| {
            let cols = app.powerquery.preview(&app.session).map(|p| p.table.columns.clone()).unwrap_or_default();
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                for c in cols {
                    if ui.button(&c).clicked() {
                        app.powerquery.selected = vec![c];
                        ui.close();
                    }
                }
            });
        });
    });
}

/// The right-click menu of a column header.
pub fn column_menu(app: &mut SheetApp, ui: &mut Ui, col: &str) {
    if !app.powerquery.selected.iter().any(|c| c == col) {
        app.powerquery.selected = vec![col.to_string()];
    }
    if ui.button("Remove").clicked() {
        remove_columns(app, false);
        ui.close();
    }
    if ui.button("Remove Other Columns").clicked() {
        remove_columns(app, true);
        ui.close();
    }
    if ui.button("Duplicate Column").clicked() {
        add_step(
            app,
            "Duplicated Column",
            &format!("Table.DuplicateColumn({{prev}}, {}, {})", quote_text(col), quote_text(&format!("{col} - Copy"))),
        );
        ui.close();
    }
    if ui.button("Add Column From Examples…").clicked() {
        app.powerquery.examples = Some(super::Examples { name: "Custom".into(), inputs: app.powerquery.selected.clone(), ..Default::default() });
        ui.close();
    }
    ui.separator();
    if ui.button("Remove Duplicates").clicked() {
        let c = app.powerquery.selected.clone();
        add_step(app, "Removed Duplicates", &format!("Table.Distinct({{prev}}, {})", text_list(&c)));
        ui.close();
    }
    if ui.button("Remove Errors").clicked() {
        let c = app.powerquery.selected.clone();
        add_step(app, "Removed Errors", &format!("Table.RemoveRowsWithErrors({{prev}}, {})", text_list(&c)));
        ui.close();
    }
    ui.separator();
    type_menu(app, ui);
    ui.menu_button("Transform", |ui| {
        for m in ["Format", "Extract", "Standard", "Rounding", "Date"] {
            op_menu(app, ui, m, false);
        }
    });
    ui.separator();
    if ui.button("Replace Values…").clicked() {
        app.powerquery.dialog = Some(Dialog::replace(app.powerquery.selected.clone()));
        ui.close();
    }
    if ui.button("Replace Errors…").clicked() {
        app.powerquery.dialog = Some(Dialog::replace_errors(app.powerquery.selected.clone()));
        ui.close();
    }
    split_menu(app, ui);
    if ui.button("Group By…").clicked() {
        app.powerquery.dialog = Some(Dialog::group_by(app));
        ui.close();
    }
    fill_menu(app, ui);
    unpivot_menu(app, ui);
    if ui.button("Rename…").clicked() {
        app.powerquery.dialog = Some(Dialog::rename_column(col));
        ui.close();
    }
    move_menu(app, ui);
}
