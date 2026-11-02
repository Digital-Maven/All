//! Renders the whole GridCraft window offscreen (wgpu, no window) to a PNG.
//!
//! `cargo run --release -p gridcraft-ui-egui --example snapshot -- [--sample sales] [--in file.xlsx]
//!  [--size 1440x900] [--scale 2] [--cmd 'id={json}']... [--tab Insert] [--dark] [--language en|es|zh|ja|ko|ru|pt|de|cs|fr|tr]
//!  [--dialog 'name={json}'] [--hf-edit header|footer] [--powerpivot data|diagram|measure|relationships|pivot|ribbon] out.png`

use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut sample = None;
    let mut input = None;
    let mut size = (1440.0f32, 900.0f32);
    let mut scale = 2.0f32;
    let mut cmds: Vec<String> = vec![];
    let mut tab = None;
    let mut dark = false;
    let mut pane: Option<String> = None;
    let mut chart: Option<u32> = None;
    let mut dialog: Option<String> = None;
    let mut language: Option<String> = None;
    let mut hf_edit: Option<String> = None;
    let mut powerpivot: Option<String> = None;
    let mut powerquery: Option<String> = None;
    let mut out = "snapshot.png".to_string();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--sample" => {
                i += 1;
                sample = args.get(i).cloned();
            }
            "--in" => {
                i += 1;
                input = args.get(i).cloned();
            }
            "--size" => {
                i += 1;
                if let Some((w, h)) = args.get(i).and_then(|s| s.split_once('x')) {
                    size = (w.parse().unwrap_or(1440.0), h.parse().unwrap_or(900.0));
                }
            }
            "--scale" => {
                i += 1;
                scale = args.get(i).and_then(|s| s.parse().ok()).unwrap_or(2.0);
            }
            "--cmd" => {
                i += 1;
                if let Some(c) = args.get(i) {
                    cmds.push(c.clone());
                }
            }
            "--tab" => {
                i += 1;
                tab = args.get(i).cloned();
            }
            "--dark" => dark = true,
            "--pane" => {
                i += 1;
                pane = args.get(i).cloned();
            }
            "--chart" => {
                i += 1;
                chart = args.get(i).and_then(|s| s.parse().ok());
            }
            "--dialog" => {
                i += 1;
                dialog = args.get(i).cloned();
            }
            "--hf-edit" => {
                i += 1;
                hf_edit = args.get(i).cloned();
            }
            "--powerquery" => {
                i += 1;
                powerquery = args.get(i).cloned();
            }
            "--powerpivot" => {
                i += 1;
                powerpivot = args.get(i).cloned();
            }
            "--language" => {
                i += 1;
                language = args.get(i).cloned();
            }
            other => out = other.to_string(),
        }
        i += 1;
    }
    // Catalog the installed fonts first (the app scans in the background from its first frame), so
    // the workbook's fonts are drawn in their own faces, not a substitute.
    let _ = gridcraft_ui_egui::fonts::catalog();
    let mut session = Session::new();
    if let Some(p) = &input {
        session.execute("file.open", json!({"path": p})).expect("open");
    } else if let Some(s) = &sample {
        session.execute("file.new", json!({"sample": s})).expect("sample");
    } else {
        session.new_workbook();
    }
    for c in &cmds {
        let (id, p) = c.split_once('=').map(|(a, b)| (a.to_string(), serde_json::from_str(b).unwrap_or(json!({})))).unwrap_or((c.clone(), json!({})));
        if let Err(e) = session.execute(&id, p) {
            eprintln!("{id}: {e}");
        }
    }
    let mut harness =
        egui_kittest::Harness::builder().with_size(egui::vec2(size.0, size.1)).with_pixels_per_point(scale).with_max_steps(12).wgpu().build_ui_state(
            move |ui, app: &mut SheetApp| {
                let ctx = ui.ctx().clone();
                app.logic(&ctx);
                app.ui(ui);
            },
            {
                let mut app = SheetApp::new(session, Default::default());
                app.ui.dark = dark;
                if let Some(code) = &language
                    && let Some(l) = gridcraft_ui_egui::i18n::Language::parse(code)
                {
                    app.ui.language = l;
                }
                if let Some(t) = tab {
                    app.ui.ribbon_tab = t;
                }
                app.grid.pane = pane;
                app.selected_chart = chart;
                if let Some(d) = &dialog {
                    let (name, p) =
                        d.split_once('=').map(|(a, b)| (a, serde_json::from_str(b).unwrap_or(json!({})))).unwrap_or((d.as_str(), json!({})));
                    app.open_dialog(name, p);
                }
                // Power Pivot: `data`, `diagram`, `measure`, `relationships`, `pivot` or `ribbon`.
                if let Some(mode) = &powerpivot {
                    use gridcraft_ui_egui::powerpivot::{self, Form, View};
                    let list = |s: &str| s.split(',').filter(|x| !x.is_empty()).map(str::to_string).collect::<Vec<_>>();
                    match mode.as_str() {
                        "ribbon" => app.ui.ribbon_tab = "Power Pivot".into(),
                        // `pivot:rows;columns;measures`, each comma-separated.
                        m if m.starts_with("pivot:") => {
                            let parts: Vec<&str> = m.trim_start_matches("pivot:").split(';').collect();
                            let get = |i: usize| list(parts.get(i).copied().unwrap_or_default());
                            app.powerpivot.form = Some(Form::pivot_with(get(0), get(1), get(2)));
                        }
                        "pivot" => app.powerpivot.form = Some(Form::pivot()),
                        "relationships" => app.powerpivot.form = Some(Form::ManageRelationships),
                        _ => {
                            powerpivot::open(&mut app);
                            app.powerpivot.calc_area = true;
                            if mode == "diagram" {
                                app.powerpivot.view = View::Diagram;
                            }
                            if mode == "measure" {
                                let table = app.powerpivot.table.clone();
                                app.powerpivot.form = Some(Form::new_measure(&table));
                            }
                        }
                    }
                }
                // Power Query: `editor`, `examples`, `profile`, `deps`, `ribbon`, or a dialog
                // (`getdata`, `groupby`, `merge`, `advanced`, `custom`, `conditional`).
                if let Some(mode) = &powerquery {
                    use gridcraft_ui_egui::powerquery::{self, Dialog};
                    if mode == "ribbon" {
                        app.ui.ribbon_tab = "Power Query".into();
                    } else {
                        powerquery::open(&mut app, Some("Sales"));
                        let tab = match mode.as_str() {
                            "transform" => 1,
                            "add" | "examples" => 2,
                            "view" | "profile" => 3,
                            _ => 0,
                        };
                        app.powerquery.tab = tab;
                        match mode.as_str() {
                            "examples" => {
                                let mut ex =
                                    powerquery::Examples { name: "First Name".into(), inputs: vec!["Customer".into()], ..Default::default() };
                                ex.values.insert(0, "John".into());
                                ex.values.insert(1, "Mary".into());
                                app.powerquery.examples = Some(ex);
                            }
                            "profile" => {
                                app.powerquery.view.profile = true;
                                app.powerquery.selected = vec!["Amount".into()];
                            }
                            "deps" => app.powerquery.deps = true,
                            "getdata" => app.powerquery.dialog = Some(Dialog::get_data("csv")),
                            "groupby" => {
                                app.powerquery.selected = vec!["Region".into()];
                                app.powerquery.dialog = Some(Dialog::group_by(&mut app));
                            }
                            "merge" => app.powerquery.dialog = Some(Dialog::merge(&mut app, false)),
                            "advanced" => app.powerquery.dialog = Some(Dialog::advanced(&mut app)),
                            "custom" => app.powerquery.dialog = Some(Dialog::custom_column(&mut app)),
                            "conditional" => app.powerquery.dialog = Some(Dialog::conditional(&mut app)),
                            _ => {}
                        }
                    }
                }
                // Page Layout view: edit the first page's centre header (or footer) section.
                let active = app.session.active().and_then(|d| d.wb.active().cloned().map(|s| (d.wb.active_sheet(), s)));
                if let (Some(part), Some((si, sh))) = (&hf_edit, active) {
                    let zone = gridcraft_engine::paper::HfZone { footer: part == "footer", section: 1 };
                    gridcraft_ui_egui::paper_view::begin(&mut app, si, &sh, (0, 0), zone);
                }
                app
            },
        );
    // The workbook's installed fonts bind once the font scan is done; frames past the first few
    // apply them, so the snapshot shows them as the app does.
    harness.run_steps(14);
    let img = harness.render().expect("render");
    img.save(&out).expect("save png");
    eprintln!("wrote {out} ({}x{})", img.width(), img.height());
}
