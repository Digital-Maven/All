use gridcraft_core::{CellRef, Value};
use serde_json::{Value as Json, json};

use crate::Session;

/// Three worksheet tables: Sales (Sheet1), Products and Customers (Lookups).
fn book() -> Session {
    let mut s = Session::new();
    s.new_workbook();
    let sales = json!([
        ["OrderId", "ProductId", "CustomerId", "Qty", "Amount"],
        [1, 1, 100, 2, 20],
        [2, 2, 101, 1, 50],
        [3, 3, 100, 5, 15],
        [4, 1, 102, 3, 30],
        [5, 2, 100, 2, 100],
        [6, 3, 101, 4, 12]
    ]);
    s.execute("range.setValues", json!({"range": "A1", "values": sales})).unwrap();
    s.execute("insert.table", json!({"range": "A1:E7", "name": "Sales"})).unwrap();
    s.execute("home.insertSheet", json!({})).unwrap();
    let lookups = s.doc().unwrap().wb.sheets.len() - 1;
    s.execute("sheet.rename", json!({"sheet": lookups, "name": "Lookups"})).unwrap();
    s.execute("sheet.activate", json!({"sheet": "Lookups"})).unwrap();
    let products = json!([["ProductId", "Product", "Category"], [1, "Pen", "Office"], [2, "Chair", "Furniture"], [3, "Paper", "Office"]]);
    s.execute("range.setValues", json!({"range": "A1", "values": products})).unwrap();
    s.execute("insert.table", json!({"range": "A1:C4", "name": "Products"})).unwrap();
    let customers = json!([["CustomerId", "Region"], [100, "East"], [101, "West"], [102, "East"]]);
    s.execute("range.setValues", json!({"range": "F1", "values": customers})).unwrap();
    s.execute("insert.table", json!({"range": "F1:G4", "name": "Customers"})).unwrap();
    s.execute("sheet.activate", json!({"sheet": 0})).unwrap();
    s
}

fn modelled() -> Session {
    let mut s = book();
    for t in ["Sales", "Products", "Customers"] {
        s.execute("powerpivot.addTable", json!({"table": t})).unwrap();
    }
    s.execute("powerpivot.createRelationship", json!({"from": "Sales[ProductId]", "to": "Products[ProductId]"})).unwrap();
    s.execute("powerpivot.createRelationship", json!({"from": "Sales[CustomerId]", "to": "Customers[CustomerId]"})).unwrap();
    s.execute("powerpivot.newMeasure", json!({"table": "Sales", "name": "Total Sales", "expression": "SUM(Sales[Amount])"})).unwrap();
    s
}

fn value_at(s: &Session, sheet: &str, a1: &str) -> Value {
    let wb = &s.doc().unwrap().wb;
    wb.sheet(wb.sheet_index(sheet).unwrap()).unwrap().value(CellRef::parse(a1).unwrap())
}

fn eval(s: &mut Session, e: &str, filters: Json) -> Json {
    s.execute("powerpivot.evaluate", json!({"expression": e, "filters": filters})).unwrap()["value"].clone()
}

#[test]
fn builds_a_model_from_worksheet_tables() {
    let mut s = modelled();
    let m = s.execute("powerpivot.model", json!({})).unwrap();
    assert_eq!(m["tables"].as_array().unwrap().len(), 3);
    assert_eq!(m["tables"][0]["rows"], 6);
    assert_eq!(m["relationships"].as_array().unwrap().len(), 2);
    assert_eq!(eval(&mut s, "[Total Sales]", json!([])), json!(227.0));
    assert_eq!(eval(&mut s, "[Total Sales]", json!(["Products[Category] = \"Office\""])), json!(77.0));
    assert_eq!(eval(&mut s, "[Total Sales]", json!(["Customers[Region] = \"East\"", "Products[Category] = \"Furniture\""])), json!(100.0));
    let q = s.execute("powerpivot.query", json!({"query": "EVALUATE SUMMARIZECOLUMNS(Customers[Region], \"S\", [Total Sales])"})).unwrap();
    assert_eq!(q["rows"], json!([["East", 165.0], ["West", 62.0]]));
}

