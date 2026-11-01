//! Power Pivot: the workbook's data model. Worksheet tables are loaded into the native columnar
//! engine (`gridcraft-datamodel`), related to each other, and summarised with DAX measures in
//! data-model PivotTables, `EVALUATE` queries and single DAX evaluations.
//!
//! The model's definition (tables, relationships, measures, calculated columns, reports) is a
//! workbook stash item, so it is undone with the edit that changed it and travels inside the
//! xlsx (and JSON) file. The columnar store itself is rebuilt from the tables' current data
//! whenever it is needed, so it never goes stale.

use gridcraft_core::{CellError, CellRef, Value};
use gridcraft_datamodel::pivot::{PivotSpec, layout};
use gridcraft_datamodel::{CalcColumnDef, CrossFilter, DataModelDef, MeasureDef, Model, RelationshipDef, ReportDef, SourceData, TableDef};
use gridcraft_model::{Sheet, Workbook};
use serde_json::{Value as Json, json};

use super::{CommandSpec, EngineError, Result, Session, bad, bool_param, cell_param_on, cmd, edit, has_doc, stash, str_param, target_sheet};

/// The stash key the model definition is kept under.
const KEY: &str = "powerpivot";

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!("powerpivot.addTable", "Add to Data Model", ["Power Pivot", "Tables"], None, "{table?: \"Sales\"}", has_doc, add_table),
        cmd!("powerpivot.removeTable", "Remove from Data Model", ["Power Pivot", "Tables"], None, "{table: \"Sales\"}", has_doc, remove_table),
        cmd!(
            "powerpivot.createRelationship",
            "Create Relationship",
            ["Power Pivot", "Relationships"],
            None,
            "{from: \"Sales[ProductId]\", to: \"Products[ProductId]\", crossFilter?: \"single\"|\"both\", active?: true}",
            has_doc,
            create_relationship
        ),
        cmd!(
            "powerpivot.deleteRelationship",
            "Delete Relationship",
            ["Power Pivot", "Relationships"],
            None,
            "{from: \"Sales[ProductId]\", to: \"Products[ProductId]\"}",
            has_doc,
            delete_relationship
        ),
        cmd!(
            "powerpivot.newMeasure",
            "New Measure",
            ["Power Pivot", "Calculations"],
            None,
            "{table: \"Sales\", name: \"Total Sales\", expression: \"SUM(Sales[Amount])\", format?}",
            has_doc,
            new_measure
        ),
        cmd!("powerpivot.deleteMeasure", "Delete Measure", ["Power Pivot", "Calculations"], None, "{name: \"Total Sales\"}", has_doc, delete_measure),
        cmd!(
            "powerpivot.addColumn",
            "Add Calculated Column",
            ["Power Pivot", "Calculations"],
            None,
            "{table: \"Sales\", name: \"Margin\", expression: \"Sales[Amount] - Sales[Cost]\"}",
            has_doc,
            add_column
        ),
        cmd!(
            "powerpivot.deleteColumn",
            "Delete Calculated Column",
            ["Power Pivot", "Calculations"],
            None,
            "{table: \"Sales\", name: \"Margin\"}",
            has_doc,
            delete_column
        ),
        cmd!(
            "powerpivot.insertPivot",
            "PivotTable from Data Model",
            ["Power Pivot", "Reports"],
            None,
            "{rows?: [\"Products[Category]\"], columns?: [\"Calendar[Year]\"], values: [\"Total Sales\"], filters?: [\"Customers[Region] = \\\"East\\\"\"], at?: \"Sheet2!A1\", overwrite?: false}",
            has_doc,
            insert_pivot
        ),
        cmd!("powerpivot.refresh", "Refresh All", ["Power Pivot", "Data Model"], None, "{report?: id}", has_doc, refresh),
        cmd!("powerpivot.deletePivot", "Delete Data Model PivotTable", ["Power Pivot", "Reports"], None, "{report: id}", has_doc, delete_pivot),
        cmd!("powerpivot.addAllTables", "Add All Tables to Data Model", ["Power Pivot", "Tables"], None, "{}", has_doc, add_all_tables),
        cmd!(
            "powerpivot.editMeasure",
            "Edit Measure",
            ["Power Pivot", "Calculations"],
            None,
            "{name: \"Total Sales\", newName?, expression?, table?, format?}",
            has_doc,
            edit_measure
        ),
        cmd!(
            "powerpivot.editRelationship",
            "Edit Relationship",
            ["Power Pivot", "Relationships"],
            None,
            "{from: \"Sales[ProductId]\", to: \"Products[ProductId]\", active?: true, crossFilter?: \"single\"|\"both\"}",
            has_doc,
            edit_relationship
        ),
        cmd!(
            "powerpivot.editColumn",
            "Edit Calculated Column",
            ["Power Pivot", "Calculations"],
            None,
            "{table: \"Sales\", name: \"Margin\", expression?, newName?}",
            has_doc,
            edit_column
        ),
        cmd!("powerpivot.detectRelationships", "Detect Relationships", ["Power Pivot", "Relationships"], None, "{}", has_doc, detect_relationships),
        cmd!(
            "powerpivot.newDateTable",
            "New Date Table",
            ["Power Pivot", "Calendars"],
            None,
            "{column: \"Sales[OrderDate]\", name?: \"Calendar\"}",
            has_doc,
            new_date_table
        ),
        cmd!(query "powerpivot.model", "Manage Data Model", ["Power Pivot", "Data Model"], None, "{}", has_doc, describe),
        cmd!(query "powerpivot.evaluate", "Evaluate DAX", ["Power Pivot", "Data Model"], None, "{expression: \"[Total Sales]\", filters?: [\"Products[Category] = \\\"Office\\\"\"]}", has_doc, evaluate),
        cmd!(query "powerpivot.query", "DAX Query", ["Power Pivot", "Data Model"], None, "{query: \"EVALUATE VALUES(Products[Category])\"}", has_doc, query),
    ]
}

