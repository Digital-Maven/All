use gridcraft_core::{CellRef, Value};
use serde_json::{Value as Json, json};

use crate::Session;

fn book() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let rows = json!([
        ["Name", "Region", "Units", "Price"],
        ["John Smith", "East", 10, 1.5],
        ["Mary Jones", "West", 5, 2],
        ["Ann Lee", "East", 2, 12],
        ["Bob Ray", "North", 0, 3]
    ]);
    s.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    s.execute("insert.table", json!({"range": "A1:D5", "name": "Sales"})).unwrap();
    s
}

fn preview(s: &mut Session, q: &str) -> Json {
    s.execute("powerquery.preview", json!({"query": q})).unwrap()
}

fn column(p: &Json, name: &str) -> Vec<Json> {
    let c = p["columns"].as_array().unwrap().iter().position(|c| c == name).unwrap_or_else(|| panic!("no {name} in {p}"));
    p["rows"].as_array().unwrap().iter().map(|r| r[c].clone()).collect()
}

fn cell(s: &Session, sheet: &str, a1: &str) -> Value {
    let wb = &s.doc().unwrap().wb;
    wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap().value(CellRef::parse(a1).unwrap())
}

#[test]
fn query_from_a_table_with_steps_loads_and_refreshes() {
    let mut s = book();
    let r = s.execute("powerquery.newQuery", json!({"source": {"kind": "table", "table": "Sales"}})).unwrap();
    assert_eq!(r["name"], "Sales");
    let list = s.execute("powerquery.list", json!({})).unwrap();
    let steps: Vec<&str> = list["queries"][0]["steps"].as_array().unwrap().iter().map(|x| x["name"].as_str().unwrap()).collect();
    assert_eq!(steps, vec!["Source", "Changed Type"]);
    s.execute("powerquery.addStep", json!({"query": "Sales", "name": "Filtered Rows", "expr": "Table.SelectRows({prev}, each [Units] > 0)"}))
        .unwrap();
    s.execute(
        "powerquery.addStep",
        json!({"query": "Sales", "name": "Added Custom", "expr": "Table.AddColumn({prev}, \"Revenue\", each [Units] * [Price], type number)"}),
    )
    .unwrap();
    let p = preview(&mut s, "Sales");
    assert_eq!(column(&p, "Revenue"), vec![json!(15.0), json!(10.0), json!(24.0)]);
    // A step preview shows the data at that step.
    let p = s.execute("powerquery.preview", json!({"query": "Sales", "step": "Changed Type"})).unwrap();
    assert_eq!(p["rowCount"], 4);
    // Close & Load writes a table on a new sheet.
    let r = s.execute("powerquery.load", json!({"query": "Sales"})).unwrap();
    let table = r["loaded"][0]["table"].as_str().unwrap().to_string();
    assert_ne!(table, "Sales");
    assert_eq!(cell(&s, "Sales", "E1"), Value::text("Revenue"));
    assert_eq!(cell(&s, "Sales", "E4"), Value::Number(24.0));
    // The source changes; Refresh rewrites the loaded table.
    s.execute("cell.set", json!({"cell": "Sheet1!C5", "input": "4"})).unwrap();
    s.execute("powerquery.refresh", json!({})).unwrap();
    assert_eq!(cell(&s, "Sales", "E5"), Value::Number(12.0));
    assert!(s.doc().unwrap().wb.table(&table).is_some());
    // Delete a step: the next one takes over its input.
    s.execute("powerquery.deleteStep", json!({"query": "Sales", "step": "Filtered Rows"})).unwrap();
    assert_eq!(preview(&mut s, "Sales")["rowCount"], 4);
    s.execute("edit.undo", json!({})).unwrap();
    assert_eq!(preview(&mut s, "Sales")["rowCount"], 4);
    let m = s.execute("powerquery.list", json!({})).unwrap()["queries"][0]["m"].as_str().unwrap().to_string();
    assert!(m.contains("Filtered Rows"), "{m}");
}

