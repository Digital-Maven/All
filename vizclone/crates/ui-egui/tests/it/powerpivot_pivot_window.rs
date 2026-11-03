//! The PivotTable from Data Model window keeps its size: its panes are sized from the screen, not
//! from the space they are given, which would feed the window's width back into itself and widen
//! it every frame.

use egui::{Role, vec2};
use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_ui_egui::SheetApp;
use gridcraft_ui_egui::powerpivot::Form;
use serde_json::json;

type Harness = egui_kittest::Harness<'static, SheetApp>;

fn harness() -> Harness {
    let mut session = Session::new();
    session.new_workbook();
    let rows = json!([["Region", "Amount"], ["East", 5], ["West", 7], ["East", 3]]);
    session.execute("range.setValues", json!({"range": "A1", "values": rows})).unwrap();
    session.execute("insert.table", json!({"range": "A1:B4", "name": "Sales"})).unwrap();
    session.execute("powerpivot.addAllTables", json!({})).unwrap();
    let mut h = crate::common::builder().with_size(vec2(1440.0, 900.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(ui.ctx());
            app.ui(ui);
        },
        crate::common::app(session),
    );
    h.run_steps(2);
    h
}

/// The window's right edge, read from its Cancel button (bottom right corner).
fn right_edge(h: &Harness) -> f32 {
    assert!(h.state().powerpivot.form.is_some(), "the PivotTable window is open");
    h.get_by_role_and_label(Role::Button, "Cancel").rect().right()
}

#[test]
fn the_pivot_window_does_not_widen_itself() {
    for fields in [None, Some((vec!["Sales[Region]".to_string()], vec!["Total := SUM(Sales[Amount])".to_string()]))] {
        let mut h = harness();
        h.state_mut().powerpivot.form = Some(match &fields {
            None => Form::pivot(),
            Some((rows, values)) => Form::pivot_with(rows.clone(), vec![], values.clone()),
        });
        h.run_steps(8);
        let first = right_edge(&h);
        h.run_steps(120);
        let later = right_edge(&h);
        assert_eq!(first, later, "the window must keep its width ({fields:?})");
        assert!(later < 1440.0, "the window fits the screen, its right edge is at {later}");
    }
}
