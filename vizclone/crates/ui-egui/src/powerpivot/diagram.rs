//! Diagram View: each model table as a box listing its columns, with its relationships drawn
//! between the key columns. The canvas scrolls both ways (scroll bars, mouse wheel, or drag the
//! background) and zooms (toolbar or Ctrl/Cmd + wheel); Fit brings every table back into view.
//! Drag a box's title to move it, drag a column onto a column of another table to relate them,
//! click a line to select its relationship (Delete removes it), double-click a title to open the
//! table in Data View.

use std::collections::HashMap;

use egui::{Align2, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use gridcraft_engine::datamodel::{CrossFilter, Model};
use serde_json::json;

use super::{Form, View, run, toast};
use crate::SheetApp;
use crate::icons::{self, Icon};
use crate::theme::{self, Tokens};
use crate::widgets::small_button;

const BOX_W: f32 = 220.0;
const TITLE_H: f32 = 32.0;
const COL_H: f32 = 22.0;
/// Column rows a card shows before it scrolls.
const VISIBLE_ROWS: usize = 9;

/// Where a table's card is on screen, and its first visible column row.
#[derive(Clone, Copy)]
struct Geom {
    rect: Rect,
    first: usize,
}

pub struct DiagramState {
    /// Table name → top-left of its box, in canvas coordinates.
    pub pos: HashMap<String, Pos2>,
    /// Zoom factor (1.0 = 100%).
    pub zoom: f32,
    /// A scroll position to jump to on the next frame (Fit, background drag).
    pub scroll_to: Option<Vec2>,
    /// Fit everything into view on the next frame.
    pub fit: bool,
    /// The canvas viewport size, from the last frame.
    viewport: Vec2,
    /// The column a relationship drag started from.
    drag_from: Option<(String, String)>,
    /// The selected relationship, as (from table, from column, to table, to column).
    pub selected: Option<(String, String, String, String)>,
    /// The table whose card was clicked last (drawn with a thick border).
    pub active_table: Option<String>,
    /// Table name → how far its card's column list is scrolled, in rows.
    row_scroll: HashMap<String, f32>,
}

impl Default for DiagramState {
    fn default() -> Self {
        Self {
            pos: HashMap::new(),
            zoom: 1.0,
            scroll_to: None,
            fit: false,
            viewport: Vec2::ZERO,
            drag_from: None,
            selected: None,
            active_table: None,
            row_scroll: HashMap::new(),
        }
    }
}

const MIN_ZOOM: f32 = 0.3;
const MAX_ZOOM: f32 = 2.0;
/// Room past the last box, so a box can always be dragged further out.
const MARGIN: f32 = 240.0;

fn box_height(n_cols: usize) -> f32 {
    TITLE_H + n_cols.min(VISIBLE_ROWS) as f32 * COL_H + 8.0
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
/// A column scrolled out of its card maps to the card's title (above) or last row (below).
fn column_rect(boxes: &HashMap<usize, Geom>, m: &Model, t: usize, c: usize, z: f32) -> Option<Rect> {
    let g = boxes.get(&t)?;
    let b = g.rect;
    let n = m.tables.get(t)?.columns.len();
    if c < g.first || n == 0 {
        return Some(Rect::from_min_size(b.min, vec2(BOX_W, TITLE_H) * z));
    }
    let row = (c - g.first).min(n.min(VISIBLE_ROWS).saturating_sub(1));
    Some(Rect::from_min_size(pos2(b.min.x, b.min.y + (TITLE_H + row as f32 * COL_H) * z), vec2(BOX_W, COL_H) * z))
}

/// The unzoomed extent of every placed box (bottom-right corner).
fn extent(st: &DiagramState, m: &Model) -> Vec2 {
    m.tables
        .iter()
        .filter_map(|t| st.pos.get(&t.name).map(|p| vec2(p.x + BOX_W, p.y + box_height(t.columns.len()))))
        .fold(Vec2::ZERO, |a, b| a.max(b))
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

fn arrow(painter: &egui::Painter, at: Pos2, dir: Vec2, color: Color32, scale: f32) {
    let d = dir.normalized() * 7.0 * scale;
    let n = vec2(-d.y, d.x) * 0.7;
    painter.add(Shape::convex_polygon(vec![at + d, at - d + n, at - d - n], color, Stroke::NONE));
}

pub(super) fn show(app: &mut SheetApp, ui: &mut Ui, m: &Model) {
    let t = Tokens::get(ui.ctx());
    layout(&mut app.powerpivot.diagram, m);
    action_strip(app, ui, &t, m);
    let st = &mut app.powerpivot.diagram;
    if st.fit && st.viewport.x > 0.0 {
        let ext = extent(st, m) + vec2(30.0, 30.0);
        st.zoom = (st.viewport.x / ext.x.max(1.0)).min(st.viewport.y / ext.y.max(1.0)).clamp(MIN_ZOOM, 1.0);
        st.scroll_to = Some(Vec2::ZERO);
        st.fit = false;
    }
    // Solid, always-drawn scroll bars (the app default floats them, invisible until hovered), so
    // the way back from anywhere on the canvas is always in sight.
    let mut scroll = egui::style::ScrollStyle::solid();
    scroll.bar_width = 12.0;
    ui.spacing_mut().scroll = scroll;
    let mut area = egui::ScrollArea::both()
        .id_salt("pp_diagram")
        .auto_shrink([false, false])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .scroll_source(egui::scroll_area::ScrollSource::SCROLL_BAR | egui::scroll_area::ScrollSource::MOUSE_WHEEL);
    if let Some(off) = st.scroll_to.take() {
        area = area.scroll_offset(off);
    }
    let mut pan = Vec2::ZERO;
    let out = area.show(ui, |ui| canvas(app, ui, &t, m, &mut pan));
    let st = &mut app.powerpivot.diagram;
    st.viewport = out.inner_rect.size();
    if pan != Vec2::ZERO {
        st.scroll_to = Some((out.state.clamped_offset() - pan).max(Vec2::ZERO));
        ui.ctx().request_repaint();
    }
    // Ctrl/Cmd + wheel (or pinch) zooms.
    if ui.rect_contains_pointer(out.inner_rect) {
        let factor = ui.input(|i| i.zoom_delta());
        if (factor - 1.0).abs() > f32::EPSILON {
            st.zoom = (st.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        }
    }
}

/// The scrollable canvas: the dot grid, relationship lines and table boxes. Background drags are
/// returned in `pan` for the caller to turn into scrolling.
fn canvas(app: &mut SheetApp, ui: &mut Ui, t: &Tokens, m: &Model, pan: &mut Vec2) {
    let z = app.powerpivot.diagram.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    let size = ((extent(&app.powerpivot.diagram, m) + vec2(MARGIN, MARGIN)) * z).max(ui.available_size());
    let (content, bg) = ui.allocate_exact_size(size, Sense::click_and_drag());
    let visible = ui.clip_rect().intersect(content);
    let painter = ui.painter_at(visible);
    painter.rect_filled(visible, 0.0, t.desk);
    // Dot grid, anchored to the content so it scrolls with the boxes.
    let step = 24.0 * z;
    let mut gx = content.min.x + ((visible.min.x - content.min.x) / step).floor() * step;
    while gx < visible.max.x {
        let mut gy = content.min.y + ((visible.min.y - content.min.y) / step).floor() * step;
        while gy < visible.max.y {
            painter.circle_filled(pos2(gx, gy), 0.8, t.gridline);
            gy += step;
        }
        gx += step;
    }
    let origin = content.min;
    let st = &app.powerpivot.diagram;
    let boxes: HashMap<usize, Geom> = m
        .tables
        .iter()
        .enumerate()
        .filter_map(|(i, tb)| {
            let max_first = tb.columns.len().saturating_sub(VISIBLE_ROWS);
            let first = (st.row_scroll.get(&tb.name).copied().unwrap_or(0.0).round().max(0.0) as usize).min(max_first);
            st.pos
                .get(&tb.name)
                .map(|p| (i, Geom { rect: Rect::from_min_size(origin + p.to_vec2() * z, vec2(BOX_W, box_height(tb.columns.len())) * z), first }))
        })
        .collect();
    let pointer = ui.input(|i| i.pointer.interact_pos());
    let hit_rel = draw_relationships(app, &painter, t, m, &boxes, visible, pointer, z);
    let mut move_req: Option<(String, Vec2)> = None;
    let mut open_table: Option<String> = None;
    let mut drop_target: Option<(String, String)> = None;
    let mut any_box_hover = false;
    for (i, tb) in m.tables.iter().enumerate() {
        let Some(g) = boxes.get(&i).copied() else { continue };
        if !visible.intersects(g.rect) {
            continue;
        }
        let hit = draw_box(app, ui, &painter, t, m, i, g, visible, z, pointer);
        move_req = hit.moved.map(|d| (tb.name.clone(), d / z)).or(move_req);
        if hit.open {
            open_table = Some(tb.name.clone());
        }
        any_box_hover |= hit.hovered;
        if hit.drop.is_some() {
            drop_target = hit.drop;
        }
    }
    finish_relationship_drag(app, ui, &painter, t, m, &boxes, pointer, drop_target, z);
    let moving = move_req.is_some();
    if let Some((name, d)) = move_req
        && let Some(p) = app.powerpivot.diagram.pos.get_mut(&name)
    {
        // Boxes stay on the canvas: the scroll range starts at zero.
        *p = (*p + d).max(Pos2::ZERO);
    }
    if bg.dragged() && app.powerpivot.diagram.drag_from.is_none() && !moving {
        *pan += bg.drag_delta();
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

/// What happened on one table box this frame.
struct BoxHit {
    moved: Option<Vec2>,
    open: bool,
    hovered: bool,
    drop: Option<(String, String)>,
}

/// Draws table `i` as a card (white body, thin accent border, table icon and name, one row per
/// column with its icon) and handles its title drag, column drags and wheel scrolling.
#[allow(clippy::too_many_arguments)]
fn draw_box(
    app: &mut SheetApp,
    ui: &mut Ui,
    painter: &egui::Painter,
    t: &Tokens,
    m: &Model,
    i: usize,
    g: Geom,
    visible: Rect,
    z: f32,
    pointer: Option<Pos2>,
) -> BoxHit {
    let mut hit = BoxHit { moved: None, open: false, hovered: false, drop: None };
    let Some(tb) = m.tables.get(i) else { return hit };
    let b = g.rect;
    let active = app.powerpivot.diagram.active_table.as_deref() == Some(tb.name.as_str());
    painter.rect_filled(b.translate(vec2(1.5, 2.0)), 2.0, t.shadow);
    painter.rect_filled(b, 2.0, t.grid_bg);
    let title = Rect::from_min_size(b.min, vec2(BOX_W, TITLE_H) * z);
    let icon = Rect::from_center_size(pos2(title.left() + 16.0 * z, title.center().y), vec2(14.0, 14.0) * z);
    icons::paint(painter, icon, Icon::Table, t.text_dim);
    let name_clip = Rect::from_min_max(title.min, pos2(title.right() - 6.0 * z, title.max.y)).intersect(visible);
    painter.with_clip_rect(name_clip).text(
        icon.right_center() + vec2(7.0 * z, 0.0),
        Align2::LEFT_CENTER,
        &tb.name,
        theme::ui_bold(13.0 * z),
        t.accent,
    );
    let head = ui.interact(title.intersect(visible), ui.scope_id().with(("pp_box", i)), Sense::click_and_drag());
    if head.dragged() {
        hit.moved = Some(head.drag_delta());
    }
    if head.clicked() || head.drag_started() {
        app.powerpivot.diagram.active_table = Some(tb.name.clone());
    }
    hit.open = head.double_clicked();
    hit.hovered |= head.hovered();
    head.on_hover_text(format!(
        "{} · {} rows, {} columns\nDrag to move · double-click to open in Data View",
        tb.name,
        tb.row_count(),
        tb.columns.len()
    ));
    // Column rows, scrolled when the table has more than fit.
    let n = tb.columns.len();
    let shown = n.min(VISIBLE_ROWS);
    let list = Rect::from_min_size(pos2(b.min.x, title.max.y), vec2(BOX_W * z, shown as f32 * COL_H * z));
    let scrollable = n > VISIBLE_ROWS;
    let text_right = if scrollable { list.right() - 14.0 * z } else { list.right() - 6.0 * z };
    for (c, col) in tb.columns.iter().enumerate().skip(g.first).take(shown) {
        let rr = Rect::from_min_size(pos2(b.min.x, list.top() + (c - g.first) as f32 * COL_H * z), vec2(BOX_W, COL_H) * z);
        if !visible.intersects(rr) {
            continue;
        }
        let resp = ui.interact(rr.intersect(visible), ui.scope_id().with(("pp_col", i, c)), Sense::drag());
        hit.hovered |= resp.hovered();
        let dragging_this = app.powerpivot.diagram.drag_from.as_ref().is_some_and(|(dt, dc)| *dt == tb.name && *dc == col.name);
        if resp.hovered() || dragging_this {
            painter.rect_filled(Rect::from_min_max(rr.min + vec2(3.0, 1.0), pos2(text_right, rr.max.y - 1.0)), 2.0, t.hover);
        }
        let key = m.relationships.iter().any(|r| (r.from_table == i && r.from_column == c) || (r.to_table == i && r.to_column == c));
        let calc = col.expression.is_some();
        let ci = Rect::from_center_size(pos2(rr.left() + 30.0 * z, rr.center().y), vec2(13.0, 13.0) * z);
        icons::paint(painter, ci, if calc { Icon::Fx } else { Icon::Table }, if calc { t.accent } else { t.text_dim });
        let font = if key { theme::ui_bold(12.5 * z) } else { theme::ui_font(12.5 * z) };
        painter.with_clip_rect(Rect::from_min_max(rr.min, pos2(text_right, rr.max.y)).intersect(visible)).text(
            ci.right_center() + vec2(8.0 * z, 0.0),
            Align2::LEFT_CENTER,
            &col.name,
            font,
            t.text,
        );
        if resp.drag_started() {
            app.powerpivot.diagram.drag_from = Some((tb.name.clone(), col.name.clone()));
        }
        if pointer.is_some_and(|p| rr.contains(p)) {
            hit.drop = Some((tb.name.clone(), col.name.clone()));
        }
    }
    if scrollable {
        // The card's own scroll bar: track, thumb, and the wheel scrolls the card's columns.
        let track = Rect::from_min_max(pos2(list.right() - 11.0 * z, list.top() + 2.0), pos2(list.right() - 3.0 * z, list.bottom() - 2.0));
        painter.rect_filled(track, 2.0, t.input_bg);
        let max_first = n - shown;
        let thumb_h = (track.height() * shown as f32 / n as f32).max(12.0);
        let thumb_y = track.top() + (track.height() - thumb_h) * g.first as f32 / max_first.max(1) as f32;
        painter.rect_filled(Rect::from_min_size(pos2(track.left(), thumb_y), vec2(track.width(), thumb_h)), 2.0, t.text_disabled);
        let bar = ui.interact(track.intersect(visible), ui.scope_id().with(("pp_bar", i)), Sense::drag());
        let mut first = g.first as f32;
        if bar.dragged() {
            first += bar.drag_delta().y / (track.height() - thumb_h).max(1.0) * max_first as f32;
        }
        if pointer.is_some_and(|p| b.contains(p)) && !ui.input(|i| i.modifiers.command || i.modifiers.ctrl) {
            let dy = ui.input(|i| i.smooth_scroll_delta.y);
            if dy != 0.0 {
                first -= dy / (COL_H * z);
                // The wheel scrolled the card: the canvas keeps still.
                ui.ctx().input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            }
        }
        let fine = app.powerpivot.diagram.row_scroll.entry(tb.name.clone()).or_insert(0.0);
        *fine += first - g.first as f32;
        *fine = fine.clamp(0.0, max_first as f32);
    }
    let border = if active { Stroke::new(3.0, t.accent.gamma_multiply(0.45)) } else { Stroke::new(1.0, t.accent) };
    painter.rect_stroke(b, 2.0, border, StrokeKind::Inside);
    hit
}

/// A relationship drag in progress: a line from the source column to the pointer; on release
/// over another table's column, creates the relationship.
#[allow(clippy::too_many_arguments)]
fn finish_relationship_drag(
    app: &mut SheetApp,
    ui: &Ui,
    painter: &egui::Painter,
    t: &Tokens,
    m: &Model,
    boxes: &HashMap<usize, Geom>,
    pointer: Option<Pos2>,
    drop_target: Option<(String, String)>,
    z: f32,
) {
    let Some((ft, fc)) = app.powerpivot.diagram.drag_from.clone() else { return };
    if let (Some(fi), Some(p)) = (m.table_index(&ft), pointer) {
        let ci = m.table(&ft).and_then(|x| x.column_index(&fc)).unwrap_or(0);
        if let Some(rr) = column_rect(boxes, m, fi, ci, z) {
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

/// Draws the relationship lines (under the boxes); returns the one under the pointer.
#[allow(clippy::too_many_arguments)]
fn draw_relationships(
    app: &SheetApp,
    painter: &egui::Painter,
    t: &Tokens,
    m: &Model,
    boxes: &HashMap<usize, Geom>,
    canvas: Rect,
    pointer: Option<Pos2>,
    z: f32,
) -> Option<usize> {
    let mut hit_rel: Option<usize> = None;
    for (ri, r) in m.relationships.iter().enumerate() {
        let (Some(a), Some(b)) = (column_rect(boxes, m, r.from_table, r.from_column, z), column_rect(boxes, m, r.to_table, r.to_column, z)) else {
            continue;
        };
        // Right-angled routes, as in a model diagram: out of the facing sides, across to a
        // shared vertical, and into the other card.
        let stacked = (a.center().x - b.center().x).abs() < BOX_W * z;
        let stub = 14.0 * z;
        let (pa, pb, pts) = if stacked {
            let lane = a.right().max(b.right()) + (24.0 + (ri % 4) as f32 * 10.0) * z;
            let (pa, pb) = (a.right_center(), b.right_center());
            (pa, pb, vec![pa, pos2(lane, pa.y), pos2(lane, pb.y), pb])
        } else {
            let (pa, pb) = if a.center().x < b.center().x { (a.right_center(), b.left_center()) } else { (a.left_center(), b.right_center()) };
            let mx = (pa.x + pb.x) / 2.0;
            (pa, pb, vec![pa, pos2(mx, pa.y), pos2(mx, pb.y), pb])
        };
        let names = rel_names(m, r);
        let selected = app.powerpivot.diagram.selected.as_ref() == Some(&names);
        let color = if selected {
            t.accent
        } else if r.active {
            t.text
        } else {
            t.text_disabled
        };
        let stroke = Stroke::new(if selected { 2.5 } else { 1.2 }, color);
        if r.active {
            painter.add(Shape::line(pts.clone(), stroke));
        } else {
            for w in pts.windows(2) {
                if let [p0, p1] = w {
                    painter.extend(Shape::dashed_line(&[*p0, *p1], stroke, 6.0, 4.0));
                }
            }
        }
        // `*` at the many end, `1` at the one end, just off the card edge.
        let side = |p: Pos2, next: Pos2| if next.x >= p.x { 1.0 } else { -1.0 };
        let label_at = |p: Pos2, s: f32| p + vec2(s * (stub * 0.6), -9.0 * z);
        painter.text(label_at(pa, side(pa, pts.get(1).copied().unwrap_or(pb))), Align2::CENTER_CENTER, "*", theme::ui_font(13.0 * z), color);
        painter.text(label_at(pb, side(pb, pts.get(2).copied().unwrap_or(pa))), Align2::CENTER_CENTER, "1", theme::ui_font(12.0 * z), color);
        // The filter-direction marker: a small square with an arrow, on the longest segment,
        // pointing from the one side to the many side (both ways for two-way filtering).
        let (s0, s1) = pts
            .windows(2)
            .filter_map(|w| if let [p0, p1] = w { Some((*p0, *p1)) } else { None })
            .max_by(|x, y| (x.1 - x.0).length().total_cmp(&(y.1 - y.0).length()))
            .unwrap_or((pa, pb));
        let mid = s0 + (s1 - s0) * 0.5;
        let dir = s0 - s1; // segments run many → one; filters travel one → many
        let sq = Rect::from_center_size(mid, vec2(11.0, 11.0) * z);
        painter.rect_filled(sq, 1.0, t.window);
        painter.rect_stroke(sq, 1.0, Stroke::new(1.0, color), StrokeKind::Inside);
        if r.cross_filter == CrossFilter::Both {
            arrow(painter, mid + dir.normalized() * 2.0 * z, dir, color, z * 0.35);
            arrow(painter, mid - dir.normalized() * 2.0 * z, -dir, color, z * 0.35);
        } else {
            arrow(painter, mid, dir, color, z * 0.45);
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
        let st = &mut app.powerpivot.diagram;
        if small_button(ui, Icon::Expand, "Fit to Screen", "Zoom and scroll so every table is in view", false).clicked() {
            st.fit = true;
        }
        if small_button(ui, Icon::Minus, "", "Zoom out", false).clicked() {
            st.zoom = ((st.zoom * 10.0).round() / 10.0 - 0.1).clamp(MIN_ZOOM, MAX_ZOOM);
        }
        ui.label(RichText::new(format!("{:.0}%", st.zoom * 100.0)).color(t.text_dim));
        if small_button(ui, Icon::Plus, "", "Zoom in", false).clicked() {
            st.zoom = ((st.zoom * 10.0).round() / 10.0 + 0.1).clamp(MIN_ZOOM, MAX_ZOOM);
        }
        if small_button(ui, Icon::Zoom, "100%", "Zoom to 100%", false).clicked() {
            st.zoom = 1.0;
        }
        if small_button(ui, Icon::Normal, "Reset Layout", "Arrange the tables again: fact tables in the middle, lookups around them", false).clicked() {
            st.pos.clear();
            st.zoom = 1.0;
            st.scroll_to = Some(Vec2::ZERO);
        }
        ui.separator();
        if small_button(ui, Icon::Link, "Create Relationship", "Relate a column of one table to a key column of another", false).clicked() {
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
                if (small_button(ui, Icon::Delete, "Delete", "Delete the selected relationship", false).clicked() || ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)))
                    && run(app, "powerpivot.deleteRelationship", json!({ "from": from, "to": to })).is_some()
                {
                    app.powerpivot.diagram.selected = None;
                }
            }
            None => {
                ui.add(
                    egui::Label::new(
                        RichText::new("Drag a column onto the matching column of another table to relate them · scroll bars, wheel or background drag move around · Ctrl + wheel zooms")
                            .small()
                            .color(t.text_dim),
                    )
                    .truncate(),
                );
            }
        }
    });
}

fn quote(t: &str) -> String {
    gridcraft_engine::datamodel::model::quote_table(t)
}
