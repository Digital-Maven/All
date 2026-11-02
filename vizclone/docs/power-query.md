# Power Query: the M engine and the Power Query Editor

VizClone adds **Power Query** to GridCraft: queries written in **M** that get data, transform it
step by step, and load it into a worksheet table or straight into the **data model** (the
Power BI way), all native Rust with no outside engine.

## Pieces

| Piece | Where | What it does |
|---|---|---|
| M language | `crates/query/src/m/` | Lexer, parser (spans kept per step) and lazy evaluator: `let`, `each`, functions, records, lists, `try … otherwise`, `error`, `??`, `meta`, `#date`/`#table`/… intrinsics, generalized field names (`[Order Date]`), item and field access, per-cell error values. |
| Function library | `crates/query/src/funcs/` | ~300 functions: `Table.*` (80+: filter, sort, group, pivot/unpivot, merge with all six join kinds, append, split, combine, expand, fill, replace, transpose, index/rank, profile…), `Text.*`, `List.*`, `Number.*`, `Date/DateTime/Time/Duration.*`, `Record.*`, `Value.*`, `Splitter.*`, `Combiner.*`, `Replacer.*`, sources (`Excel.CurrentWorkbook`, `Excel.Workbook`, `Csv.Document`, `Json.Document`, `File.Contents`, `Folder.Files`, `Lines.*`), type conversion with en-US parsing. |
| Applied steps | `crates/query/src/steps.rs` | A query is M; its `let` bindings are the steps. Add (anywhere), delete (rewiring the next step), delete until end, rename (references follow), edit, move. |
| Column From Examples | `crates/query/src/examples.rs` | Finds an M formula reproducing typed examples: case changes, trimming, text before/after/between delimiters, first/last/middle characters, lengths, digits, arithmetic, rounding, date parts and formats, combinations of pieces joined with literals, and value mappings. Every candidate is run by the M evaluator. |
| Profiling | `crates/query/src/profile.rs` | Column quality (valid/error/empty), distribution (distinct/unique/top values), statistics. |
| Commands | `crates/engine/src/cmd/powerquery.rs` | `powerquery.*` (below). Queries and parameters are saved in the workbook (xlsx and JSON) and undo with the edit that changed them. |
| Editor | `crates/ui-egui/src/powerquery/` | The Power Query Editor window. |

## The editor

Open it from the **Power Query** ribbon tab (Get Data, From Table/Range, From Text/CSV, From
Folder, Launch Power Query Editor, Refresh All, Manage Parameters, Query Dependencies, Queries &
Connections).

- **Queries pane** (left): queries by group, parameters; right-click to rename, duplicate,
  reference, delete, enable/disable load, load to the data model, open the Advanced Editor or
  properties.
- **Formula bar**: the selected step's M, editable; `fx` adds a step.
- **Preview**: column type badges (click to change type), sort/filter menus with value
  checklists and conditions, column quality and distribution under each header, click-through
  into nested tables/records/lists, multi-column selection, right-click column menu.
- **Query Settings** (right): name, properties and the **Applied Steps** (click to preview a step,
  ✕ to delete, double-click to rename, right-click for Delete Until End, Insert Step After, Move
  Before/After).
- **Home**: New Source, Enter Data, Refresh Preview, Refresh All, Advanced Editor, Manage
  (Duplicate/Reference/Delete), Choose Columns, Remove Columns / Other Columns, Keep Rows (top,
  bottom, range, duplicates, errors), Remove Rows (top, bottom, alternate, duplicates, blank,
  errors), Sort, Split Column, Group By, Data Type, Use First Row as Headers, Replace Values,
  Merge Queries (and as New), Append Queries (and as New), Combine Files, Manage Parameters,
  Close & Load, Close & Load To (table, data model, connection only).
- **Transform**: Group By, headers, Transpose, Reverse Rows, Count Rows, Data Type, Detect Data
  Type, Rename, Replace Values/Errors, Fill Down/Up, Pivot Column, Unpivot (selected, other,
  only selected), Move, Convert to List, Split Column (delimiter left/right/each, characters,
  positions, lower→upper, upper→lower, digit↔non-digit), Format, Merge Columns, Extract, Parse
  JSON, Statistics, Standard, Scientific, Rounding, Information, Date, Time, Duration, Expand.
- **Add Column**: **Column From Examples** (from all columns or the selection; live formula and
  predicted values, alternative formulas), Custom Column (formula check, column list), Conditional
  Column (if/else-if/else rules), Index Column, Duplicate Column, and the Format/Extract/Merge/
  Number/Date/Time/Duration families as new columns.
- **View**: Formula Bar, Monospaced, Show Whitespace, Column Quality, Column Distribution, Column
  Profile (statistics and value distribution of the selected column), Profile Entire Data Set,
  Query Dependencies, Advanced Editor, Go to Column.

## Commands

`powerquery.newQuery {name?, source}` (`table`, `csv`, `excel` + `item`, `json`, `folder`,
`text`, `web`, `enter`, `blank`, `m`; detects column types), `setQuery {name, m?, newName?,
load?, description?, group?}`, `deleteQuery`, `duplicateQuery`, `referenceQuery`,
`addStep {query, name, expr with {prev}, after?}`, `deleteStep {query, step, untilEnd?}`,
`renameStep`, `editStep`, `moveStep`, `load {query?, to?: table|model|none}`,
`refresh {query?}`, `setParameter {name, value, kind?, description?, newName?}`,
`deleteParameter`, and the read-only `list`, `preview {query, step?, rows?}`,
`evaluate {m}`, `suggestColumn {query, columns?, examples}`.

## Known gaps

- `Web.Contents` (the app can't download yet), databases, SharePoint and other online sources.
- Query folding, privacy levels, incremental refresh, Power BI dataflows.
- Culture-specific parsing (en-US only), `Table.Buffer` and laziness tuning for very large files.
- Steps can't be edited through their original dialogs (the gear icon); edit them in the formula
  bar or the Advanced Editor.
