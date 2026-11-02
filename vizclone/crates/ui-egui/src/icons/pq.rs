//! Icons of the Power Query Editor's ribbon, drawn in code on the same 16×16 grid as the rest of
//! the set (original artwork).

use egui::Color32;

use super::{G, Palette};
use crate::theme::Cat;

/// A Power Query ribbon icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Pq {
    NewSource,
    EnterData,
    Refresh,
    RefreshAll,
    AdvancedEditor,
    Manage,
    ChooseColumns,
    RemoveColumns,
    KeepRows,
    RemoveRows,
    SplitColumn,
    GroupBy,
    DataType,
    Headers,
    Replace,
    MergeQueries,
    AppendQueries,
    CombineFiles,
    Parameters,
    CloseLoad,
    Transpose,
    ReverseRows,
    CountRows,
    DetectType,
    Rename,
    PivotColumn,
    Unpivot,
    MoveColumns,
    ToList,
    Format,
    Extract,
    Parse,
    Standard,
    Scientific,
    Duration,
    ExpandColumn,
    FromExamples,
    CustomColumn,
    Conditional,
    IndexColumn,
    Dependencies,
    GoToColumn,
    Monospace,
    Whitespace,
    Quality,
    Profile,
    AllRows,
}

/// A small table: outline, a tinted header row, one column divider and row lines.
fn table(g: &G<'_>, (x, y, w, h): (f32, f32, f32, f32), c: Color32, head: Color32, light: Color32) {
    g.rect(x, y, w, 3.0, Some(head), None);
    g.rect(x, y, w, h, None, Some(c));
    g.stroke(&[(x + w / 2.0, y), (x + w / 2.0, y + h)], light);
    let mut ry = y + 3.0 + (h - 3.0) / 3.0;
    while ry < y + h - 0.5 {
        g.stroke(&[(x, ry), (x + w, ry)], light);
        ry += (h - 3.0) / 3.0;
    }
}

/// A circular arrow centred at (cx, cy), radius r, open at the top right, head at its end.
fn circular_arrow(g: &G<'_>, cx: f32, cy: f32, r: f32, c: Color32) {
    let pts: Vec<(f32, f32)> = (0..=16)
        .map(|i| {
            let a = (-50.0 + i as f32 * 17.0_f32).to_radians();
            (cx + r * a.cos(), cy + r * a.sin())
        })
        .collect();
    g.line(&pts, c, 1.4);
    let a = (-50.0_f32).to_radians();
    let (ex, ey) = (cx + r * a.cos(), cy + r * a.sin());
    g.poly(&[(ex - 2.6, ey - 1.0), (ex + 1.6, ey - 2.4), (ex + 0.6, ey + 2.0)], c);
}

fn arrow_down(g: &G<'_>, x: f32, y0: f32, y1: f32, c: Color32) {
    g.line(&[(x, y0), (x, y1 - 1.5)], c, 1.3);
    g.poly(&[(x - 2.0, y1 - 2.5), (x, y1), (x + 2.0, y1 - 2.5)], c);
}

fn sparkle(g: &G<'_>, x: f32, y: f32, s: f32, c: Color32) {
    g.poly(
        &[
            (x, y - s),
            (x + s * 0.3, y - s * 0.3),
            (x + s, y),
            (x + s * 0.3, y + s * 0.3),
            (x, y + s),
            (x - s * 0.3, y + s * 0.3),
            (x - s, y),
            (x - s * 0.3, y - s * 0.3),
        ],
        c,
    );
}

fn cross(g: &G<'_>, x: f32, y: f32, s: f32, c: Color32) {
    g.line(&[(x - s, y - s), (x + s, y + s)], c, 1.5);
    g.line(&[(x - s, y + s), (x + s, y - s)], c, 1.5);
}

