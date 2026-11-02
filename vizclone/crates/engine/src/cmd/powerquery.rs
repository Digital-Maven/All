//! Power Query: queries written in M (`gridcraft-query`) that get data from the workbook's
//! tables, files and folders, transform it step by step, and load it into a worksheet table or
//! straight into the data model (Power Pivot), Power BI style.
//!
//! The queries and parameters are a workbook stash item, so they undo with the edit that changed
//! them and travel inside the xlsx (and JSON) file. A query's data is computed whenever it is
//! needed (preview, load, refresh, the data model), so it never goes stale.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gridcraft_core::{CellError, CellRef, RangeRef, Value as CellValue};
use gridcraft_model::{NumFmt, Sheet, Workbook};
use gridcraft_query::m::eval::Host;
use gridcraft_query::steps::{self, LoadTo, Parameter, QueryDef, QueryDoc, ident, text_list};
use gridcraft_query::value::quote_text;
use gridcraft_query::{Error as QError, MType, Table, Value};
use serde_json::{Value as Json, json};

use super::{CommandSpec, EngineError, Result, Session, bad, bool_param, cmd, edit, has_doc, stash, str_param};

const KEY: &str = "powerquery";
/// Rows a preview computes by default.
pub const PREVIEW_ROWS: usize = 1000;
/// The largest file a query reads.
const MAX_FILE: u64 = 1 << 30;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "powerquery.newQuery",
            "Get Data",
            ["Data", "Get & Transform Data"],
            None,
            "{name?, source: {kind: \"table\", table} | {kind: \"csv\", path, delimiter?} | {kind: \"excel\", path, item?} | {kind: \"json\", path} | {kind: \"folder\", path} | {kind: \"text\", path} | {kind: \"web\", url} | {kind: \"enter\", columns, rows} | {kind: \"blank\"} | {kind: \"m\", m}}",
            has_doc,
            new_query
        ),
        cmd!(
            "powerquery.setQuery",
            "Edit Query",
            ["Data", "Queries & Connections"],
            None,
            "{name, m?, newName?, load?: \"table\"|\"model\"|\"none\", description?, group?}",
            has_doc,
            set_query
        ),
        cmd!("powerquery.deleteQuery", "Delete Query", ["Data", "Queries & Connections"], None, "{name}", has_doc, delete_query),
        cmd!("powerquery.duplicateQuery", "Duplicate Query", ["Data", "Queries & Connections"], None, "{name}", has_doc, duplicate_query),
        cmd!("powerquery.referenceQuery", "Reference Query", ["Data", "Queries & Connections"], None, "{name}", has_doc, reference_query),
        cmd!(
            "powerquery.addStep",
            "Add Step",
            ["Data", "Power Query"],
            None,
            "{query, name: \"Filtered Rows\", expr: \"Table.SelectRows({prev}, each [Units] > 0)\", after?: step}",
            has_doc,
            add_step
        ),
        cmd!("powerquery.deleteStep", "Delete Step", ["Data", "Power Query"], None, "{query, step, untilEnd?: false}", has_doc, delete_step),
        cmd!("powerquery.renameStep", "Rename Step", ["Data", "Power Query"], None, "{query, step, newName}", has_doc, rename_step),
        cmd!("powerquery.editStep", "Edit Step", ["Data", "Power Query"], None, "{query, step, expr}", has_doc, edit_step),
        cmd!("powerquery.moveStep", "Move Step", ["Data", "Power Query"], None, "{query, step, delta: -1|1}", has_doc, move_step),
        cmd!("powerquery.load", "Close & Load", ["Data", "Power Query"], None, "{query?, to?: \"table\"|\"model\"|\"none\"}", has_doc, load),
        cmd!("powerquery.refresh", "Refresh All", ["Data", "Queries & Connections"], None, "{query?}", has_doc, refresh),
        cmd!(
            "powerquery.setParameter",
            "Manage Parameters",
            ["Data", "Power Query"],
            None,
            "{name, value: \"East\" | 100 | \"#date(2024,1,1)\", kind?: \"Text\"|\"Decimal Number\"|\"Date\"|\"True/False\", description?, newName?}",
            has_doc,
            set_parameter
        ),
        cmd!("powerquery.deleteParameter", "Delete Parameter", ["Data", "Power Query"], None, "{name}", has_doc, delete_parameter),
        cmd!(query "powerquery.list", "Queries & Connections", ["Data", "Queries & Connections"], None, "{}", has_doc, list),
        cmd!(query "powerquery.preview", "Preview Query", ["Data", "Power Query"], None, "{query, step?, rows?: 1000}", has_doc, preview),
        cmd!(query "powerquery.evaluate", "Evaluate M", ["Data", "Power Query"], None, "{m: \"Text.Upper(\\\"abc\\\")\"}", has_doc, evaluate_cmd),
        cmd!(
            query "powerquery.suggestColumn",
            "Column From Examples",
            ["Data", "Power Query"],
            None,
            "{query, step?, columns?: [\"Name\"], examples: [[0, \"John\"], [1, \"Mary\"]]}",
            has_doc,
            suggest_column
        ),
    ]
}

// ---------------------------------------------------------------- document