#[test]
fn measures_and_columns_are_checked_before_they_are_stored() {
    let mut s = modelled();
    assert!(s.execute("powerpivot.newMeasure", json!({"table": "Sales", "name": "Bad", "expression": "SUM("})).is_err());
    assert!(s.execute("powerpivot.newMeasure", json!({"table": "Nope", "name": "X", "expression": "1"})).is_err());
    assert!(s.execute("powerpivot.addTable", json!({"table": "Sales"})).is_err());
    assert!(s.execute("powerpivot.addTable", json!({"table": "Missing"})).is_err());
    // Region is not unique, so it can't be the lookup side.
    assert!(s.execute("powerpivot.createRelationship", json!({"from": "Customers[Region]", "to": "Sales[Qty]"})).is_err());
    let r = s.execute("powerpivot.newMeasure", json!({"table": "Sales", "name": "Avg Qty", "expression": "AVERAGE(Sales[Qty])"})).unwrap();
    assert_eq!(r["value"], json!(17.0 / 6.0));
    s.execute("powerpivot.addColumn", json!({"table": "Sales", "name": "Category", "expression": "RELATED(Products[Category])"})).unwrap();
    assert_eq!(eval(&mut s, "CALCULATE([Total Sales], Sales[Category] = \"Office\")", json!([])), json!(77.0));
    s.execute("powerpivot.deleteColumn", json!({"table": "Sales", "name": "Category"})).unwrap();
    assert!(s.execute("powerpivot.evaluate", json!({"expression": "COUNTROWS(VALUES(Sales[Category]))"})).is_err());
}

#[test]
fn data_model_pivot_writes_refreshes_and_survives_xlsx() {
    let mut s = modelled();
    let r = s
        .execute("powerpivot.insertPivot", json!({"rows": ["Products[Category]"], "columns": ["Customers[Region]"], "values": ["Total Sales"]}))
        .unwrap();
    let sheet = r["sheet"].as_str().unwrap().to_string();
    assert_eq!(sheet, "Data Model Pivot");
    assert_eq!(value_at(&s, &sheet, "B1"), Value::text("East"));
    assert_eq!(value_at(&s, &sheet, "A3"), Value::text("Furniture"));
    assert_eq!(value_at(&s, &sheet, "B3"), Value::Number(100.0));
    assert_eq!(value_at(&s, &sheet, "C3"), Value::Number(50.0));
    assert_eq!(value_at(&s, &sheet, "D5"), Value::Number(227.0));
    // Source data changes; a refresh picks them up.
    s.execute("cell.set", json!({"cell": "Sheet1!E2", "input": "120"})).unwrap();
    s.execute("powerpivot.refresh", json!({})).unwrap();
    assert_eq!(value_at(&s, &sheet, "D5"), Value::Number(327.0));
    // Adding a measure rewrites reports that use the model too, and undo takes it back.
    s.execute("powerpivot.newMeasure", json!({"table": "Sales", "name": "Orders", "expression": "COUNTROWS(Sales)"})).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    let m = s.execute("powerpivot.model", json!({})).unwrap();
    assert_eq!(m["measures"].as_array().unwrap().len(), 1);
    // The model and its reports travel inside the xlsx file.
    let bytes = crate::io::save_bytes(&s.doc().unwrap().wb, "book.xlsx").unwrap();
    let (wb, _) = crate::io::open_bytes("book.xlsx", &bytes).unwrap();
    let model = super::model_of(&wb).unwrap();
    assert_eq!(model.tables.len(), 3);
    assert_eq!(model.evaluate("[Total Sales]").unwrap(), gridcraft_datamodel::Value::Number(327.0));
    assert_eq!(super::definition(&wb).reports.len(), 1);
    // A report at a chosen cell refuses to overwrite data unless asked.
    assert!(s.execute("powerpivot.insertPivot", json!({"rows": ["Customers[Region]"], "values": ["Total Sales"], "at": "Sheet1!A1"})).is_err());
    let r = s.execute("powerpivot.insertPivot", json!({"rows": ["Customers[Region]"], "values": ["Total Sales"], "at": "Sheet1!H1"})).unwrap();
    assert_eq!(value_at(&s, "Sheet1", "I4"), Value::Number(327.0));
    s.execute("powerpivot.deletePivot", json!({"report": r["report"]})).unwrap();
    assert_eq!(value_at(&s, "Sheet1", "I4"), Value::Empty);
}

#[test]
fn removing_a_table_drops_what_depends_on_it() {
    let mut s = modelled();
    s.execute("powerpivot.removeTable", json!({"table": "Products"})).unwrap();
    let m = s.execute("powerpivot.model", json!({})).unwrap();
    assert_eq!(m["tables"].as_array().unwrap().len(), 2);
    assert_eq!(m["relationships"].as_array().unwrap().len(), 1);
    // Deleting the worksheet table under the model is reported, not a crash.
    s.execute("sheet.activate", json!({"sheet": "Lookups"})).unwrap();
    s.execute("home.deleteSheet", json!({})).unwrap();
    assert!(s.execute("powerpivot.model", json!({})).is_err());
}

#[test]
fn every_powerpivot_command_survives_odd_params() {
    let mut s = modelled();
    for spec in crate::command_specs().iter().filter(|c| c.id.starts_with("powerpivot.")) {
        for p in [
            json!({}),
            json!([1]),
            json!({"table": 5, "from": "x", "to": "]["}),
            json!({"report": -1, "at": "ZZZZ99999999"}),
            json!({"rows": ["A[B]"], "values": [1]}),
        ] {
            let _ = s.execute(spec.id, p);
        }
    }
}
