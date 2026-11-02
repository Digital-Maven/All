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

## Using it in the app

**Power Pivot tab** (main ribbon): Manage, Add to Data Model, Add All Tables, Refresh All,
New Measure, Manage Measures, Calculated Column, Relationships, Create Relationship, Detect,
Diagram View, PivotTable and Date Table.

**Power Pivot window** (Manage; replaces the grid until Back to Workbook or Esc):

- An icon ribbon with captioned groups. *Home*: Get Data (Add All Tables, Refresh), Reports
  (PivotTable), Calculations (New Measure, Manage Measures, Calculated Column, Calculation Area),
  View (Data View / Diagram View). *Design*: Relationships (Create, Manage, Detect), Calendars
  (Date Table), Edit (Remove Table, Delete Selected). *View*: the views, Calculation Area, and
  the diagram's Fit to Screen / Reset Layout.
- **Data View**: one tab per model table; the rows in a grid with a sticky header showing each
  column's type and distinct count (calculated columns tinted); right-click a header to sort;
  `+` adds a calculated column. The **formula bar** shows the selected measure (`Name := formula`)
  or calculated column (`= formula`) and edits it in place; with nothing selected, typing
  `Name := formula` and Enter adds a measure. The **calculation area** lists the table's measures
  with their values (double-click to edit, right-click to delete).
- **Diagram View**: tables as cards (table icon and name, a row per column with its icon, `fx`
  for calculated columns) in a star layout (data tables in the middle, lookup tables around
  them). Cards with many columns scroll on their own (wheel or the card's scroll bar).
  Relationships are right-angled lines between the key columns with `*`/`1` ends and a small
  square marker whose arrow shows the filter direction (two arrows for both directions, dashed
  when inactive). The canvas has horizontal and vertical **scroll bars** (also the wheel or a
  background drag), **zoom** (−/+, 100%, Ctrl/Cmd + wheel, 30–200%) and **Fit to Screen**, so
  you can always get back to every table. Drag a column onto a column of another table to relate
  them; click a line to select it (Make Active/Inactive, Filter direction, Delete); drag titles to
  move cards; double-click a title to open the table.

**Forms**: New/Edit Measure (table, name, formula with Insert function / Insert field helpers,
Check Formula, number format), Add/Edit Calculated Column (with a preview of the first values),
Create/Edit Relationship, Manage Relationships, Manage Measures, New Date Table, and
**PivotTable from Data Model**, laid out like a PivotTable Fields pane: the **live report** on
the left; on the right a searchable field list (a checkbox per measure and column, grouped by
table) over the four areas **Filters, Columns, Rows, Values**. Ticking a field puts numbers and
measures in Values (an implicit Sum) and other columns in Rows; unticking removes it. Fields drag
from the list into any area, and the chips drag between areas and reorder within one (a line
shows where the drop lands). Right-click a field for *Add to …*; click a chip for Move Up / Down /
to Beginning / to End, Move to another area, Remove Field, *Summarize Values By* (Sum, Average,
Count, Distinct Count, Min, Max) and, in Filters, *Filter Values*. Measures stay in Values. The
report is inserted on a new or existing sheet. Inserted reports get bold headers and totals, the measures'
number formats and fitted column widths.

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
| `powerpivot.addAllTables` | `{}`: adds every workbook table not yet in the model |
| `powerpivot.editMeasure` | `{name, newName?, expression?, table?, format?}`: reports follow a rename |
| `powerpivot.editColumn` | `{table, name, expression?, newName?}` |
| `powerpivot.editRelationship` | `{from, to, active?, crossFilter?}`: activating one turns off others between the same tables |
| `powerpivot.detectRelationships` | `{}`: relates same-named columns, unique on one side, whose values match |
| `powerpivot.newDateTable` | `{column: "Sales[OrderDate]", name?}`: whole-year calendar sheet + table, added and related (one undo step) |
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
- Reports are static values that `powerpivot.refresh` rewrites (not an interactive PivotTable
  with slicers, drill-down or subtotal layouts yet). No KPIs, hierarchies, perspectives, Sort by
  Column or hidden columns. Diagram positions aren't saved with the workbook.