pub fn document(wb: &Workbook) -> QueryDoc {
    stash::get_json(wb, KEY).and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn store(wb: &mut Workbook, doc: &QueryDoc) -> Result<()> {
    if doc.is_empty() {
        stash::remove(wb, KEY);
        return Ok(());
    }
    let v = serde_json::to_value(doc).map_err(|e| EngineError::Other(e.to_string()))?;
    stash::set_json(wb, KEY, &v)
}

fn qerr(e: QError) -> EngineError {
    EngineError::Other(e.message())
}

fn change(s: &mut Session, f: impl FnOnce(&Workbook, &mut QueryDoc) -> Result<Json>) -> Result<Json> {
    edit(s, |cx| {
        let mut doc = document(&cx.wb);
        let out = f(&cx.wb, &mut doc)?;
        store(&mut cx.wb, &doc)?;
        Ok(out)
    })
}

fn req<'a>(p: &'a Json, key: &str, cmd: &str) -> Result<&'a str> {
    str_param(p, key).filter(|v| !v.trim().is_empty()).ok_or_else(|| bad(cmd, format!("`{key}` is required")))
}

fn query_mut<'a>(doc: &'a mut QueryDoc, name: &str, cmd: &str) -> Result<&'a mut QueryDef> {
    doc.query_mut(name).ok_or_else(|| bad(cmd, format!("there is no query named `{name}`")))
}

// ---------------------------------------------------------------- the host: workbook, files, other queries

/// A worksheet cell as an M value.
pub fn to_m(v: &CellValue) -> Value {
    match v {
        CellValue::Empty => Value::Null,
        CellValue::Number(n) => Value::Number(*n),
        CellValue::Text(t) => Value::Text(Rc::from(&**t)),
        CellValue::Bool(b) => Value::Logical(*b),
        CellValue::Error(e) => Value::error(format!("{e:?}")),
        CellValue::Array(_) => to_m(&v.scalar()),
    }
}

/// An M value as a worksheet cell (dates as serial numbers).
pub fn to_cell(v: &Value) -> CellValue {
    match v {
        Value::Null => CellValue::Empty,
        Value::Number(n) if n.is_finite() => CellValue::Number(*n),
        Value::Number(_) => CellValue::Error(CellError::Num),
        Value::Text(t) => CellValue::Text(std::sync::Arc::from(&**t)),
        Value::Logical(b) => CellValue::Bool(*b),
        Value::Date(d) => CellValue::Number(*d as f64),
        Value::DateTime(x) | Value::Time(x) | Value::Duration(x) => CellValue::Number(*x),
        Value::Error(_) => CellError::Value.into(),
        other => CellValue::Text(std::sync::Arc::from(other.display().as_str())),
    }
}

/// A worksheet table's data as an M table (header names, typed values).
fn sheet_table(wb: &Workbook, si: usize, ti: usize) -> Option<Table> {
    let sh = wb.sheet(si)?;
    let t = sh.tables.get(ti)?;
    let columns: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
    let mut rows = Vec::new();
    if let Some(dr) = t.data_range() {
        for r in dr.start.row..=dr.end.row {
            rows.push((dr.start.col..=dr.end.col).map(|c| to_m(&sh.value(CellRef::new(r, c)))).collect());
        }
    }
    Some(Table::new(columns, rows))
}

/// A sheet's used range as an M table with `Column1…` names.
fn sheet_data(sh: &Sheet) -> Table {
    let mut max_r = 0u32;
    let mut max_c = 0u32;
    let mut any = false;
    for (c, _) in sh.cells.iter() {
        any = true;
        max_r = max_r.max(c.row);
        max_c = max_c.max(c.col);
    }
    if !any {
        return Table::new(vec![], vec![]);
    }
    let (max_r, max_c) = (max_r.min(1_048_575), max_c.min(16_383));
    let columns = (1..=max_c + 1).map(|i| format!("Column{i}")).collect();
    let rows = (0..=max_r).map(|r| (0..=max_c).map(|c| to_m(&sh.value(CellRef::new(r, c)))).collect()).collect();
    Table::new(columns, rows)
}

struct WbHost<'a> {
    wb: &'a Workbook,
    doc: &'a QueryDoc,
    stack: RefCell<Vec<String>>,
    cache: RefCell<HashMap<String, gridcraft_query::Result<Value>>>,
}

impl<'a> WbHost<'a> {
    fn new(wb: &'a Workbook, doc: &'a QueryDoc) -> WbHost<'a> {
        WbHost { wb, doc, stack: RefCell::new(Vec::new()), cache: RefCell::new(HashMap::new()) }
    }

    fn eval_query(&self, name: &str, m: &str) -> gridcraft_query::Result<Value> {
        if let Some(v) = self.cache.borrow().get(name) {
            return v.clone();
        }
        if self.stack.borrow().iter().any(|n| n == name) {
            return Err(QError::Eval(format!("The query '{name}' refers to itself (directly or through other queries).")));
        }
        if self.stack.borrow().len() > 64 {
            return Err(QError::Eval("Queries refer to each other too deeply.".into()));
        }
        self.stack.borrow_mut().push(name.to_string());
        let r = gridcraft_query::evaluate(m, self);
        self.stack.borrow_mut().pop();
        self.cache.borrow_mut().insert(name.to_string(), r.clone());
        r
    }
}