// ---------------------------------------------------------------- definition and model

/// The workbook's model definition (empty when it has none, or when the stored one can't be read).
pub fn definition(wb: &Workbook) -> DataModelDef {
    stash::get_json(wb, KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn store(wb: &mut Workbook, def: &DataModelDef) -> Result<()> {
    if def.is_empty() {
        stash::remove(wb, KEY);
        return Ok(());
    }
    let v = serde_json::to_value(def).map_err(|e| EngineError::Other(e.to_string()))?;
    stash::set_json(wb, KEY, &v)
}

fn to_model_value(v: &Value) -> gridcraft_datamodel::Value {
    use gridcraft_datamodel::Value as M;
    match v {
        Value::Number(n) => M::Number(*n),
        Value::Text(t) if t.is_empty() => M::Blank,
        Value::Text(t) => M::Text(t.clone()),
        Value::Bool(b) => M::Bool(*b),
        Value::Array(_) => to_model_value(&v.scalar()),
        Value::Empty | Value::Error(_) => M::Blank,
    }
}

fn to_cell_value(v: &gridcraft_datamodel::Value) -> Value {
    use gridcraft_datamodel::Value as M;
    match v {
        M::Blank => Value::Empty,
        M::Number(n) if n.is_nan() => Value::Error(CellError::Num),
        M::Number(n) if n.is_infinite() => Value::Error(CellError::Div0),
        M::Number(n) => Value::Number(*n),
        M::Text(t) => Value::Text(t.clone()),
        M::Bool(b) => Value::Bool(*b),
    }
}

/// The current data of the worksheet table `name`: its header names and data rows.
fn source(wb: &Workbook, name: &str) -> Option<SourceData> {
    let (si, ti) = wb.table(name)?;
    let sh = wb.sheet(si)?;
    let t = sh.tables.get(ti)?;
    let headers = t.columns.iter().map(|c| c.name.clone()).collect();
    let mut rows = Vec::new();
    if let Some(dr) = t.data_range() {
        for r in dr.start.row..=dr.end.row {
            rows.push((dr.start.col..=dr.end.col).map(|c| to_model_value(&sh.value(CellRef::new(r, c)))).collect());
        }
    }
    Some(SourceData { headers, rows })
}

/// Builds the model from `def` and the workbook's tables.
fn build(wb: &Workbook, def: &DataModelDef) -> Result<Model> {
    def.build(&|name| source(wb, name)).map_err(model_err)
}

fn model_err(e: gridcraft_datamodel::Error) -> EngineError {
    EngineError::Other(e.to_string())
}

/// The data model of `wb`, built from its tables' current data.
pub fn model_of(wb: &Workbook) -> Result<Model> {
    build(wb, &definition(wb))
}

/// The active workbook's model.
pub fn model(s: &Session) -> Result<Model> {
    model_of(&s.doc()?.wb)
}

/// Applies `change` to the definition, checks that the model still builds, and stores it (one
/// undo step). Reports are rewritten so they show the changed model.
fn change(s: &mut Session, cmd: &str, f: impl FnOnce(&Workbook, &mut DataModelDef) -> Result<Json>) -> Result<Json> {
    let cmd = cmd.to_string();
    edit(s, |cx| {
        let mut def = definition(&cx.wb);
        let out = f(&cx.wb, &mut def)?;
        let m = build(&cx.wb, &def).map_err(|e| bad(&cmd, e.to_string()))?;
        store(&mut cx.wb, &def)?;
        rewrite_reports(cx, &m, &mut def, None)?;
        store(&mut cx.wb, &def)?;
        Ok(out)
    })
}

fn req<'a>(p: &'a Json, key: &str, cmd: &str) -> Result<&'a str> {
    str_param(p, key).filter(|v| !v.trim().is_empty()).ok_or_else(|| bad(cmd, format!("`{key}` is required")))
}