pub(super) fn paint(g: &G<'_>, icon: Pq, c: Color32, light: Color32, pal: &Palette) {
    let blue = pal.cat(Cat::Blue);
    let green = pal.cat(Cat::Green);
    let red = pal.cat(Cat::Red);
    let orange = pal.cat(Cat::Orange);
    let purple = pal.cat(Cat::Purple);
    let amber = pal.cat(Cat::Amber);
    let teal = pal.cat(Cat::Teal);
    let wash = |k: Cat| pal.wash(k);
    let head = wash(Cat::Blue);
    match icon {
        Pq::NewSource => {
            for y in [2.5, 6.5, 10.5] {
                g.rect(1.5, y, 9.0, 3.2, Some(wash(Cat::Slate)), Some(c));
            }
            g.line(&[(13.0, 2.0), (13.0, 8.0)], green, 1.6);
            g.line(&[(10.0, 5.0), (16.0, 5.0)], green, 1.6);
        }
        Pq::EnterData => {
            table(g, (1.0, 1.5, 11.0, 11.0), c, head, light);
            g.line(&[(8.5, 14.5), (14.5, 8.5)], orange, 2.0);
            g.poly(&[(7.5, 15.5), (8.0, 13.5), (9.5, 15.0)], c);
        }
        Pq::Refresh => circular_arrow(g, 8.0, 8.5, 5.5, green),
        Pq::RefreshAll => {
            table(g, (1.0, 1.0, 8.0, 8.0), c, head, light);
            g.circle(10.5, 10.5, 5.0, Some(pal.paper), None);
            circular_arrow(g, 10.5, 10.5, 3.8, green);
        }
        Pq::AdvancedEditor => {
            g.rect(2.5, 1.5, 11.0, 13.0, Some(pal.paper), Some(c));
            g.text(8.0, 8.0, "</>", 5.5, blue, (true, false));
        }
        Pq::Manage => {
            table(g, (1.0, 1.0, 10.0, 10.0), c, head, light);
            g.circle(12.0, 12.0, 3.2, Some(pal.paper), Some(c));
            g.circle(12.0, 12.0, 1.2, Some(c), None);
            for (dx, dy) in [(0.0, -4.0), (0.0, 4.0), (-4.0, 0.0), (4.0, 0.0)] {
                g.line(&[(12.0 + dx * 0.8, 12.0 + dy * 0.8), (12.0 + dx, 12.0 + dy)], c, 1.6);
            }
        }
        Pq::ChooseColumns => {
            g.rect(5.5, 1.5, 4.5, 13.0, Some(wash(Cat::Green)), None);
            g.rect(1.0, 1.5, 14.0, 13.0, None, Some(c));
            g.stroke(&[(5.5, 1.5), (5.5, 14.5)], light);
            g.stroke(&[(10.0, 1.5), (10.0, 14.5)], light);
            g.line(&[(6.2, 8.0), (7.5, 9.6), (9.6, 6.0)], green, 1.5);
        }
        Pq::RemoveColumns => {
            g.rect(1.0, 1.5, 14.0, 13.0, None, Some(c));
            g.stroke(&[(5.5, 1.5), (5.5, 14.5)], light);
            g.stroke(&[(10.0, 1.5), (10.0, 14.5)], light);
            g.rect(10.0, 1.5, 5.0, 13.0, Some(wash(Cat::Red)), None);
            cross(g, 12.5, 8.0, 1.8, red);
        }
        Pq::KeepRows => {
            g.rect(1.0, 5.5, 14.0, 5.0, Some(wash(Cat::Green)), None);
            g.rect(1.0, 1.5, 14.0, 13.0, None, Some(c));
            for y in [5.5, 10.5] {
                g.stroke(&[(1.0, y), (15.0, y)], light);
            }
            g.line(&[(5.5, 8.0), (7.0, 9.5), (10.0, 6.5)], green, 1.5);
        }
        Pq::RemoveRows => {
            g.rect(1.0, 1.5, 14.0, 13.0, None, Some(c));
            for y in [5.5, 10.5] {
                g.stroke(&[(1.0, y), (15.0, y)], light);
            }
            g.rect(1.0, 10.5, 14.0, 4.0, Some(wash(Cat::Red)), None);
            g.line(&[(5.0, 12.5), (11.0, 12.5)], red, 1.8);
        }
        Pq::SplitColumn => {
            g.rect(1.5, 2.0, 5.0, 12.0, None, Some(c));
            g.rect(9.5, 2.0, 5.0, 12.0, None, Some(c));
            g.line(&[(8.0, 1.0), (8.0, 15.0)], red, 1.0);
            g.poly(&[(6.0, 6.0), (4.0, 8.0), (6.0, 10.0)], blue);
            g.poly(&[(10.0, 6.0), (12.0, 8.0), (10.0, 10.0)], blue);
        }
        Pq::GroupBy => {
            for y in [3.0, 6.0] {
                g.stroke(&[(1.5, y), (6.0, y)], c);
            }
            for y in [10.0, 13.0] {
                g.stroke(&[(1.5, y), (6.0, y)], c);
            }
            g.stroke(&[(7.5, 2.5), (8.5, 2.5), (8.5, 6.5), (7.5, 6.5)], blue);
            g.stroke(&[(7.5, 9.5), (8.5, 9.5), (8.5, 13.5), (7.5, 13.5)], blue);
            for y in [2.5, 9.5] {
                g.line(&[(14.5, y), (10.5, y), (12.8, y + 2.0), (10.5, y + 4.0), (14.5, y + 4.0)], blue, 1.2);
            }
        }
        Pq::DataType => {
            g.text(8.0, 4.8, "ABC", 6.0, c, (true, false));
            g.text(8.0, 11.5, "123", 6.0, blue, (true, false));
        }
        Pq::Headers => {
            g.rect(1.0, 4.0, 14.0, 4.0, Some(wash(Cat::Blue)), None);
            g.rect(1.0, 4.0, 14.0, 11.0, None, Some(c));
            g.stroke(&[(8.0, 4.0), (8.0, 15.0)], light);
            g.stroke(&[(1.0, 11.5), (15.0, 11.5)], light);
            g.line(&[(8.0, 7.5), (8.0, 1.5)], blue, 1.4);
            g.poly(&[(6.0, 3.0), (8.0, 0.5), (10.0, 3.0)], blue);
        }
        Pq::Replace => {
            g.text(4.0, 4.5, "a", 8.0, c, (true, false));
            g.text(12.0, 11.5, "b", 8.0, blue, (true, false));
            g.line(&[(4.0, 8.5), (4.0, 12.0), (8.0, 12.0)], orange, 1.3);
            g.poly(&[(7.5, 10.0), (10.0, 12.0), (7.5, 14.0)], orange);
        }
        Pq::MergeQueries => {
            g.circle(5.8, 8.0, 4.6, Some(wash(Cat::Blue)), Some(blue));
            g.circle(10.2, 8.0, 4.6, Some(wash(Cat::Green)), Some(green));
        }
        Pq::AppendQueries => {
            table(g, (2.0, 0.8, 12.0, 6.0), c, head, light);
            table(g, (2.0, 9.2, 12.0, 6.0), c, wash(Cat::Green), light);
            g.line(&[(8.0, 6.8), (8.0, 9.2)], green, 1.6);
        }
        Pq::CombineFiles => {
            g.rect(5.0, 1.0, 9.0, 10.0, Some(pal.paper), Some(light));
            g.rect(3.5, 2.5, 9.0, 10.0, Some(pal.paper), Some(light));
            g.rect(2.0, 4.0, 9.0, 10.0, Some(pal.paper), Some(c));
            arrow_down(g, 6.5, 6.0, 12.5, green);
        }
        Pq::Parameters => {
            for (y, k) in [(3.5, 10.0), (8.0, 5.0), (12.5, 8.5)] {
                g.stroke(&[(1.5, y), (14.5, y)], c);
                g.circle(k, y, 1.8, Some(blue), None);
            }
        }
        Pq::CloseLoad => {
            table(g, (1.0, 5.0, 10.0, 10.0), c, wash(Cat::Green), light);
            g.line(&[(14.5, 1.5), (14.5, 5.0), (9.0, 5.0)], green, 1.5);
            g.poly(&[(10.0, 2.8), (7.2, 5.0), (10.0, 7.2)], green);
        }
        Pq::Transpose => {
            table(g, (1.0, 1.0, 8.0, 8.0), c, head, light);
            g.rect(11.0, 7.0, 4.0, 8.0, None, Some(c));
            g.rect(7.0, 11.0, 8.0, 4.0, None, Some(c));
            g.line(&[(4.0, 10.5), (4.0, 13.0), (5.5, 13.0)], blue, 1.3);
            g.poly(&[(5.0, 11.2), (7.0, 13.0), (5.0, 14.8)], blue);
        }
        Pq::ReverseRows => {
            arrow_down(g, 5.0, 2.0, 14.5, c);
            g.line(&[(11.0, 14.0), (11.0, 3.5)], blue, 1.3);
            g.poly(&[(9.0, 4.5), (11.0, 1.5), (13.0, 4.5)], blue);
        }
        Pq::CountRows => {
            for y in [3.0, 6.0, 9.0, 12.0] {
                g.stroke(&[(1.5, y), (7.0, y)], light);
            }
            g.text(11.5, 8.0, "#", 11.0, blue, (true, false));
        }
        Pq::DetectType => {
            g.text(7.0, 10.0, "ABC", 6.0, c, (true, false));
            sparkle(g, 12.5, 4.0, 3.0, amber);
        }
        Pq::Rename => {
            g.rect(1.0, 4.0, 14.0, 8.0, Some(pal.paper), Some(c));
            g.text(6.0, 8.0, "ab", 7.0, c, (false, false));
            g.line(&[(11.0, 2.5), (11.0, 13.5)], blue, 1.3);
        }
        Pq::PivotColumn => {
            g.rect(1.0, 4.0, 4.0, 11.0, Some(wash(Cat::Blue)), Some(c));
            g.rect(7.0, 1.0, 8.0, 4.0, Some(wash(Cat::Green)), Some(c));
            g.line(&[(3.0, 3.0), (3.0, 1.5), (5.5, 1.5)], blue, 1.3);
            g.poly(&[(5.0, 0.0), (7.0, 1.5), (5.0, 3.0)], blue);
            g.rect(7.0, 7.0, 8.0, 8.0, None, Some(light));
        }
        Pq::Unpivot => {
            g.rect(1.0, 1.0, 8.0, 4.0, Some(wash(Cat::Green)), Some(c));
            g.rect(11.0, 7.0, 4.0, 8.0, Some(wash(Cat::Blue)), Some(c));
            g.line(&[(10.5, 3.0), (13.0, 3.0), (13.0, 5.0)], blue, 1.3);
            g.poly(&[(11.5, 4.5), (13.0, 7.0), (14.5, 4.5)], blue);
            g.rect(1.0, 7.0, 8.0, 8.0, None, Some(light));
        }
        Pq::MoveColumns => {
            g.rect(1.0, 3.0, 5.0, 11.0, None, Some(c));
            g.rect(10.0, 3.0, 5.0, 11.0, Some(wash(Cat::Blue)), Some(c));
            g.line(&[(4.0, 1.5), (12.5, 1.5)], blue, 1.2);
            g.poly(&[(11.5, 0.0), (13.5, 1.5), (11.5, 3.0)], blue);
        }
        Pq::ToList => {
            for y in [3.5, 8.0, 12.5] {
                g.circle(3.0, y, 1.3, Some(blue), None);
                g.stroke(&[(6.0, y), (14.5, y)], c);
            }
        }
        Pq::Format => g.text(8.0, 8.5, "Aa", 10.0, c, (false, false)),
        Pq::Extract => {
            g.text(8.0, 7.0, "ABC", 6.5, light, (true, false));
            g.text(8.0, 7.0, " B ", 6.5, blue, (true, false));
            g.stroke(&[(5.5, 11.0), (5.5, 13.0), (10.5, 13.0), (10.5, 11.0)], blue);
        }
        Pq::Parse => g.text(8.0, 8.0, "{ }", 9.0, purple, (true, false)),
        Pq::Standard => {
            g.text(4.5, 4.5, "+", 7.0, c, (true, false));
            g.text(11.5, 4.5, "-", 7.0, c, (true, false));
            g.text(4.5, 11.5, "x", 7.0, blue, (true, false));
            g.text(11.5, 11.5, "/", 7.0, blue, (true, false));
        }
        Pq::Scientific => {
            g.text(6.5, 9.5, "x", 11.0, c, (false, true));
            g.text(12.5, 4.5, "2", 6.0, blue, (true, false));
        }
        Pq::Duration => {
            g.stroke(&[(3.0, 1.5), (13.0, 1.5)], c);
            g.stroke(&[(3.0, 14.5), (13.0, 14.5)], c);
            g.stroke(&[(4.0, 1.5), (12.0, 14.5)], c);
            g.stroke(&[(12.0, 1.5), (4.0, 14.5)], c);
            g.poly(&[(5.5, 13.5), (10.5, 13.5), (8.0, 10.0)], amber);
        }
        Pq::ExpandColumn => {
            table(g, (1.0, 1.0, 9.0, 9.0), c, head, light);
            g.line(&[(9.5, 9.5), (14.0, 14.0)], blue, 1.4);
            g.poly(&[(15.0, 15.0), (10.5, 14.6), (14.6, 10.5)], blue);
        }
        Pq::FromExamples => {
            table(g, (1.0, 3.0, 11.0, 12.0), c, head, light);
            sparkle(g, 12.5, 3.5, 3.2, amber);
        }
        Pq::CustomColumn => {
            table(g, (1.0, 1.0, 9.0, 13.0), c, head, light);
            g.rect(8.0, 7.0, 7.5, 7.5, Some(pal.paper), Some(c));
            g.text(11.8, 10.7, "fx", 6.0, blue, (false, true));
        }
        Pq::Conditional => {
            g.line(&[(1.5, 8.0), (6.0, 8.0)], c, 1.3);
            g.line(&[(6.0, 8.0), (10.5, 3.5), (13.0, 3.5)], green, 1.3);
            g.line(&[(6.0, 8.0), (10.5, 12.5), (13.0, 12.5)], red, 1.3);
            g.circle(6.0, 8.0, 1.6, Some(c), None);
            g.circle(14.0, 3.5, 1.4, Some(green), None);
            g.circle(14.0, 12.5, 1.4, Some(red), None);
        }
        Pq::IndexColumn => {
            g.rect(3.0, 1.0, 10.0, 14.0, None, Some(c));
            for (i, y) in [4.0f32, 8.0, 12.0].iter().enumerate() {
                g.text(8.0, *y, ["1", "2", "3"].get(i).copied().unwrap_or(""), 5.5, blue, (true, false));
            }
        }
        Pq::Dependencies => {
            g.stroke(&[(4.0, 4.0), (12.0, 4.0)], light);
            g.stroke(&[(4.0, 4.0), (8.0, 12.0)], light);
            g.stroke(&[(12.0, 4.0), (8.0, 12.0)], light);
            g.rect(1.0, 2.0, 5.0, 4.0, Some(wash(Cat::Blue)), Some(c));
            g.rect(10.0, 2.0, 5.0, 4.0, Some(wash(Cat::Green)), Some(c));
            g.rect(5.5, 10.0, 5.0, 4.0, Some(wash(Cat::Orange)), Some(c));
        }
        Pq::GoToColumn => {
            g.rect(1.0, 5.0, 14.0, 10.0, None, Some(c));
            g.rect(6.0, 5.0, 4.0, 10.0, Some(wash(Cat::Blue)), None);
            g.stroke(&[(6.0, 5.0), (6.0, 15.0)], light);
            g.stroke(&[(10.0, 5.0), (10.0, 15.0)], light);
            arrow_down(g, 8.0, 0.5, 5.0, blue);
        }
        Pq::Monospace => {
            g.rect(1.0, 2.5, 14.0, 11.0, None, Some(light));
            g.text(8.0, 8.0, "Mi", 8.0, c, (true, false));
            for x in [4.5, 11.5] {
                g.stroke(&[(x - 2.5, 12.0), (x + 2.5, 12.0)], blue);
            }
        }
        Pq::Whitespace => {
            g.text(3.5, 7.5, "a", 7.0, c, (false, false));
            g.text(12.5, 7.5, "b", 7.0, c, (false, false));
            g.circle(8.0, 8.5, 1.1, Some(blue), None);
            g.stroke(&[(1.5, 13.0), (14.5, 13.0)], light);
        }
        Pq::Quality => {
            g.rect(1.0, 6.0, 9.5, 4.0, Some(green), None);
            g.rect(10.5, 6.0, 2.5, 4.0, Some(red), None);
            g.rect(13.0, 6.0, 2.0, 4.0, Some(light), None);
            g.text(5.0, 2.8, "OK", 4.5, c, (true, false));
        }
        Pq::Profile => {
            g.rect(1.0, 1.0, 14.0, 14.0, None, Some(c));
            for (x, h) in [(3.0, 6.0), (6.0, 9.0), (9.0, 4.0), (12.0, 7.0)] {
                g.rect(x, 13.5 - h, 2.0, h, Some(teal), None);
            }
        }
        Pq::AllRows => {
            for y in [2.5, 5.5, 8.5, 11.5, 14.0] {
                g.stroke(&[(1.5, y), (10.0, y)], c);
            }
            arrow_down(g, 13.0, 1.5, 15.0, blue);
        }
    }
}