impl Host for WbHost<'_> {
    fn resolve(&self, name: &str) -> Option<gridcraft_query::Result<Value>> {
        if let Some(p) = self.doc.parameters.iter().find(|p| p.name == name) {
            return Some(gridcraft_query::evaluate(&p.value, &gridcraft_query::NoHost));
        }
        let q = self.doc.query(name)?;
        Some(self.eval_query(&q.name, &q.m))
    }

    fn current_workbook(&self) -> gridcraft_query::Result<Table> {
        let mut rows = Vec::new();
        for (si, sh) in self.wb.sheets.iter().enumerate() {
            for (ti, t) in sh.tables.iter().enumerate() {
                if let Some(data) = sheet_table(self.wb, si, ti) {
                    rows.push(vec![Value::table(data), Value::text(&t.name)]);
                }
            }
        }
        Ok(Table::new(vec!["Content".into(), "Name".into()], rows))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn file_contents(&self, path: &str) -> gridcraft_query::Result<Vec<u8>> {
        let meta = std::fs::metadata(path).map_err(|e| QError::Eval(format!("DataSource.Error: Could not find file '{path}' ({e}).")))?;
        if meta.len() > MAX_FILE {
            return Err(QError::Eval(format!("The file '{path}' is too large.")));
        }
        std::fs::read(path).map_err(|e| QError::Eval(format!("DataSource.Error: Could not read '{path}' ({e}).")))
    }

    fn excel_workbook(&self, bytes: &[u8], _use_headers: bool) -> gridcraft_query::Result<Table> {
        let (wb, _) = crate::io::open_bytes("source.xlsx", bytes)
            .map_err(|e| QError::Eval(format!("DataFormat.Error: The file isn't a workbook we can read ({e}).")))?;
        let mut rows = Vec::new();
        for (si, sh) in wb.sheets.iter().enumerate() {
            rows.push(vec![Value::text(&sh.name), Value::table(sheet_data(sh)), Value::text(&sh.name), Value::text("Sheet"), Value::Logical(false)]);
            for (ti, t) in sh.tables.iter().enumerate() {
                if let Some(data) = sheet_table(&wb, si, ti) {
                    rows.push(vec![Value::text(&t.name), Value::table(data), Value::text(&t.name), Value::text("Table"), Value::Logical(false)]);
                }
            }
        }
        Ok(Table::new(vec!["Name".into(), "Data".into(), "Item".into(), "Kind".into(), "Hidden".into()], rows))
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn folder_files(&self, path: &str) -> gridcraft_query::Result<Table> {
        let mut rows = Vec::new();
        let mut total = 0u64;
        let mut stack = vec![std::path::PathBuf::from(path)];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir)
                .map_err(|e| QError::Eval(format!("DataSource.Error: Could not find the folder '{}' ({e}).", dir.display())))?;
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                let meta = e.metadata().ok();
                total += meta.as_ref().map(|m| m.len()).unwrap_or(0);
                if total > MAX_FILE * 2 {
                    return Err(QError::Eval("The folder holds too much data to read.".into()));
                }
                let content = std::fs::read(&p).unwrap_or_default();
                let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                let ext = p.extension().map(|x| format!(".{}", x.to_string_lossy())).unwrap_or_default();
                let modified = meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| Value::DateTime(25569.0 + d.as_secs_f64() / 86400.0))
                    .unwrap_or_default();
                let folder = format!("{}{}", dir.display(), std::path::MAIN_SEPARATOR);
                rows.push(vec![Value::Binary(Rc::new(content)), Value::text(name), Value::text(ext), modified, Value::text(folder)]);
            }
        }
        Ok(Table::new(vec!["Content".into(), "Name".into(), "Extension".into(), "Date modified".into(), "Folder Path".into()], rows))
    }
}

/// Evaluates a query (or the query truncated at `step`) against the workbook.
pub fn evaluate_query(wb: &Workbook, doc: &QueryDoc, name: &str, step: Option<&str>) -> std::result::Result<Value, String> {
    let q = doc.query(name).ok_or_else(|| format!("There is no query named {name}."))?;
    let host = WbHost::new(wb, doc);
    let r = match step {
        Some(st) => steps::m_at_step(&q.m, st).and_then(|m| gridcraft_query::evaluate(&m, &host)),
        None => host.eval_query(&q.name, &q.m),
    };
    r.map_err(|e| e.message())
}

/// Evaluates M text with the workbook's queries and parameters in scope.
pub fn evaluate_m(wb: &Workbook, doc: &QueryDoc, m: &str) -> std::result::Result<Value, String> {
    gridcraft_query::evaluate(m, &WbHost::new(wb, doc)).map_err(|e| e.message())
}

fn as_table(v: Value) -> std::result::Result<Rc<Table>, String> {
    match v {
        Value::Table(t) => Ok(t),
        Value::List(l) => Ok(Rc::new(Table::new(vec!["Value".into()], l.iter().map(|x| vec![x.clone()]).collect()))),
        Value::Record(r) => Ok(Rc::new(Table::new(
            vec!["Name".into(), "Value".into()],
            r.names.iter().zip(r.values.iter()).map(|(n, v)| vec![Value::text(n), v.clone()]).collect(),
        ))),
        other => Ok(Rc::new(Table::new(vec!["Value".into()], vec![vec![other]]))),
    }
}

/// A query's result as data-model rows (for Power Pivot tables loaded from queries).
pub fn source_data(wb: &Workbook, query: &str) -> Option<gridcraft_datamodel::SourceData> {
    let doc = document(wb);
    let t = evaluate_query(wb, &doc, query, None).and_then(as_table).ok()?;
    let rows = t
        .rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|v| match to_cell(v) {
                    CellValue::Number(n) => gridcraft_datamodel::Value::Number(n),
                    CellValue::Text(s) if s.is_empty() => gridcraft_datamodel::Value::Blank,
                    CellValue::Text(s) => gridcraft_datamodel::Value::Text(s),
                    CellValue::Bool(b) => gridcraft_datamodel::Value::Bool(b),
                    _ => gridcraft_datamodel::Value::Blank,
                })
                .collect()
        })
        .collect();
    Some(gridcraft_datamodel::SourceData { headers: t.columns.clone(), rows })
}