/// `Table[Column]` as its two names.
fn column_param(p: &Json, key: &str, cmd: &str) -> Result<(String, String)> {
    let text = req(p, key, cmd)?;
    match gridcraft_datamodel::dax::parse(text) {
        Ok(gridcraft_datamodel::dax::Expr::Column { table, column }) => Ok((table, column)),
        _ => Err(bad(cmd, format!("`{key}` must be a column such as Products[ProductId], not `{text}`"))),
    }
}

fn string_list(p: &Json, key: &str) -> Vec<String> {
    match p.get(key) {
        Some(Json::Array(a)) => a.iter().filter_map(Json::as_str).map(str::to_string).collect(),
        Some(Json::String(s)) if !s.trim().is_empty() => vec![s.clone()],
        _ => vec![],
    }
}

// ---------------------------------------------------------------- tables

fn add_table(s: &mut Session, p: &Json) -> Result<Json> {
    let name = match str_param(p, "table") {
        Some(n) => {
            let (si, ti) = s.doc()?.wb.table(n).ok_or_else(|| bad("powerpivot.addTable", format!("no table named `{n}`")))?;
            s.doc()?.wb.sheet(si).and_then(|sh| sh.tables.get(ti)).map(|t| t.name.clone()).unwrap_or_default()
        }
        None => {
            let d = s.doc()?;
            let sh = d.wb.active().ok_or(EngineError::NoDocument)?;
            sh.table_at(d.selection.active)
                .map(|t| t.name.clone())
                .ok_or_else(|| EngineError::Other("Select a cell inside a table first (Insert › Table), or pass `table`.".into()))?
        }
    };
    change(s, "powerpivot.addTable", |_, def| {
        if def.has_table(&name) {
            return Err(bad("powerpivot.addTable", format!("`{name}` is already in the data model")));
        }
        def.tables.push(TableDef { name: name.clone() });
        Ok(json!({ "table": name }))
    })
}

fn remove_table(s: &mut Session, p: &Json) -> Result<Json> {
    let name = req(p, "table", "powerpivot.removeTable")?.to_string();
    change(s, "powerpivot.removeTable", |_, def| {
        if !def.has_table(&name) {
            return Err(bad("powerpivot.removeTable", format!("`{name}` is not in the data model")));
        }
        let is = |t: &str| t.eq_ignore_ascii_case(&name);
        def.tables.retain(|t| !is(&t.name));
        def.relationships.retain(|r| !is(&r.from_table) && !is(&r.to_table));
        def.measures.retain(|m| !is(&m.table));
        def.calculated_columns.retain(|c| !is(&c.table));
        Ok(Json::Null)
    })
}

// ---------------------------------------------------------------- relationships

fn create_relationship(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.createRelationship";
    let from = column_param(p, "from", CMD)?;
    let to = column_param(p, "to", CMD)?;
    let cross_filter = match str_param(p, "crossFilter").map(str::to_ascii_lowercase).as_deref() {
        None | Some("single") | Some("one") => CrossFilter::Single,
        Some("both") => CrossFilter::Both,
        Some(other) => return Err(bad(CMD, format!("crossFilter is \"single\" or \"both\", not \"{other}\""))),
    };
    let active = bool_param(p, "active").unwrap_or(true);
    change(s, CMD, |_, def| {
        def.relationships.push(RelationshipDef { from_table: from.0, from_column: from.1, to_table: to.0, to_column: to.1, cross_filter, active });
        Ok(Json::Null)
    })
}

