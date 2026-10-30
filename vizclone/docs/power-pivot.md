# Power Pivot: the native data model engine

VizClone adds GridCraft's missing piece next to Excel: **Power Pivot**, a data model that loads
worksheet tables into an in-memory columnar engine, relates them, and summarises them with
**DAX** measures. It is written from scratch in pure Rust (`crates/datamodel`, no dependencies on
other GridCraft crates) and wired into the spreadsheet as `powerpivot.*` commands, so the CLI,
the control channel and MCP can drive it.

## Architecture

| Piece | File | What it does |
|---|---|---|
| Columnar store | `crates/datamodel/src/store.rs` | Each column is dictionary-encoded: distinct values once, plus a `u32` id per row. Row sets are bitmaps. |
| Relationships | `store.rs`, `model.rs` | Many-to-one, single or both-direction cross-filtering, active or inactive. The lookup (one) side is detected from the data. |
| Filter context | `crates/datamodel/src/filter.rs` | Column-value and row filters; each table's visible rows come from its own filters and the filters that reach it through relationships. |
| DAX | `crates/datamodel/src/dax/` | Lexer and parser for expressions, `VAR … RETURN`, table constructors and `DEFINE MEASURE … EVALUATE … ORDER BY` queries. |
| Evaluator | `crates/datamodel/src/eval.rs` | Row context, context transition, CALCULATE filter arguments and modifiers, iterators, table functions, time intelligence. |
| PivotTable layout | `crates/datamodel/src/pivot.rs` | Row and column fields from any related tables, measures as values, grand totals. |
| Saved definition | `crates/datamodel/src/def.rs` | Tables, relationships, measures, calculated columns and reports, kept in the workbook (inside the `.xlsx`) and rebuilt from the tables' current data on demand. |
| Commands | `crates/engine/src/cmd/powerpivot.rs` | The `powerpivot.*` commands below. Every change is one undo step. |

## Commands

| Command | Params |
|---|---|
| `powerpivot.addTable` | `{table?}`: adds a worksheet table (default: the table at the active cell) |
| `powerpivot.removeTable` | `{table}`: also drops its relationships, measures and calculated columns |
| `powerpivot.createRelationship` | `{from: "Sales[ProductId]", to: "Products[ProductId]", crossFilter?: "single"\|"both", active?}` |
| `powerpivot.deleteRelationship` | `{from, to}` |
| `powerpivot.newMeasure` | `{table, name, expression, format?}`: returns the measure's grand-total value |
| `powerpivot.deleteMeasure` | `{name}` |
| `powerpivot.addColumn` / `powerpivot.deleteColumn` | `{table, name, expression}`: calculated columns |
| `powerpivot.insertPivot` | `{rows?, columns?, values, filters?, at?, overwrite?}`: writes a data-model PivotTable (new sheet by default) |
| `powerpivot.refresh` | `{report?}`: reloads the tables and rewrites the reports |
| `powerpivot.deletePivot` | `{report}` |
| `powerpivot.model` | Describes tables (rows, column types, cardinality, encoded size), relationships, measures and reports |
| `powerpivot.evaluate` | `{expression, filters?}`: one DAX value |
| `powerpivot.query` | `{query: "EVALUATE …"}`: a DAX query's result table |

Example, headless:

```bash
gridcraft-cli run --in book.xlsx \
  --cmd 'powerpivot.addTable={"table":"Sales"}' \
  --cmd 'powerpivot.addTable={"table":"Products"}' \
  --cmd 'powerpivot.createRelationship={"from":"Sales[ProductId]","to":"Products[ProductId]"}' \
  --cmd 'powerpivot.newMeasure={"table":"Sales","name":"Total Sales","expression":"SUM(Sales[Amount])"}' \
  --cmd 'powerpivot.insertPivot={"rows":["Products[Category]"],"values":["Total Sales"]}' \
  --out book.xlsx
```

## DAX supported

- **Aggregations:** SUM, AVERAGE, MIN, MAX, COUNT, COUNTA, COUNTBLANK, COUNTROWS, DISTINCTCOUNT,
  DISTINCTCOUNTNOBLANK, PRODUCT, MEDIAN; iterators SUMX, AVERAGEX, MINX, MAXX, COUNTX, COUNTAX,
  PRODUCTX, MEDIANX, CONCATENATEX, RANKX.
- **Filter:** CALCULATE, CALCULATETABLE, FILTER, ALL, REMOVEFILTERS, ALLEXCEPT, ALLNOBLANKROW,
  KEEPFILTERS, USERELATIONSHIP, VALUES, DISTINCT, RELATED, RELATEDTABLE, LOOKUPVALUE, HASONEVALUE,
  SELECTEDVALUE, ISFILTERED, ISCROSSFILTERED, ISEMPTY.
- **Tables:** SUMMARIZECOLUMNS, SUMMARIZE, ADDCOLUMNS, SELECTCOLUMNS, TOPN, GENERATESERIES,
  `{…}` constructors, `IN`.
- **Time intelligence:** TOTALYTD, TOTALQTD, TOTALMTD, DATESYTD, DATESQTD, DATESMTD,
  SAMEPERIODLASTYEAR, DATEADD, PREVIOUSYEAR, PREVIOUSQUARTER, PREVIOUSMONTH, DATESBETWEEN.
- **Logic, math, text, dates:** IF, SWITCH, IFERROR, COALESCE, AND, OR, NOT, BLANK, ISBLANK,
  DIVIDE, ABS, ROUND, ROUNDUP, ROUNDDOWN, INT, TRUNC, MOD, SQRT, POWER, EXP, LN, LOG10, SIGN,
  CONCATENATE, LEFT, RIGHT, MID, LEN, UPPER, LOWER, TRIM, SUBSTITUTE, REPT, CONTAINSSTRING,
  FORMAT (common number patterns), DATE, YEAR, MONTH, DAY, QUARTER, WEEKDAY, EOMONTH.
- **Language:** `VAR … RETURN`, `&&`, `||`, `&`, `==`, comments, quoted table names.

## Known gaps

- Context transition on a table without a unique key filters the single current row, not every
  identical row. Table filters don't use DAX's expanded-table semantics (a `FILTER(Sales, …)`
  filter doesn't override filters on lookup tables).
- ALLSELECTED behaves like ALL. No many-to-many relationships, hierarchies, KPIs, perspectives or
  Power Query sources; tables come from worksheet tables.
- The store is rebuilt from the worksheet tables on each call (no cache yet), and filter
  propagation is recomputed per filter context, so very large models (millions of rows with many
  pivot cells) are slower than a dedicated engine.
- No ribbon tab or Diagram View yet: the feature is reached through commands (command search,
  CLI, control channel, MCP). Reports are static values that `powerpivot.refresh` rewrites.