// ---------------------------------------------------------------- new queries

/// A `Changed Type` step from the types the data suggests (`None` when nothing to change).
fn detect_types_step(wb: &Workbook, doc: &QueryDoc, m: &str) -> Option<String> {
    let v = evaluate_m(wb, doc, m).ok()?;
    let Value::Table(t) = v else { return None };
    let mut pairs = Vec::new();
    for (c, name) in t.columns.iter().enumerate() {
        let vals: Vec<Value> = t.rows.iter().take(200).map(|r| r.get(c).cloned().unwrap_or_default()).collect();
        let ty = gridcraft_query::funcs::convert::detect(&vals);
        if ty != MType::Any {
            pairs.push(format!("{{{}, {}}}", quote_text(name), ty.m()));
        }
    }
    (!pairs.is_empty()).then(|| format!("{{{}}}", pairs.join(", ")))
}

fn source_steps(wb: &Workbook, p: &Json) -> Result<(String, Vec<(String, String)>)> {
    const CMD: &str = "powerquery.newQuery";
    let src = p.get("source").cloned().unwrap_or(Json::Null);
    let kind = src.get("kind").and_then(Json::as_str).unwrap_or("blank");
    let s = |k: &str| src.get(k).and_then(Json::as_str).map(str::to_string);
    let path = || s("path").filter(|x| !x.trim().is_empty()).ok_or_else(|| bad(CMD, "`source.path` is required"));
    let file_name = |p: &str| std::path::Path::new(p).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| "Query".into());
    Ok(match kind {
        "table" => {
            let t = s("table").ok_or_else(|| bad(CMD, "`source.table` is required"))?;
            if wb.table(&t).is_none() {
                return Err(bad(CMD, format!("there is no table named `{t}`")));
            }
            (t.clone(), vec![("Source".into(), format!("Excel.CurrentWorkbook(){{[Name={}]}}[Content]", quote_text(&t)))])
        }
        "csv" => {
            let path = path()?;
            let delim = s("delimiter").map(|d| format!("Delimiter={}, ", quote_text(&d))).unwrap_or_default();
            (
                file_name(&path),
                vec![
                    (
                        "Source".into(),
                        format!("Csv.Document(File.Contents({}), [{delim}Encoding=65001, QuoteStyle=QuoteStyle.Csv])", quote_text(&path)),
                    ),
                    ("Promoted Headers".into(), "Table.PromoteHeaders(Source, [PromoteAllScalars=true])".into()),
                ],
            )
        }
        "text" => {
            let path = path()?;
            (
                file_name(&path),
                vec![("Source".into(), format!("Table.FromColumns({{Lines.FromBinary(File.Contents({}))}}, {{\"Column1\"}})", quote_text(&path)))],
            )
        }
        "excel" => {
            let path = path()?;
            let mut steps = vec![("Source".into(), format!("Excel.Workbook(File.Contents({}), null, true)", quote_text(&path)))];
            if let Some(item) = s("item") {
                let kind = s("itemKind").unwrap_or_else(|| "Sheet".into());
                let step = format!("{item}_{kind}");
                steps.push((step.clone(), format!("Source{{[Item={}, Kind={}]}}[Data]", quote_text(&item), quote_text(&kind))));
                if kind == "Sheet" {
                    steps.push(("Promoted Headers".into(), format!("Table.PromoteHeaders({}, [PromoteAllScalars=true])", ident(&step))));
                }
                (item, steps)
            } else {
                (file_name(&path), steps)
            }
        }
        "json" => {
            let path = path()?;
            (
                file_name(&path),
                vec![
                    ("Source".into(), format!("Json.Document(File.Contents({}))", quote_text(&path))),
                    (
                        "Converted to Table".into(),
                        "if Source is list then Table.FromRecords(List.Select(Source, each _ is record)) else Record.ToTable(Source)".into(),
                    ),
                ],
            )
        }
        "folder" => {
            let path = path()?;
            (file_name(&path), vec![("Source".into(), format!("Folder.Files({})", quote_text(&path)))])
        }
        "web" => {
            let url = s("url").ok_or_else(|| bad(CMD, "`source.url` is required"))?;
            ("Web".into(), vec![("Source".into(), format!("Csv.Document(Web.Contents({}))", quote_text(&url)))])
        }
        "enter" => {
            let cols: Vec<String> = src
                .get("columns")
                .and_then(Json::as_array)
                .map(|a| a.iter().filter_map(Json::as_str).map(str::to_string).collect())
                .unwrap_or_default();
            if cols.is_empty() {
                return Err(bad(CMD, "`source.columns` is required"));
            }
            let rows: Vec<String> = src
                .get("rows")
                .and_then(Json::as_array)
                .map(|a| {
                    a.iter()
                        .map(|r| {
                            let cells: Vec<String> = r.as_array().map(|c| c.iter().map(json_literal).collect()).unwrap_or_default();
                            format!("{{{}}}", cells.join(", "))
                        })
                        .collect()
                })
                .unwrap_or_default();
            ("Table".into(), vec![("Source".into(), format!("#table({}, {{{}}})", text_list(&cols), rows.join(", ")))])
        }
        "m" => {
            let m = s("m").ok_or_else(|| bad(CMD, "`source.m` is required"))?;
            gridcraft_query::m::parse(&m).map_err(|e| bad(CMD, e.message()))?;
            ("Query".into(), vec![("__raw".into(), m)])
        }
        _ => ("Query".into(), vec![("Source".into(), "\"\"".into())]),
    })
}