fn delete_relationship(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.deleteRelationship";
    let a = column_param(p, "from", CMD)?;
    let b = column_param(p, "to", CMD)?;
    change(s, CMD, |_, def| {
        let same = |x: &(String, String), t: &str, c: &str| x.0.eq_ignore_ascii_case(t) && x.1.eq_ignore_ascii_case(c);
        let before = def.relationships.len();
        def.relationships.retain(|r| {
            !((same(&a, &r.from_table, &r.from_column) && same(&b, &r.to_table, &r.to_column))
                || (same(&b, &r.from_table, &r.from_column) && same(&a, &r.to_table, &r.to_column)))
        });
        if def.relationships.len() == before {
            return Err(bad(CMD, "there is no relationship between those columns"));
        }
        Ok(Json::Null)
    })
}

// ---------------------------------------------------------------- measures and calculated columns

fn new_measure(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.newMeasure";
    let table = req(p, "table", CMD)?.to_string();
    let name = req(p, "name", CMD)?.trim().to_string();
    let expression = req(p, "expression", CMD)?.to_string();
    let format = str_param(p, "format").map(str::to_string);
    let value = change(s, CMD, |_, def| {
        def.measures.push(MeasureDef { table, name: name.clone(), expression, format });
        Ok(Json::Null)
    })?;
    // The measure's value over the whole model, as a check the caller can read.
    let total = model(s).and_then(|m| m.evaluate(&format!("[{}]", name.replace(']', "]]"))).map_err(model_err));
    Ok(match total {
        Ok(v) => json!({ "name": name, "value": v.to_json() }),
        Err(e) => json!({ "name": name, "error": e.to_string(), "result": value }),
    })
}

fn delete_measure(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.deleteMeasure";
    let name = req(p, "name", CMD)?.to_string();
    change(s, CMD, |_, def| {
        let before = def.measures.len();
        def.measures.retain(|m| !m.name.eq_ignore_ascii_case(&name));
        if def.measures.len() == before {
            return Err(bad(CMD, format!("no measure named `{name}`")));
        }
        Ok(Json::Null)
    })
}

fn add_column(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.addColumn";
    let table = req(p, "table", CMD)?.to_string();
    let name = req(p, "name", CMD)?.trim().to_string();
    let expression = req(p, "expression", CMD)?.to_string();
    change(s, CMD, |_, def| {
        def.calculated_columns.push(CalcColumnDef { table, name, expression });
        Ok(Json::Null)
    })
}

fn delete_column(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.deleteColumn";
    let table = req(p, "table", CMD)?.to_string();
    let name = req(p, "name", CMD)?.to_string();
    change(s, CMD, |_, def| {
        let before = def.calculated_columns.len();
        def.calculated_columns.retain(|c| !(c.table.eq_ignore_ascii_case(&table) && c.name.eq_ignore_ascii_case(&name)));
        if def.calculated_columns.len() == before {
            return Err(bad(CMD, format!("`{table}` has no calculated column named `{name}`")));
        }
        Ok(Json::Null)
    })
}

fn add_all_tables(s: &mut Session, _: &Json) -> Result<Json> {
    let names: Vec<String> = s.doc()?.wb.sheets.iter().flat_map(|sh| sh.tables.iter().map(|t| t.name.clone())).collect();
    change(s, "powerpivot.addAllTables", |_, def| {
        let mut added = Vec::new();
        for n in names {
            if !def.has_table(&n) {
                def.tables.push(TableDef { name: n.clone() });
                added.push(n);
            }
        }
        if added.is_empty() {
            return Err(bad("powerpivot.addAllTables", "every table in the workbook is already in the data model"));
        }
        Ok(json!({ "added": added }))
    })
}

fn edit_measure(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.editMeasure";
    let name = req(p, "name", CMD)?.to_string();
    let new_name = str_param(p, "newName").map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    let expression = str_param(p, "expression").map(str::to_string);
    let table = str_param(p, "table").map(str::to_string);
    let format = p.get("format").map(|f| f.as_str().map(str::to_string).filter(|f| !f.trim().is_empty()));
    change(s, CMD, |_, def| {
        let m = def.measures.iter_mut().find(|m| m.name.eq_ignore_ascii_case(&name)).ok_or_else(|| bad(CMD, format!("no measure named `{name}`")))?;
        let old = m.name.clone();
        if let Some(n) = new_name {
            m.name = n;
        }
        if let Some(e) = expression {
            m.expression = e;
        }
        if let Some(t) = table {
            m.table = t;
        }
        if let Some(f) = format {
            m.format = f;
        }
        let renamed = (!m.name.eq_ignore_ascii_case(&old)).then(|| m.name.clone());
        // Reports that show the measure follow its new name.
        if let Some(n) = renamed {
            for r in &mut def.reports {
                for v in &mut r.values {
                    if v.trim().trim_start_matches('[').trim_end_matches(']').eq_ignore_ascii_case(&old) {
                        *v = n.clone();
                    }
                }
            }
        }
        Ok(Json::Null)
    })
}

