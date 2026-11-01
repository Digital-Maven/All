//! Diagram View: each model table as a box listing its columns, with its relationships drawn
//! between the key columns. Drag a box's title to move it, drag the background to pan, drag a
//! column onto a column of another table to relate them, click a line to select its
//! relationship (Delete removes it), double-click a title to open the table in Data View.

use std::collections::HashMap;

use egui::{Align2, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use gridcraft_engine::datamodel::{CrossFilter, Model};
use serde_json::json;

use super::{Form, View, run, toast};
use crate::SheetApp;
use crate::theme::{self, Tokens};

const BOX_W: f32 = 210.0;
const TITLE_H: f32 = 28.0;
const COL_H: f32 = 19.0;
const MAX_COLS: usize = 40;

#[derive(Default)]
pub struct DiagramState {
    /// Table name → top-left of its box, in canvas coordinates.
    pub pos: HashMap<String, Pos2>,
    pub pan: Vec2,
    /// The column a relationship drag started from.
    drag_from: Option<(String, String)>,
    /// The selected relationship, as (from table, from column, to table, to column).
    pub selected: Option<(String, String, String, String)>,
}

fn box_height(n_cols: usize) -> f32 {
    TITLE_H + n_cols.min(MAX_COLS) as f32 * COL_H + if n_cols > MAX_COLS { COL_H } else { 0.0 } + 6.0
}

/// Places tables that have no position yet. With nothing placed, a star: data (fact) tables in
/// the middle column, the lookup tables they use alternately on its left and right, unrelated
/// tables in a row underneath. Tables added later go in a row below everything.
fn layout(st: &mut DiagramState, m: &Model) {
    let missing: Vec<usize> = (0..m.tables.len()).filter(|i| m.tables.get(*i).is_some_and(|t| !st.pos.contains_key(&t.name))).collect();
    if missing.is_empty() {
        return;
    }
    let height = |i: usize| m.tables.get(i).map(|t| box_height(t.columns.len())).unwrap_or(100.0);
    let place = |st: &mut DiagramState, i: usize, p: Pos2| {
        if let Some(t) = m.tables.get(i) {
            st.pos.insert(t.name.clone(), p);
        }
    };
    if !st.pos.is_empty() {
        let below = m.tables.iter().filter_map(|t| st.pos.get(&t.name).map(|p| p.y + box_height(t.columns.len()))).fold(0.0f32, f32::max) + 60.0;
        for (k, i) in missing.into_iter().enumerate() {
            place(st, i, pos2(30.0 + k as f32 * (BOX_W + 70.0), below));
        }
        return;
    }
    let is_lookup = |i: usize| m.relationships.iter().any(|r| r.to_table == i);
    let is_fact = |i: usize| m.relationships.iter().any(|r| r.from_table == i) && !is_lookup(i);
    let gap = 140.0;
    let (left_x, mid_x, right_x) = (30.0, 30.0 + BOX_W + gap, 30.0 + 2.0 * (BOX_W + gap));
    let (mut ly, mut my, mut ry) = (30.0f32, 30.0f32, 30.0f32);
    let mut side = false;
    let mut others = Vec::new();
    for i in missing {
        if is_fact(i) {
            place(st, i, pos2(mid_x, my));
            my += height(i) + 50.0;
        } else if is_lookup(i) {
            if side {
                place(st, i, pos2(right_x, ry));
                ry += height(i) + 40.0;
            } else {
                place(st, i, pos2(left_x, ly));
                ly += height(i) + 40.0;
            }
            side = !side;
        } else {
            others.push(i);
        }
    }
    let below = ly.max(my).max(ry) + 20.0;
    for (k, i) in others.into_iter().enumerate() {
        place(st, i, pos2(30.0 + (k % 4) as f32 * (BOX_W + 70.0), below + (k / 4) as f32 * 320.0));
    }
}

/// Screen rect of a column row (or of the title when the column isn't shown).
fn column_rect(boxes: &HashMap<usize, Rect>, m: &Model, t: usize, c: usize) -> Option<Rect> {
    let b = boxes.get(&t)?;
    let n = m.tables.get(t)?.columns.len();
    if c >= MAX_COLS.min(n) {
        return Some(Rect::from_min_size(b.min, vec2(BOX_W, TITLE_H)));
    }
    Some(Rect::from_min_size(pos2(b.min.x, b.min.y + TITLE_H + c as f32 * COL_H), vec2(BOX_W, COL_H)))
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 <= f32::EPSILON {
        return (p - a).length();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).length()
}

/// (from table, from column, to table, to column) of a relationship.
fn rel_names(m: &Model, r: &gridcraft_engine::datamodel::store::Relationship) -> (String, String, String, String) {
    let tn = |t: usize| m.tables.get(t).map(|x| x.name.clone()).unwrap_or_default();
    let cn = |t: usize, c: usize| m.tables.get(t).and_then(|x| x.column(c)).map(|c| c.name.clone()).unwrap_or_default();
    (tn(r.from_table), cn(r.from_table, r.from_column), tn(r.to_table), cn(r.to_table, r.to_column))
}

fn arrow(painter: &egui::Painter, at: Pos2, dir: Vec2, color: Color32) {
    let d = dir.normalized() * 7.0;
    let n = vec2(-d.y, d.x) * 0.7;
    painter.add(Shape::convex_polygon(vec![at + d, at - d + n, at - d - n], color, Stroke::NONE));
}

pub(super) fn show(app: &mut SheetApp, ui: &mut Ui, m: &Model) {
    let t = Tokens::get(ui.ctx());
    layout(&mut app.powerpivot.diagram, m);
    action_strip(app, ui, &t, m);
    let (canvas, bg) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(canvas);
    painter.rect_filled(canvas, 0.0, t.desk);
    // Dot grid.
    let pan = app.powerpivot.diagram.pan;
    let step = 24.0;
    let ox = (canvas.min.x + pan.x).rem_euclid(step);
    let oy = (canvas.min.y + pan.y).rem_euclid(step);
    let mut gx = canvas.min.x + ox;
    while gx < canvas.max.x {
        let mut gy = canvas.min.y + oy;
        while gy < canvas.max.y {
            painter.circle_filled(pos2(gx, gy), 0.8, t.gridline);
            gy += step;
        }
        gx += step;
    }
    let origin = canvas.min + pan;
    let boxes: HashMap<usize, Rect> = m
        .tables
        .iter()
        .enumerate()
        .filter_map(|(i, tb)| {
            app.powerpivot
                .diagram
                .pos
                .get(&tb.name)
                .map(|p| (i, Rect::from_min_size(origin + p.to_vec2(), vec2(BOX_W, box_height(tb.columns.len())))))
        })
        .collect();
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let hit_rel = draw_relationships(app, &painter, &t, m, &boxes, canvas, pointer);
    // Boxes.
    let mut move_req: Option<(String, Vec2)> = None;
    let mut open_table: Option<String> = None;
    let mut drop_target: Option<(String, String)> = None;
    let mut any_box_hover = false;
    for (i, tb) in m.tables.iter().enumerate() {
        let Some(b) = boxes.get(&i).copied() else { continue };
        if !canvas.intersects(b) {
            continue;
        }
        painter.rect_filled(b.translate(vec2(2.0, 3.0)), 6.0, t.shadow);
        painter.rect_filled(b, 6.0, t.window);
        let title = Rect::from_min_size(b.min, vec2(BOX_W, TITLE_H));
        painter.rect_filled(title, egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, t.accent);
        let rows = painter.text(
            title.right_center() - vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            format!("{} rows", tb.row_count()),
            theme::ui_font(10.0),
            t.on_accent,
        );
        let name_clip = Rect::from_min_max(title.min, pos2(rows.left() - 8.0, title.max.y)).intersect(canvas);
        painter.with_clip_rect(name_clip).text(
            title.left_center() + vec2(10.0, 0.0),
            Align2::LEFT_CENTER,
            &tb.name,
            theme::ui_bold(13.0),
            t.on_accent,
        );
        painter.rect_stroke(b, 6.0, Stroke::new(1.0, t.ribbon_border), StrokeKind::Inside);
        let head = ui.interact(title.intersect(canvas), ui.scope_id().with(("pp_box", i)), Sense::click_and_drag());
        if head.dragged() {
            move_req = Some((tb.name.clone(), head.drag_delta()));
        }
        if head.double_clicked() {
            open_table = Some(tb.name.clone());
        }
        any_box_hover |= head.hovered();
        for (c, col) in tb.columns.iter().enumerate().take(MAX_COLS) {
            let Some(rr) = column_rect(&boxes, m, i, c) else { continue };
            let resp = ui.interact(rr.intersect(canvas), ui.scope_id().with(("pp_col", i, c)), Sense::drag());
            any_box_hover |= resp.hovered();
            let dragging_this = app.powerpivot.diagram.drag_from.as_ref().is_some_and(|(dt, dc)| *dt == tb.name && *dc == col.name);
            if resp.hovered() || dragging_this {
                painter.rect_filled(rr.shrink(1.0), 3.0, t.hover);
            }
            let key = m.relationships.iter().any(|r| (r.from_table == i && r.from_column == c) || (r.to_table == i && r.to_column == c));
            let color = if col.expression.is_some() { t.accent } else { t.text };
            let font = if key { theme::ui_bold(12.0) } else { theme::ui_font(12.0) };
            painter.text(
                rr.left_center() + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                if col.expression.is_some() { format!("fx {}", col.name) } else { col.name.clone() },
                font,
                color,
            );
            if resp.drag_started() {
                app.powerpivot.diagram.drag_from = Some((tb.name.clone(), col.name.clone()));
            }
            if let Some(p) = pointer
                && rr.contains(p)
            {
                drop_target = Some((tb.name.clone(), col.name.clone()));
            }
        }
        if tb.columns.len() > MAX_COLS {
            painter.text(
                pos2(b.min.x + 10.0, b.min.y + TITLE_H + MAX_COLS as f32 * COL_H + COL_H / 2.0),
                Align2::LEFT_CENTER,
                format!("+{} more", tb.columns.len() - MAX_COLS),
                theme::ui_font(11.0),
                t.text_dim,
            );
        }
    }
    // A relationship drag in progress: a line from the source column to the pointer.
    if let Some((ft, fc)) = app.powerpivot.diagram.drag_from.clone() {
        if let (Some(fi), Some(p)) = (m.table_index(&ft), pointer) {
            let ci = m.table(&ft).and_then(|x| x.column_index(&fc)).unwrap_or(0);
            if let Some(rr) = column_rect(&boxes, m, fi, ci) {
                painter.line_segment([rr.right_center(), p], Stroke::new(2.0, t.accent));
            }
        }
        if ui.input(|i| i.pointer.any_released()) {
            app.powerpivot.diagram.drag_from = None;
            match drop_target {
                Some((tt, tc)) if !tt.eq_ignore_ascii_case(&ft) => {
                    let from = format!("{}[{}]", quote(&ft), fc);
                    let to = format!("{}[{}]", quote(&tt), tc);
                    if run(app, "powerpivot.createRelationship", json!({ "from": from, "to": to })).is_some() {
                        toast(app, format!("Related {ft}[{fc}] and {tt}[{tc}]"));
                    }
                }
                Some(_) => toast(app, "Drop the column on a column of another table"),
                None => {}
            }
        }
    }
    let moving = move_req.is_some();
    if let Some((name, d)) = move_req
        && let Some(p) = app.powerpivot.diagram.pos.get_mut(&name)
    {
        *p += d;
    }
    if bg.dragged() && app.powerpivot.diagram.drag_from.is_none() && !moving {
        app.powerpivot.diagram.pan += bg.drag_delta();
    }
    if bg.clicked() && !any_box_hover {
        app.powerpivot.diagram.selected = hit_rel.and_then(|ri| m.relationships.get(ri)).map(|r| rel_names(m, r));
    }
    if bg.double_clicked()
        && let Some(r) = hit_rel.and_then(|ri| m.relationships.get(ri))
    {
        app.powerpivot.form = Some(Form::edit_relationship(m, r));
    }
    if let Some(name) = open_table {
        app.powerpivot.table = name;
        app.powerpivot.view = View::Data;
    }
}

/// Draws the relationship lines (under the boxes); returns the one under the pointer.
fn draw_relationships(
    app: &SheetApp,
    painter: &egui::Painter,
    t: &Tokens,
    m: &Model,
    boxes: &HashMap<usize, Rect>,
    canvas: Rect,
    pointer: Option<Pos2>,
) -> Option<usize> {
    let mut hit_rel: Option<usize> = None;
    for (ri, r) in m.relationships.iter().enumerate() {
        let (Some(a), Some(b)) = (column_rect(boxes, m, r.from_table, r.from_column), column_rect(boxes, m, r.to_table, r.to_column)) else {
            continue;
        };
        let stacked = (a.center().x - b.center().x).abs() < BOX_W;
        let (pa, pb, da, db, qa, qb) = if stacked {
            // One box above the other: out of both right sides and down a shared lane.
            let lane = a.right().max(b.right()) + 22.0 + (ri % 4) as f32 * 10.0;
            (a.right_center(), b.right_center(), vec2(18.0, 0.0), vec2(18.0, 0.0), pos2(lane, a.center().y), pos2(lane, b.center().y))
        } else if a.center().x < b.center().x {
            (
                a.right_center(),
                b.left_center(),
                vec2(18.0, 0.0),
                vec2(-18.0, 0.0),
                a.right_center() + vec2(18.0, 0.0),
                b.left_center() - vec2(18.0, 0.0),
            )
        } else {
            (
                a.left_center(),
                b.right_center(),
                vec2(-18.0, 0.0),
                vec2(18.0, 0.0),
                a.left_center() - vec2(18.0, 0.0),
                b.right_center() + vec2(18.0, 0.0),
            )
        };
        let names = rel_names(m, r);
        let selected = app.powerpivot.diagram.selected.as_ref() == Some(&names);
        let color = if selected {
            t.accent
        } else if r.active {
            t.text_dim
        } else {
            t.text_disabled
        };
        let stroke = Stroke::new(if selected { 2.5 } else { 1.5 }, color);
        let pts = [pa, qa, qb, pb];
        if r.active {
            painter.add(Shape::line(pts.to_vec(), stroke));
        } else {
            for w in pts.windows(2) {
                if let [p0, p1] = w {
                    painter.extend(Shape::dashed_line(&[*p0, *p1], stroke, 6.0, 4.0));
                }
            }
        }
        // Many side `*`, one side `1`, arrows show which way filters travel (one → many).
        painter.text(pa + da * 0.5 + vec2(0.0, -9.0), Align2::CENTER_CENTER, "*", theme::ui_bold(13.0), color);
        painter.text(pb + db * 0.5 + vec2(0.0, -9.0), Align2::CENTER_CENTER, "1", theme::ui_bold(11.0), color);
        let mid = qa + (qb - qa) * 0.5;
        arrow(painter, mid, qa - qb, color);
        if r.cross_filter == CrossFilter::Both {
            arrow(painter, mid + (qb - qa).normalized() * 16.0, qb - qa, color);
        }
        if let Some(p) = pointer
            && canvas.contains(p)
            && pts.windows(2).any(|w| matches!(w, [p0, p1] if dist_to_segment(p, *p0, *p1) < 6.0))
        {
            hit_rel = Some(ri);
        }
    }
    hit_rel
}

/// The strip above the canvas: layout and create buttons, and the selected relationship's actions.
fn action_strip(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, m: &Model) {
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        if ui.button("Reset Layout").clicked() {
            app.powerpivot.diagram.pos.clear();
            app.powerpivot.diagram.pan = Vec2::ZERO;
        }
        if ui.button("Create Relationship…").clicked() {
            app.powerpivot.form = Some(Form::new_relationship(&app.powerpivot.table.clone()));
        }
        match app.powerpivot.diagram.selected.clone() {
            Some((ft, fc, tt, tc)) => {
                let names = (ft.clone(), fc.clone(), tt.clone(), tc.clone());
                let rel = m.relationships.iter().find(|r| rel_names(m, r) == names);
                ui.separator();
                ui.label(RichText::new(format!("{ft}[{fc}]  →  {tt}[{tc}]")).strong());
                let from = format!("{}[{}]", quote(&ft), fc);
                let to = format!("{}[{}]", quote(&tt), tc);
                if let Some(r) = rel {
                    let label = if r.active { "Make Inactive" } else { "Make Active" };
                    if ui.button(label).clicked() {
                        run(app, "powerpivot.editRelationship", json!({ "from": from, "to": to, "active": !r.active }));
                    }
                    let both = r.cross_filter == CrossFilter::Both;
                    if ui
                        .button(if both { "Filter: Both directions" } else { "Filter: Single direction" })
                        .on_hover_text("Which way filters travel along the relationship")
                        .clicked()
                    {
                        run(
                            app,
                            "powerpivot.editRelationship",
                            json!({ "from": from, "to": to, "crossFilter": if both { "single" } else { "both" } }),
                        );
                    }
                }
                if (ui.button("Delete").clicked() || ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)))
                    && run(app, "powerpivot.deleteRelationship", json!({ "from": from, "to": to })).is_some()
                {
                    app.powerpivot.diagram.selected = None;
                }
            }
            None => {
                ui.label(
                    RichText::new("Drag a column onto the matching column of another table to relate them · drag the background to pan")
                        .small()
                        .color(t.text_dim),
                );
            }
        }
    });
}

fn quote(t: &str) -> String {
    gridcraft_engine::datamodel::model::quote_table(t)
}
