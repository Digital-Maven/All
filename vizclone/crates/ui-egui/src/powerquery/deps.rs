//! Query Dependencies: sources on the left, queries in the middle, where they load on the right.

use egui::{Align2, Pos2, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use gridcraft_engine::query::steps::{LoadTo, QueryDoc, identifiers};

use crate::SheetApp;
use crate::theme::{self, Tokens};

const W: f32 = 190.0;
const H: f32 = 44.0;

/// The outside sources a query reads (`Excel.CurrentWorkbook`, a file, a folder).
fn sources(m: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (f, label) in [
        ("Excel.CurrentWorkbook", "This workbook"),
        ("File.Contents", "File"),
        ("Folder.Files", "Folder"),
        ("Web.Contents", "Web"),
        ("#table", "Entered data"),
    ] {
        if m.contains(f) {
            // Name files and folders by their path when it is a literal.
            let path =
                m.split(&format!("{f}(\"")).nth(1).and_then(|r| r.split('"').next()).map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_string());
            out.push(match path {
                Some(p) if !p.is_empty() => format!("{label}: {p}"),
                _ => label.to_string(),
            });
        }
    }
    out
}

pub fn show(app: &mut SheetApp, ctx: &egui::Context, doc: &QueryDoc) {
    let t = Tokens::get(ctx);
    let mut open = true;
    let screen = ctx.content_rect();
    egui::Window::new("Query Dependencies")
        .open(&mut open)
        .default_size(vec2((screen.width() - 120.0).min(1100.0), (screen.height() - 160.0).min(640.0)))
        .show(ctx, |ui| {
            let (canvas, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
            let painter = ui.painter_at(canvas);
            painter.rect_filled(canvas, 0.0, t.desk);
            // Columns: sources, queries (by depth), destinations.
            let mut depth: Vec<(String, usize)> = Vec::new();
            for q in &doc.queries {
                let mut d = 0;
                let mut stack = vec![(q.name.clone(), 0usize)];
                while let Some((n, k)) = stack.pop() {
                    d = d.max(k);
                    if k < 20 {
                        for dep in doc.dependencies(&n) {
                            if doc.query(&dep).is_some() {
                                stack.push((dep, k + 1));
                            }
                        }
                    }
                }
                depth.push((q.name.clone(), d));
            }
            let max_depth = depth.iter().map(|x| x.1).max().unwrap_or(0);
            let mut pos: Vec<(String, Pos2, &str)> = Vec::new();
            let mut srcs: Vec<String> = Vec::new();
            for q in &doc.queries {
                for s in sources(&q.m) {
                    if !srcs.contains(&s) {
                        srcs.push(s);
                    }
                }
            }
            let col_x = |c: usize| canvas.min.x + 20.0 + c as f32 * (W + 60.0);
            for (i, s) in srcs.iter().enumerate() {
                pos.push((format!("src:{s}"), pos2(col_x(0), canvas.min.y + 20.0 + i as f32 * (H + 20.0)), "source"));
            }
            for d in 0..=max_depth {
                for (i, (n, _)) in depth.iter().filter(|x| x.1 == d).enumerate() {
                    pos.push((n.clone(), pos2(col_x(1 + d), canvas.min.y + 20.0 + i as f32 * (H + 20.0)), "query"));
                }
            }
            let dest_col = 2 + max_depth;
            let dests = [("dest:table", "Worksheet tables"), ("dest:model", "Data Model"), ("dest:none", "Connection only")];
            for (i, (k, _)) in dests.iter().enumerate() {
                pos.push((k.to_string(), pos2(col_x(dest_col), canvas.min.y + 20.0 + i as f32 * (H + 20.0)), "dest"));
            }
            let at = |k: &str| pos.iter().find(|(n, _, _)| n == k).map(|(_, p, _)| *p);
            // Edges.
            let mut edges: Vec<(String, String)> = Vec::new();
            for q in &doc.queries {
                for s in sources(&q.m) {
                    edges.push((format!("src:{s}"), q.name.clone()));
                }
                for dep in doc.dependencies(&q.name) {
                    if doc.query(&dep).is_some() {
                        edges.push((dep, q.name.clone()));
                    }
                }
                let dest = match q.load {
                    LoadTo::Table if q.loaded_table.is_some() => "dest:table",
                    LoadTo::Model => "dest:model",
                    LoadTo::None => "dest:none",
                    LoadTo::Table => "",
                };
                if !dest.is_empty() {
                    edges.push((q.name.clone(), dest.to_string()));
                }
            }
            for (a, b) in &edges {
                if let (Some(p), Some(q)) = (at(a), at(b)) {
                    painter.line_segment([p + vec2(W, H / 2.0), q + vec2(0.0, H / 2.0)], Stroke::new(1.5, t.text_dim));
                }
            }
            for (k, p, kind) in &pos {
                let r = Rect::from_min_size(*p, vec2(W, H));
                let (fill, label) = match *kind {
                    "source" => (t.notice_info_bg, k.trim_start_matches("src:").to_string()),
                    "dest" => (t.accent_soft, dests.iter().find(|d| d.0 == k).map(|d| d.1).unwrap_or_default().to_string()),
                    _ => (t.window, k.clone()),
                };
                painter.rect_filled(r, 6.0, fill);
                painter.rect_stroke(r, 6.0, Stroke::new(1.0, t.ribbon_border), StrokeKind::Inside);
                painter.with_clip_rect(r.shrink(4.0)).text(r.center(), Align2::CENTER_CENTER, label, theme::ui_font(12.0), t.text);
                if *kind == "query" {
                    let resp = ui.interact(r, ui.scope_id().with(("pq_dep", k)), Sense::click());
                    if resp.on_hover_text("Open this query").clicked() {
                        app.powerquery.query = k.clone();
                        app.powerquery.step = None;
                    }
                }
            }
            let _ = identifiers;
        });
    if !open {
        app.powerquery.deps = false;
    }
}