fn edit_relationship(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.editRelationship";
    let a = column_param(p, "from", CMD)?;
    let b = column_param(p, "to", CMD)?;
    let active = bool_param(p, "active");
    let cross = match str_param(p, "crossFilter").map(str::to_ascii_lowercase).as_deref() {
        None => None,
        Some("single") | Some("one") => Some(CrossFilter::Single),
        Some("both") => Some(CrossFilter::Both),
        Some(other) => return Err(bad(CMD, format!("crossFilter is \"single\" or \"both\", not \"{other}\""))),
    };
    change(s, CMD, |_, def| {
        let same = |x: &(String, String), t: &str, c: &str| x.0.eq_ignore_ascii_case(t) && x.1.eq_ignore_ascii_case(c);
        let pos = def
            .relationships
            .iter()
            .position(|r| {
                (same(&a, &r.from_table, &r.from_column) && same(&b, &r.to_table, &r.to_column))
                    || (same(&b, &r.from_table, &r.from_column) && same(&a, &r.to_table, &r.to_column))
            })
            .ok_or_else(|| bad(CMD, "there is no relationship between those columns"))?;
        if active == Some(true) {
            // Only one active relationship between two tables: activating one turns the others off.
            let (t1, t2) = def.relationships.get(pos).map(|r| (r.from_table.to_lowercase(), r.to_table.to_lowercase())).unwrap_or_default();
            for (i, r) in def.relationships.iter_mut().enumerate() {
                let pair = (r.from_table.to_lowercase(), r.to_table.to_lowercase());
                if i != pos && (pair == (t1.clone(), t2.clone()) || pair == (t2.clone(), t1.clone())) {
                    r.active = false;
                }
            }
        }
        if let Some(r) = def.relationships.get_mut(pos) {
            if let Some(v) = active {
                r.active = v;
            }
            if let Some(c) = cross {
                r.cross_filter = c;
            }
        }
        Ok(Json::Null)
    })
}

fn edit_column(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.editColumn";
    let table = req(p, "table", CMD)?.to_string();
    let name = req(p, "name", CMD)?.to_string();
    let expression = str_param(p, "expression").map(str::to_string);
    let new_name = str_param(p, "newName").map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    change(s, CMD, |_, def| {
        let c = def
            .calculated_columns
            .iter_mut()
            .find(|c| c.table.eq_ignore_ascii_case(&table) && c.name.eq_ignore_ascii_case(&name))
            .ok_or_else(|| bad(CMD, format!("`{table}` has no calculated column named `{name}`")))?;
        if let Some(e) = expression {
            c.expression = e;
        }
        if let Some(n) = new_name {
            c.name = n;
        }
        Ok(Json::Null)
    })
}

fn detect_relationships(s: &mut Session, _: &Json) -> Result<Json> {
    let found = model(s)?.suggest_relationships();
    if found.is_empty() {
        return Ok(json!({ "created": [] }));
    }
    change(s, "powerpivot.detectRelationships", |_, def| {
        let mut created = Vec::new();
        for ((ft, fc), (tt, tc)) in found {
            created.push(format!("{ft}[{fc}] → {tt}[{tc}]"));
            def.relationships.push(RelationshipDef {
                from_table: ft,
                from_column: fc,
                to_table: tt,
                to_column: tc,
                cross_filter: CrossFilter::Single,
                active: true,
            });
        }
        Ok(json!({ "created": created }))
    })
}