#[test]
fn queries_reference_each_other_and_parameters() {
    let mut s = book();
    s.execute("powerquery.newQuery", json!({"name": "Base", "source": {"kind": "table", "table": "Sales"}})).unwrap();
    s.execute("powerquery.setParameter", json!({"name": "Region", "value": "East", "kind": "Text"})).unwrap();
    let r = s.execute("powerquery.referenceQuery", json!({"name": "Base"})).unwrap();
    let q = r["name"].as_str().unwrap().to_string();
    s.execute("powerquery.addStep", json!({"query": q, "name": "Filtered Rows", "expr": "Table.SelectRows({prev}, each [Region] = Region)"}))
        .unwrap();
    assert_eq!(preview(&mut s, &q)["rowCount"], 2);
    s.execute("powerquery.setParameter", json!({"name": "Region", "value": "West"})).unwrap();
    assert_eq!(preview(&mut s, &q)["rowCount"], 1);
    // Renaming a query rewrites the queries that use it; deleting a used query is refused.
    s.execute("powerquery.setQuery", json!({"name": "Base", "newName": "Sales Base"})).unwrap();
    assert_eq!(preview(&mut s, &q)["rowCount"], 1);
    assert!(s.execute("powerquery.deleteQuery", json!({"name": "Sales Base"})).is_err());
    // Merge and append through M.
    let m = "let A = #table({\"Region\", \"Manager\"}, {{\"East\", \"Kim\"}, {\"West\", \"Raj\"}}), Merged = Table.NestedJoin(#\"Sales Base\", {\"Region\"}, A, {\"Region\"}, \"M\", JoinKind.LeftOuter), Expanded = Table.ExpandTableColumn(Merged, \"M\", {\"Manager\"}) in Expanded";
    s.execute("powerquery.newQuery", json!({"name": "Merged", "source": {"kind": "m", "m": m}})).unwrap();
    assert_eq!(column(&preview(&mut s, "Merged"), "Manager"), vec![json!("Kim"), json!("Raj"), json!("Kim"), Json::Null]);
}

#[test]
fn files_examples_and_the_data_model() {
    let mut s = book();
    let dir = std::env::temp_dir().join(format!("gc-pq-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("orders.csv");
    std::fs::write(&csv, "Order,Customer,Amount,Date\n1,\"Smith, J\",100.5,2024-01-05\n2,Lee,20,2024-02-11\n").unwrap();
    let r = s.execute("powerquery.newQuery", json!({"source": {"kind": "csv", "path": csv.to_string_lossy()}})).unwrap();
    assert_eq!(r["name"], "orders");
    let p = preview(&mut s, "orders");
    assert_eq!(column(&p, "Customer"), vec![json!("Smith, J"), json!("Lee")]);
    assert_eq!(column(&p, "Amount"), vec![json!(100.5), json!(20.0)]);
    assert_eq!(p["types"], json!(["Whole Number", "Text", "Decimal Number", "Date"]));
    // Folder sources list the files.
    s.execute("powerquery.newQuery", json!({"name": "Files", "source": {"kind": "folder", "path": dir.to_string_lossy()}})).unwrap();
    assert_eq!(column(&preview(&mut s, "Files"), "Name"), vec![json!("orders.csv")]);
    // Column From Examples.
    let r = s.execute("powerquery.suggestColumn", json!({"query": "Sales", "examples": [[0, "John"], [1, "Mary"]], "columns": ["Name"]}));
    assert!(r.is_err(), "no query named Sales yet");
    s.execute("powerquery.newQuery", json!({"source": {"kind": "table", "table": "Sales"}})).unwrap();
    let r = s.execute("powerquery.suggestColumn", json!({"query": "Sales", "examples": [[0, "John"], [1, "Mary"]], "columns": ["Name"]})).unwrap();
    assert_eq!(r["suggestions"][0]["expr"], "Text.BeforeDelimiter([Name], \" \", 0)");
    // Load to the data model only (no worksheet table), and measure it with DAX.
    s.execute("powerquery.load", json!({"query": "orders", "to": "model"})).unwrap();
    let v = s.execute("powerpivot.evaluate", json!({"expression": "SUM(orders[Amount])"})).unwrap();
    assert_eq!(v["value"], json!(120.5));
    // Queries survive xlsx.
    let bytes = crate::io::save_bytes(&s.doc().unwrap().wb, "book.xlsx").unwrap();
    let (wb, _) = crate::io::open_bytes("book.xlsx", &bytes).unwrap();
    let doc = super::document(&wb);
    assert_eq!(doc.queries.len(), 3);
    assert_eq!(super::source_data(&wb, "orders").unwrap().rows.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_powerquery_command_survives_odd_params() {
    let mut s = book();
    s.execute("powerquery.newQuery", json!({"source": {"kind": "table", "table": "Sales"}})).unwrap();
    for spec in crate::command_specs().iter().filter(|c| c.id.starts_with("powerquery.")) {
        for p in [
            json!({}),
            json!([1]),
            json!({"query": "Sales", "step": "Nope", "name": 5}),
            json!({"source": {"kind": "csv", "path": "/no/such/file"}}),
            json!({"query": "Sales", "name": "X", "expr": "Table.SelectRows({prev}, each"}),
            json!({"m": "let a = a in a"}),
        ] {
            let _ = s.execute(spec.id, p);
        }
    }
    // A query that refers to itself is an error, not a hang.
    s.execute("powerquery.newQuery", json!({"name": "Loop", "source": {"kind": "m", "m": "Loop"}})).unwrap();
    assert!(s.execute("powerquery.preview", json!({"query": "Loop"})).is_err());
}