fn json_literal(v: &Json) -> String {
    match v {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.to_string(),
        Json::String(s) => quote_text(s),
        other => quote_text(&other.to_string()),
    }
}

fn new_query(s: &mut Session, p: &Json) -> Result<Json> {
    let (default_name, parts) = source_steps(&s.doc()?.wb, p)?;
    let detect = bool_param(p, "detectTypes").unwrap_or(true);
    change(s, |wb, doc| {
        let name = doc.free_name(str_param(p, "name").map(str::trim).filter(|n| !n.is_empty()).unwrap_or(&default_name));
        let mut m = match parts.first() {
            Some((n, raw)) if n == "__raw" => raw.clone(),
            _ => steps::build_m(&parts.iter().map(|(n, e)| steps::Step { name: n.clone(), expr: e.clone() }).collect::<Vec<_>>()),
        };
        if detect
            && !parts.iter().any(|(n, _)| n == "__raw")
            && let Some(types) = detect_types_step(wb, doc, &m)
        {
            m = steps::add_step(&m, None, "Changed Type", |prev| format!("Table.TransformColumnTypes({prev}, {types})")).map_err(qerr)?.0;
        }
        doc.queries.push(QueryDef { name: name.clone(), m, load: LoadTo::Table, loaded_table: None, group: None, description: String::new() });
        Ok(json!({ "name": name }))
    })
}

// ---------------------------------------------------------------- query edits

fn load_param(p: &Json, cmd: &str) -> Result<Option<LoadTo>> {
    Ok(match str_param(p, "load").or_else(|| str_param(p, "to")) {
        None => None,
        Some("table") => Some(LoadTo::Table),
        Some("model") => Some(LoadTo::Model),
        Some("none") | Some("connection") => Some(LoadTo::None),
        Some(other) => return Err(bad(cmd, format!("load is \"table\", \"model\" or \"none\", not \"{other}\""))),
    })
}

fn set_query(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.setQuery";
    let name = req(p, "name", CMD)?.to_string();
    let m = str_param(p, "m").map(str::to_string);
    if let Some(m) = &m {
        gridcraft_query::m::parse(m).map_err(|e| bad(CMD, e.message()))?;
    }
    let new_name = str_param(p, "newName").map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    let load = load_param(p, CMD)?;
    change(s, |_, doc| {
        if let Some(n) = &new_name
            && n != &name
            && doc.query(n).is_some()
        {
            return Err(bad(CMD, format!("there is already a query named `{n}`")));
        }
        let q = query_mut(doc, &name, CMD)?;
        if let Some(m) = m {
            q.m = m;
        }
        if let Some(l) = load {
            q.load = l;
        }
        if let Some(d) = str_param(p, "description") {
            q.description = d.to_string();
        }
        if let Some(g) = str_param(p, "group") {
            q.group = (!g.is_empty()).then(|| g.to_string());
        }
        if let Some(n) = new_name {
            q.name = n.clone();
            // Other queries that use it follow the new name.
            for other in doc.queries.iter_mut() {
                other.m = steps::rename_refs(&other.m, &name, &n);
            }
        }
        Ok(Json::Null)
    })
}

fn delete_query(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.deleteQuery";
    let name = req(p, "name", CMD)?.to_string();
    change(s, |_, doc| {
        let users: Vec<String> =
            doc.queries.iter().filter(|q| q.name != name && doc.dependencies(&q.name).contains(&name)).map(|q| q.name.clone()).collect();
        if !users.is_empty() {
            return Err(bad(CMD, format!("`{name}` is used by {}; change those queries first", users.join(", "))));
        }
        let before = doc.queries.len();
        doc.queries.retain(|q| q.name != name);
        if doc.queries.len() == before {
            return Err(bad(CMD, format!("there is no query named `{name}`")));
        }
        Ok(Json::Null)
    })
}

fn duplicate_query(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.duplicateQuery";
    let name = req(p, "name", CMD)?.to_string();
    change(s, |_, doc| {
        let q = doc.query(&name).cloned().ok_or_else(|| bad(CMD, format!("there is no query named `{name}`")))?;
        let new = doc.free_name(&name);
        doc.queries.push(QueryDef { name: new.clone(), loaded_table: None, ..q });
        Ok(json!({ "name": new }))
    })
}

fn reference_query(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.referenceQuery";
    let name = req(p, "name", CMD)?.to_string();
    change(s, |_, doc| {
        if doc.query(&name).is_none() {
            return Err(bad(CMD, format!("there is no query named `{name}`")));
        }
        let new = doc.free_name(&name);
        let m = steps::build_m(&[steps::Step { name: "Source".into(), expr: ident(&name) }]);
        doc.queries.push(QueryDef { name: new.clone(), m, load: LoadTo::Table, loaded_table: None, group: None, description: String::new() });
        Ok(json!({ "name": new }))
    })
}