/// A calendar table covering whole years from the earliest to the latest date in `column`, added
/// to the model and related to that column (one undo step).
fn new_date_table(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.newDateTable";
    let (table, column) = column_param(p, "column", CMD)?;
    let name = str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or("Calendar").to_string();
    let m = model(s)?;
    let (ti, ci) = m.column_ref(&table, &column).map_err(model_err)?;
    let col = m.tables.get(ti).and_then(|t| t.column(ci)).ok_or_else(|| bad(CMD, "no such column"))?;
    let dates: Vec<f64> =
        (0..col.cardinality() as u32).filter_map(|id| col.dict_value(id).as_number()).filter(|d| (1.0..=2_958_465.0).contains(d)).collect();
    let (Some(lo), Some(hi)) = (dates.iter().copied().reduce(f64::min), dates.iter().copied().reduce(f64::max)) else {
        return Err(bad(CMD, format!("{table}[{column}] holds no dates")));
    };
    use gridcraft_datamodel::dates::{to_date, ymd};
    let (Some(d0), Some(d1)) = (to_date(lo), to_date(hi)) else { return Err(bad(CMD, "the dates are out of range")) };
    use chrono::Datelike;
    let (Some(start), Some(end)) = (ymd(d0.year(), 1, 1), ymd(d1.year(), 12, 31)) else { return Err(bad(CMD, "the dates are out of range")) };
    if end - start > 366.0 * 60.0 {
        return Err(bad(CMD, "the dates span more than 60 years"));
    }
    if s.doc()?.wb.table(&name).is_some() {
        return Err(bad(CMD, format!("a table named `{name}` already exists")));
    }
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    let mut rows = vec![json!(["Date", "Year", "Quarter", "Month Number", "Month", "Year Month", "Day", "Day of Week", "Weekday"])];
    let mut d = start;
    while d <= end {
        if let Some(dt) = to_date(d) {
            let mi = dt.month0() as usize;
            let wd = dt.weekday().num_days_from_sunday() as usize;
            rows.push(json!([
                d,
                dt.year(),
                format!("Q{}", mi / 3 + 1),
                mi + 1,
                MONTHS.get(mi).copied().unwrap_or_default(),
                format!("{}-{:02}", dt.year(), mi + 1),
                dt.day(),
                wd + 1,
                DAYS.get(wd).copied().unwrap_or_default()
            ]));
        }
        d += 1.0;
    }
    let n = rows.len();
    s.grouped("New Date Table", |s| {
        let sheet_name = (1..)
            .map(|i| if i == 1 { name.clone() } else { format!("{name} {i}") })
            .find(|n| s.doc().map(|d| d.wb.sheet_index(n).is_none()).unwrap_or(false))
            .unwrap_or_default();
        let end = s.doc()?.wb.sheets.len();
        let r = s.execute("home.insertSheet", json!({ "name": sheet_name, "before": end }))?;
        let si = r["index"].as_u64().unwrap_or(end as u64);
        s.execute("sheet.activate", json!({ "sheet": si }))?;
        s.execute("range.setValues", json!({ "range": "A1", "values": rows, "parse": false }))?;
        s.execute("home.numberFormat", json!({ "range": format!("A2:A{n}"), "code": "yyyy-mm-dd" }))?;
        s.execute("insert.table", json!({ "range": format!("A1:I{n}"), "name": name }))?;
        s.execute("powerpivot.addTable", json!({ "table": name }))?;
        s.execute(
            "powerpivot.createRelationship",
            json!({ "from": format!("{}[{}]", quote(&table), column), "to": format!("{}[Date]", quote(&name)) }),
        )?;
        Ok(json!({ "table": name, "rows": n - 1 }))
    })
}

fn quote(t: &str) -> String {
    gridcraft_datamodel::model::quote_table(t)
}

// ---------------------------------------------------------------- queries

fn describe(s: &mut Session, _: &Json) -> Result<Json> {
    let wb = &s.doc()?.wb;
    let def = definition(wb);
    let mut out = build(wb, &def)?.describe();
    if let Some(o) = out.as_object_mut() {
        o.insert("reports".into(), serde_json::to_value(&def.reports).unwrap_or_default());
    }
    Ok(out)
}

fn evaluate(s: &mut Session, p: &Json) -> Result<Json> {
    let e = req(p, "expression", "powerpivot.evaluate")?;
    let filters = string_list(p, "filters");
    let refs: Vec<&str> = filters.iter().map(String::as_str).collect();
    let v = model(s)?.evaluate_filtered(e, &refs).map_err(model_err)?;
    Ok(json!({ "value": v.to_json() }))
}

fn query(s: &mut Session, p: &Json) -> Result<Json> {
    let q = req(p, "query", "powerpivot.query")?;
    Ok(model(s)?.query(q).map_err(model_err)?.to_json())
}

// ---------------------------------------------------------------- reports

/// A sheet name not yet used: `Data Model Pivot`, `Data Model Pivot 2`, …
fn free_sheet_name(wb: &Workbook) -> String {
    (1..)
        .map(|i| if i == 1 { "Data Model Pivot".to_string() } else { format!("Data Model Pivot {i}") })
        .find(|n| wb.sheet_index(n).is_none())
        .unwrap_or_default()
}

/// Inserts the report and fits its columns to their contents, as one undo step.
fn insert_pivot(s: &mut Session, p: &Json) -> Result<Json> {
    s.grouped("PivotTable from Data Model", |s| {
        let r = insert_pivot_cells(s, p)?;
        let (Some(sheet), Some(at), Some(cols)) = (r["sheet"].as_str(), r["at"].as_str().and_then(CellRef::parse), r["columns"].as_u64()) else {
            return Ok(r);
        };
        if cols > 0 {
            let last = (at.col as u64 + cols - 1).min(gridcraft_core::MAX_COLS as u64 - 1) as u32;
            let range = gridcraft_core::RangeRef::cols(at.col, last).a1();
            let _ = s.execute("home.autofitColumnWidth", json!({ "sheet": sheet, "cols": range }));
        }
        Ok(r)
    })
}

fn insert_pivot_cells(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.insertPivot";
    let spec = PivotSpec {
        rows: string_list(p, "rows"),
        columns: string_list(p, "columns"),
        values: string_list(p, "values"),
        filters: string_list(p, "filters"),
    };
    let m = model(s)?;
    if m.tables.is_empty() {
        return Err(EngineError::Other("The data model is empty: add a table first (powerpivot.addTable).".into()));
    }
    let grid = layout(&m, &spec).map_err(|e| bad(CMD, e.to_string()))?;
    // `at` names a cell (optionally on another sheet); without it the report goes on a new sheet.
    let (sheet_name, at) = match p.get("at").and_then(Json::as_str) {
        Some(text) => {
            let si = target_sheet(s, &json!({ "range": text }))?;
            let cell = cell_param_on(s, &json!({ "at": text }), "at", si)?.unwrap_or_default();
            (s.doc()?.wb.sheet(si).map(|sh| sh.name.clone()).unwrap_or_default(), Some(cell))
        }
        None => (free_sheet_name(&s.doc()?.wb), None),
    };
    let overwrite = bool_param(p, "overwrite").unwrap_or(false);
    edit(s, |cx| {
        let mut def = definition(&cx.wb);
        if at.is_none() {
            cx.wb.sheets.push(std::sync::Arc::new(Sheet::new(sheet_name.clone())));
            cx.structural = true;
        }
        let id = def.reports.iter().map(|r| r.id).max().unwrap_or(0).saturating_add(1);
        let at = at.unwrap_or_default();
        def.reports.push(ReportDef {
            id,
            sheet: sheet_name.clone(),
            at: at.a1(),
            rows: spec.rows.clone(),
            columns: spec.columns.clone(),
            values: spec.values.clone(),
            filters: spec.filters.clone(),
            extent: (0, 0),
        });
        if !overwrite {
            check_free(&cx.wb, &sheet_name, at, &grid.grid)?;
        }
        rewrite_reports(cx, &m, &mut def, Some(id))?;
        store(&mut cx.wb, &def)?;
        Ok(
            json!({ "report": id, "sheet": sheet_name, "at": at.a1(), "rows": grid.grid.len(), "columns": grid.grid.first().map(Vec::len).unwrap_or(0) }),
        )
    })
}

/// Refuses to write a new report over cells that hold something.
fn check_free(wb: &Workbook, sheet: &str, at: CellRef, grid: &[Vec<gridcraft_datamodel::Value>]) -> Result<()> {
    let Some(sh) = wb.sheet_index(sheet).and_then(|i| wb.sheet(i)) else { return Ok(()) };
    for (r, line) in grid.iter().enumerate() {
        for c in 0..line.len() {
            let cell = CellRef::new(at.row.saturating_add(r as u32), at.col.saturating_add(c as u32));
            if !sh.value(cell).is_empty() {
                return Err(EngineError::Other(format!(
                    "The report would overwrite {}; pass overwrite: true to replace it, or choose another `at`.",
                    cell.a1()
                )));
            }
        }
    }
    Ok(())
}

fn refresh(s: &mut Session, p: &Json) -> Result<Json> {
    let only = p.get("report").and_then(Json::as_u64).map(|v| v.min(u32::MAX as u64) as u32);
    let m = model(s)?;
    edit(s, |cx| {
        let mut def = definition(&cx.wb);
        if let Some(id) = only
            && !def.reports.iter().any(|r| r.id == id)
        {
            return Err(bad("powerpivot.refresh", format!("no report with id {id}")));
        }
        let n = rewrite_reports(cx, &m, &mut def, only)?;
        store(&mut cx.wb, &def)?;
        Ok(json!({ "refreshed": n, "tables": m.tables.iter().map(|t| json!({"name": t.name, "rows": t.row_count()})).collect::<Vec<_>>() }))
    })
}