fn step_change(s: &mut Session, p: &Json, cmd: &str, f: impl FnOnce(&str) -> gridcraft_query::Result<(String, Json)>) -> Result<Json> {
    let query = req(p, "query", cmd)?.to_string();
    let cmd = cmd.to_string();
    change(s, |_, doc| {
        let q = query_mut(doc, &query, &cmd)?;
        let (m, out) = f(&q.m).map_err(|e| bad(&cmd, e.message()))?;
        gridcraft_query::m::parse(&m).map_err(|e| bad(&cmd, e.message()))?;
        q.m = m;
        Ok(out)
    })
}

fn add_step(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.addStep";
    let name = req(p, "name", CMD)?.to_string();
    let expr = req(p, "expr", CMD)?.to_string();
    let after = str_param(p, "after").map(str::to_string);
    step_change(s, p, CMD, |m| {
        let (m, step) = steps::add_step(m, after.as_deref(), &name, |prev| expr.replace("{prev}", prev))?;
        Ok((m, json!({ "step": step })))
    })
}

fn delete_step(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.deleteStep";
    let step = req(p, "step", CMD)?.to_string();
    let until_end = bool_param(p, "untilEnd").unwrap_or(false);
    step_change(s, p, CMD, |m| Ok((if until_end { steps::delete_until_end(m, &step)? } else { steps::delete_step(m, &step)? }, Json::Null)))
}

fn rename_step(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.renameStep";
    let step = req(p, "step", CMD)?.to_string();
    let new = req(p, "newName", CMD)?.to_string();
    step_change(s, p, CMD, |m| Ok((steps::rename_step(m, &step, &new)?, Json::Null)))
}

fn edit_step(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.editStep";
    let step = req(p, "step", CMD)?.to_string();
    let expr = req(p, "expr", CMD)?.to_string();
    step_change(s, p, CMD, |m| Ok((steps::set_step_expr(m, &step, &expr)?, Json::Null)))
}

fn move_step(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.moveStep";
    let step = req(p, "step", CMD)?.to_string();
    let delta = p.get("delta").and_then(Json::as_i64).unwrap_or(1).clamp(-1, 1) as i32;
    step_change(s, p, CMD, |m| Ok((steps::move_step(m, &step, delta)?, Json::Null)))
}

// ---------------------------------------------------------------- parameters

fn set_parameter(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.setParameter";
    let name = req(p, "name", CMD)?.trim().to_string();
    let value = match p.get("value") {
        Some(Json::String(t)) if t.starts_with('#') || t.starts_with('"') => t.clone(),
        Some(Json::String(t)) => quote_text(t),
        Some(v @ (Json::Number(_) | Json::Bool(_) | Json::Null)) => json_literal(v),
        _ => return Err(bad(CMD, "`value` is required")),
    };
    gridcraft_query::evaluate(&value, &gridcraft_query::NoHost).map_err(|e| bad(CMD, e.message()))?;
    let new_name = str_param(p, "newName").map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    change(s, |_, doc| {
        if doc.queries.iter().any(|q| q.name == name) {
            return Err(bad(CMD, format!("`{name}` is already the name of a query")));
        }
        let kind = str_param(p, "kind").map(str::to_string);
        let description = str_param(p, "description").map(str::to_string);
        match doc.parameters.iter_mut().find(|x| x.name == name) {
            Some(par) => {
                par.value = value;
                if let Some(k) = kind {
                    par.kind = k;
                }
                if let Some(d) = description {
                    par.description = d;
                }
            }
            None => doc.parameters.push(Parameter {
                name: name.clone(),
                kind: kind.unwrap_or_else(|| "Any".into()),
                value,
                description: description.unwrap_or_default(),
                suggestions: vec![],
            }),
        }
        if let Some(n) = new_name {
            if let Some(par) = doc.parameters.iter_mut().find(|x| x.name == name) {
                par.name = n.clone();
            }
            for q in doc.queries.iter_mut() {
                q.m = steps::rename_refs(&q.m, &name, &n);
            }
        }
        Ok(Json::Null)
    })
}

fn delete_parameter(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.deleteParameter";
    let name = req(p, "name", CMD)?.to_string();
    change(s, |_, doc| {
        let before = doc.parameters.len();
        doc.parameters.retain(|x| x.name != name);
        if doc.parameters.len() == before {
            return Err(bad(CMD, format!("there is no parameter named `{name}`")));
        }
        Ok(Json::Null)
    })
}

// ---------------------------------------------------------------- read-only

fn list(s: &mut Session, _: &Json) -> Result<Json> {
    let doc = document(&s.doc()?.wb);
    let queries: Vec<Json> = doc
        .queries
        .iter()
        .map(|q| {
            let steps: Vec<Json> = steps::steps_of(&q.m).map(|st| st.into_iter().map(|x| json!({ "name": x.name, "expr": x.expr })).collect()).unwrap_or_default();
            json!({ "name": q.name, "m": q.m, "load": q.load, "loadedTable": q.loaded_table, "group": q.group, "description": q.description, "steps": steps, "dependsOn": doc.dependencies(&q.name) })
        })
        .collect();
    Ok(json!({ "queries": queries, "parameters": doc.parameters }))
}

pub fn table_json(t: &Table, rows: usize) -> Json {
    json!({
        "columns": t.columns,
        "types": t.types.iter().map(|x| x.name()).collect::<Vec<_>>(),
        "rowCount": t.rows.len(),
        "rows": t.rows.iter().take(rows).map(|r| r.iter().map(|v| match v {
            Value::Null => Json::Null,
            Value::Number(n) => serde_json::Number::from_f64(*n).map(Json::Number).unwrap_or(Json::Null),
            Value::Logical(b) => Json::Bool(*b),
            other => Json::String(other.display()),
        }).collect::<Vec<_>>()).collect::<Vec<_>>(),
    })
}