fn delete_pivot(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerpivot.deletePivot";
    let id = p.get("report").and_then(Json::as_u64).ok_or_else(|| bad(CMD, "`report` is required"))?;
    edit(s, |cx| {
        let mut def = definition(&cx.wb);
        let pos = def.reports.iter().position(|r| u64::from(r.id) == id).ok_or_else(|| bad(CMD, format!("no report with id {id}")))?;
        let r = def.reports.remove(pos);
        clear_extent(cx, &r);
        store(&mut cx.wb, &def)?;
        Ok(Json::Null)
    })
}

/// Clears the block a report wrote last time.
fn clear_extent(cx: &mut super::Ctx, r: &ReportDef) {
    let (Some(si), Some(at)) = (cx.wb.sheet_index(&r.sheet), CellRef::parse(&r.at)) else { return };
    let mut cleared = Vec::new();
    if let Some(sh) = cx.wb.sheet_mut(si) {
        for dr in 0..r.extent.0 {
            for dc in 0..r.extent.1 {
                let c = CellRef::new(at.row.saturating_add(dr), at.col.saturating_add(dc));
                if sh.remove_cell(c).is_some() {
                    cleared.push((si, c));
                }
            }
        }
    }
    cx.changed.extend(cleared);
}

/// Header rows and the grand-total row in bold; value cells in their measure's number format.
fn style_report(cx: &mut super::Ctx, si: usize, at: CellRef, grid: &gridcraft_datamodel::pivot::PivotGrid) {
    let last = grid.grid.len().saturating_sub(1);
    let mut wanted = Vec::new();
    for (dr, line) in grid.grid.iter().enumerate() {
        for dc in 0..line.len() {
            let bold = dr < grid.header_rows || dr == last;
            let fmt = if dr >= grid.header_rows {
                grid.value_columns.get(dc).copied().flatten().and_then(|v| grid.value_formats.get(v).cloned().flatten())
            } else {
                None
            };
            if !bold && fmt.is_none() {
                continue;
            }
            let (Ok(r), Ok(c)) = (u32::try_from(dr), u32::try_from(dc)) else { continue };
            let cell = CellRef::new(at.row.saturating_add(r), at.col.saturating_add(c));
            if cell.row >= gridcraft_core::MAX_ROWS || cell.col >= gridcraft_core::MAX_COLS {
                continue;
            }
            wanted.push((cell, bold, fmt));
        }
    }
    let mut ids = Vec::with_capacity(wanted.len());
    for (cell, bold, fmt) in wanted {
        let base = cx.wb.sheet(si).map(|sh| sh.style_id(cell)).unwrap_or_default();
        let id = cx.wb.styles.derive(base, |st| {
            if bold {
                st.font.bold = true;
            }
            if let Some(f) = &fmt {
                st.num_fmt = gridcraft_model::NumFmt::new(f);
            }
        });
        ids.push((cell, id));
    }
    if let Some(sh) = cx.wb.sheet_mut(si) {
        for (cell, id) in ids {
            sh.set_style(cell, id);
        }
    }
}

/// Recomputes and writes every report (or only `only`); a report whose sheet is gone, or whose
/// fields no longer exist, is left as it was. Returns how many were written.
fn rewrite_reports(cx: &mut super::Ctx, m: &Model, def: &mut DataModelDef, only: Option<u32>) -> Result<usize> {
    let mut written = 0;
    for r in def.reports.iter_mut().filter(|r| only.is_none_or(|id| id == r.id)) {
        let spec = PivotSpec { rows: r.rows.clone(), columns: r.columns.clone(), values: r.values.clone(), filters: r.filters.clone() };
        let Ok(grid) = layout(m, &spec) else { continue };
        let (Some(si), Some(at)) = (cx.wb.sheet_index(&r.sheet), CellRef::parse(&r.at)) else { continue };
        clear_extent(cx, r);
        let mut changed = Vec::new();
        if let Some(sh) = cx.wb.sheet_mut(si) {
            for (dr, line) in grid.grid.iter().enumerate() {
                for (dc, v) in line.iter().enumerate() {
                    let (Ok(dr), Ok(dc)) = (u32::try_from(dr), u32::try_from(dc)) else { continue };
                    let (row, col) = (at.row.saturating_add(dr), at.col.saturating_add(dc));
                    if row >= gridcraft_core::MAX_ROWS || col >= gridcraft_core::MAX_COLS {
                        continue;
                    }
                    let c = CellRef::new(row, col);
                    sh.set_value(c, to_cell_value(v));
                    changed.push((si, c));
                }
            }
        }
        cx.changed.extend(changed);
        style_report(cx, si, at, &grid);
        r.extent = (grid.grid.len() as u32, grid.grid.iter().map(Vec::len).max().unwrap_or(0) as u32);
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests;