fn preview(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.preview";
    let wb = &s.doc()?.wb;
    let doc = document(wb);
    let name = req(p, "query", CMD)?;
    let rows = p.get("rows").and_then(Json::as_u64).unwrap_or(PREVIEW_ROWS as u64).min(100_000) as usize;
    match evaluate_query(wb, &doc, name, str_param(p, "step")) {
        Ok(Value::Table(t)) => Ok(table_json(&t, rows)),
        Ok(v) => Ok(json!({ "value": v.display(), "kind": v.kind() })),
        Err(e) => Err(EngineError::Other(e)),
    }
}

fn evaluate_cmd(s: &mut Session, p: &Json) -> Result<Json> {
    let wb = &s.doc()?.wb;
    let doc = document(wb);
    let m = req(p, "m", "powerquery.evaluate")?;
    match evaluate_m(wb, &doc, m) {
        Ok(Value::Table(t)) => Ok(table_json(&t, 1000)),
        Ok(v) => Ok(json!({ "value": v.display(), "kind": v.kind() })),
        Err(e) => Err(EngineError::Other(e)),
    }
}

fn suggest_column(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.suggestColumn";
    let wb = &s.doc()?.wb;
    let doc = document(wb);
    let name = req(p, "query", CMD)?;
    let t = evaluate_query(wb, &doc, name, str_param(p, "step")).and_then(as_table).map_err(EngineError::Other)?;
    let cols: Vec<usize> = p
        .get("columns")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(Json::as_str).filter_map(|c| t.column_index(c)).collect())
        .unwrap_or_default();
    let examples: Vec<(usize, String)> = p
        .get("examples")
        .and_then(Json::as_array)
        .map(|a| a.iter().filter_map(|e| Some((e.get(0)?.as_u64()? as usize, e.get(1)?.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let out = gridcraft_query::examples::suggest(&t, &cols, &examples);
    Ok(json!({ "suggestions": out.iter().map(|x| json!({ "expr": x.expr, "type": x.ty.name() })).collect::<Vec<_>>() }))
}

// ---------------------------------------------------------------- load and refresh

/// A worksheet table name from a query name.
fn table_name_for(wb: &Workbook, query: &str) -> String {
    let mut n: String = query.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if !n.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        n.insert(0, '_');
    }
    if n.len() < 3 || CellRef::parse(&n).is_some() {
        n.insert_str(0, "Query_");
    }
    if wb.table(&n).is_none() {
        return n;
    }
    (2..).map(|i| format!("{n}_{i}")).find(|x| wb.table(x).is_none()).unwrap_or(n)
}

/// Writes a query's result as a worksheet table: in place of the table it loaded before, or on
/// a new sheet named after the query.
fn load_to_table(s: &mut Session, name: &str, t: &Table, existing: Option<String>) -> Result<String> {
    let wb = &s.doc()?.wb;
    let found = existing.as_deref().and_then(|e| wb.table(e).map(|(si, ti)| (e.to_string(), si, ti)));
    let (table_name, sheet, at, old) = match found {
        Some((tn, si, ti)) => {
            let range = wb.sheet(si).and_then(|sh| sh.tables.get(ti)).map(|t| t.range);
            (tn, si, range.map(|r| r.start).unwrap_or_default(), range)
        }
        None => {
            let tn = table_name_for(wb, name);
            let sheet_name =
                (1..)
                    .map(|i| {
                        if i == 1 {
                            name.chars().take(31).collect::<String>()
                        } else {
                            format!("{} ({i})", name.chars().take(26).collect::<String>())
                        }
                    })
                    .find(|n| wb.sheet_index(n).is_none())
                    .unwrap_or_default();
            let end = wb.sheets.len();
            let r = s.execute("home.insertSheet", json!({ "name": sheet_name, "before": end }))?;
            (tn, r["index"].as_u64().unwrap_or(end as u64) as usize, CellRef::new(0, 0), None)
        }
    };
    let ncols = t.columns.len().clamp(1, 16_000) as u32;
    let nrows = t.rows.len().min(1_048_000) as u32;
    let date_cols: Vec<(u32, &str)> = t
        .types
        .iter()
        .enumerate()
        .filter_map(|(i, ty)| match ty {
            MType::Date => Some((i as u32, "yyyy-mm-dd")),
            MType::DateTime => Some((i as u32, "yyyy-mm-dd hh:mm:ss")),
            MType::Time => Some((i as u32, "hh:mm:ss")),
            MType::Percentage => Some((i as u32, "0.00%")),
            _ => None,
        })
        .collect();
    let name_owned = table_name.clone();
    edit(s, |cx| {
        // Clear the previous load (and its table definition).
        if let Some(r) = old {
            if let Some(sh) = cx.wb.sheet_mut(sheet) {
                sh.tables.retain(|tb| tb.name != name_owned);
            }
            let mut cleared = Vec::new();
            if let Some(sh) = cx.wb.sheet_mut(sheet) {
                for row in r.start.row..=r.end.row {
                    for col in r.start.col..=r.end.col {
                        let c = CellRef::new(row, col);
                        if sh.remove_cell(c).is_some() {
                            cleared.push((sheet, c));
                        }
                    }
                }
            }
            cx.changed.extend(cleared);
            cx.structural = true;
        }
        let styles: Vec<(u32, gridcraft_model::StyleId)> =
            date_cols.iter().map(|(c, f)| (*c, cx.wb.styles.derive(gridcraft_model::StyleId::default(), |st| st.num_fmt = NumFmt::new(f)))).collect();
        let mut changed = Vec::new();
        if let Some(sh) = cx.wb.sheet_mut(sheet) {
            for (c, h) in t.columns.iter().enumerate().take(ncols as usize) {
                let cell = CellRef::new(at.row, at.col + c as u32);
                sh.set_value(cell, CellValue::Text(std::sync::Arc::from(h.as_str())));
                changed.push((sheet, cell));
            }
            for (r, row) in t.rows.iter().enumerate().take(nrows as usize) {
                for (c, v) in row.iter().enumerate().take(ncols as usize) {
                    let cell = CellRef::new(at.row + 1 + r as u32, at.col + c as u32);
                    let cv = to_cell(v);
                    if !matches!(cv, CellValue::Empty) {
                        sh.set_value(cell, cv);
                        changed.push((sheet, cell));
                    }
                    if let Some((_, st)) = styles.iter().find(|(dc, _)| *dc == c as u32) {
                        sh.set_style(cell, *st);
                    }
                }
            }
        }
        cx.changed.extend(changed);
        Ok(())
    })?;
    let range = RangeRef::new(at, CellRef::new(at.row + nrows.max(1), at.col + ncols - 1));
    let sheet_name = s.doc()?.wb.sheet(sheet).map(|sh| sh.name.clone()).unwrap_or_default();
    s.execute(
        "insert.table",
        json!({ "range": format!("{}!{}", gridcraft_core::quote_sheet(&sheet_name), range.a1()), "name": table_name, "header": true }),
    )?;
    let _ = s.execute("home.autofitColumnWidth", json!({ "sheet": sheet_name, "cols": RangeRef::cols(at.col, at.col + ncols - 1).a1() }));
    Ok(table_name)
}

fn load_one(s: &mut Session, name: &str, to: Option<LoadTo>) -> Result<Json> {
    const CMD: &str = "powerquery.load";
    let wb = &s.doc()?.wb;
    let doc = document(wb);
    let q = doc.query(name).cloned().ok_or_else(|| bad(CMD, format!("there is no query named `{name}`")))?;
    let to = to.unwrap_or(q.load);
    match to {
        LoadTo::None => {
            change(s, |_, d| {
                query_mut(d, name, CMD)?.load = LoadTo::None;
                Ok(Json::Null)
            })?;
            Ok(json!({ "query": name, "load": "none" }))
        }
        LoadTo::Model => {
            // Check that it evaluates, then make it a data-model table fed by the query.
            let t = evaluate_query(wb, &doc, name, None).and_then(as_table).map_err(|e| bad(CMD, e))?;
            change(s, |_, d| {
                query_mut(d, name, CMD)?.load = LoadTo::Model;
                Ok(Json::Null)
            })?;
            edit(s, |cx| {
                let mut def = super::powerpivot::definition(&cx.wb);
                if !def.tables.iter().any(|t| t.name == name) {
                    def.tables.push(gridcraft_datamodel::TableDef { name: name.to_string(), query: Some(name.to_string()) });
                }
                let v = serde_json::to_value(&def).map_err(|e| EngineError::Other(e.to_string()))?;
                stash::set_json(&mut cx.wb, "powerpivot", &v)
            })?;
            Ok(json!({ "query": name, "load": "model", "rows": t.rows.len() }))
        }
        LoadTo::Table => {
            let t = evaluate_query(wb, &doc, name, None).and_then(as_table).map_err(|e| bad(CMD, e))?;
            let table = load_to_table(s, name, &t, q.loaded_table.clone())?;
            let tn = table.clone();
            change(s, |_, d| {
                let q = query_mut(d, name, CMD)?;
                q.load = LoadTo::Table;
                q.loaded_table = Some(tn);
                Ok(Json::Null)
            })?;
            Ok(json!({ "query": name, "load": "table", "table": table, "rows": t.rows.len() }))
        }
    }
}

fn load(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "powerquery.load";
    let to = load_param(p, CMD)?;
    let names: Vec<String> = match str_param(p, "query") {
        Some(q) => vec![q.to_string()],
        None => document(&s.doc()?.wb).queries.iter().map(|q| q.name.clone()).collect(),
    };
    s.grouped("Close & Load", |s| {
        let mut out = Vec::new();
        for n in names {
            out.push(load_one(s, &n, to)?);
        }
        Ok(json!({ "loaded": out }))
    })
}

fn refresh(s: &mut Session, p: &Json) -> Result<Json> {
    let doc = document(&s.doc()?.wb);
    let only = str_param(p, "query").map(str::to_string);
    let names: Vec<String> = doc
        .queries
        .iter()
        .filter(|q| only.as_ref().is_none_or(|o| *o == q.name))
        .filter(|q| q.load != LoadTo::Table || q.loaded_table.is_some())
        .map(|q| q.name.clone())
        .collect();
    s.grouped("Refresh", |s| {
        let mut out = Vec::new();
        for n in names {
            out.push(load_one(s, &n, None)?);
        }
        // Data-model reports follow refreshed data.
        let _ = s.execute("powerpivot.refresh", json!({}));
        Ok(json!({ "refreshed": out }))
    })
}

#[cfg(test)]
mod tests;
