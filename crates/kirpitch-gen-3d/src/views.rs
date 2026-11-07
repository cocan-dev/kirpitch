// SPDX-License-Identifier: GPL-3.0-or-later

//! Flat views of the model as SVG, for review.

use std::fmt::Write;

use crate::{Material, Model};

/// Which way to look at the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    /// From above (x right, y up).
    Top,
    /// From the front, along the board's y axis (x right, z up).
    Front,
    /// From the right (y right, z up).
    Side,
}

impl View {
    /// All views.
    pub const ALL: [View; 3] = [View::Top, View::Front, View::Side];

    /// A short name.
    pub fn name(self) -> &'static str {
        match self {
            View::Top => "top",
            View::Front => "front",
            View::Side => "side",
        }
    }

    /// Axes (horizontal, vertical, depth) of the view.
    fn axes(self) -> (usize, usize, usize) {
        match self {
            View::Top => (0, 1, 2),
            View::Front => (0, 2, 1),
            View::Side => (1, 2, 0),
        }
    }
}

fn fmt(v: f64) -> String {
    kirpitch_core::format_number((v * 1000.0).round() / 1000.0)
}

fn hex(m: Material) -> String {
    let (r, g, b) = m.rgb();
    let c = |v: f64| (v * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

/// Draws the model seen from one side as an SVG image, with the board surface
/// as a line in the front and side views.
pub fn view_svg(model: &Model, view: View) -> String {
    let (h, v, depth) = view.axes();
    let (lo, hi) = model.bounds();
    let margin = 0.5;
    let (x0, x1) = (lo[h] - margin, hi[h] + margin);
    let (y0, y1) = (lo[v] - margin, hi[v] + margin);
    let (w, hgt) = (x1 - x0, y1 - y0);
    let mut out = String::new();
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\">",
        fmt(w),
        fmt(hgt)
    );
    let mut order: Vec<usize> = (0..model.boxes.len()).collect();
    // Far boxes first. The top view looks down z, the others look along y or x.
    order.sort_by(|a, b| {
        let (ba, bb) = (&model.boxes[*a], &model.boxes[*b]);
        ba.max[depth]
            .partial_cmp(&bb.max[depth])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for i in order {
        let b = &model.boxes[i];
        let _ = writeln!(
            out,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"#444\" stroke-width=\"0.02\"/>",
            fmt(b.min[h] - x0),
            fmt(y1 - b.max[v]),
            fmt(b.max[h] - b.min[h]),
            fmt(b.max[v] - b.min[v]),
            hex(b.material)
        );
    }
    if view != View::Top {
        let _ = writeln!(
            out,
            "<line x1=\"0\" y1=\"{0}\" x2=\"{1}\" y2=\"{0}\" stroke=\"#2a7\" stroke-width=\"0.04\"/>",
            fmt(y1),
            fmt(w)
        );
    }
    out.push_str("</svg>\n");
    out
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// One dimension: a line with arrows at both ends, a number and extension lines
/// from the edges that it measures.
struct Dimension {
    /// Along x (the others run along y, and their number is turned to read from the right).
    horizontal: bool,
    /// The position across the line (y of a horizontal one, x of a vertical one).
    at: f64,
    /// The two ends along the line.
    from: f64,
    to: f64,
    /// Where the extension lines start, across the line.
    reach: f64,
    value: f64,
}

/// Draws the model the way an engineer would on paper: the top view as the main
/// one, the front view below it and the left view to the left of the front view
/// (third-angle projection), each lined up with the others by thin dashed
/// lines, with the overall size in millimetres.
pub fn drawing_svg(model: &Model) -> String {
    let (lo, hi) = model.bounds();
    let (w, d, h) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    let s = w.max(d).max(h).max(0.1);
    let ts = s * 0.05; // text size
    let sw = (s * 0.006).max(0.01); // line width
    let gap = ts * 5.0;
    let margin = ts * 2.5;
    // Left edge of the left view, and of the top and front views.
    let (lx, fx) = (margin, margin + d + gap);
    let ty = margin + ts * 1.5; // top of the top view
    let fy = ty + d + gap; // top of the front and left views
    let width = fx + w + ts * 7.0 + margin;
    let height = fy + h + ts * 5.5 + margin;
    let blue = "#1a5fb4";

    let mut out = String::new();
    let _ = writeln!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\" font-family=\"sans-serif\">",
        fmt(width),
        fmt(height)
    );
    let _ = writeln!(
        out,
        "<rect width=\"{}\" height=\"{}\" fill=\"#fbfbf8\"/>",
        fmt(width),
        fmt(height)
    );

    // The title block, in the empty corner.
    let _ = writeln!(
        out,
        "<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-weight=\"bold\" fill=\"#222\">THIRD-ANGLE PROJECTION</text>",
        fmt(margin),
        fmt(margin + ts),
        fmt(ts)
    );
    let _ = writeln!(
        out,
        "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"#555\">{}</text>",
        fmt(margin),
        fmt(margin + ts * 2.5),
        fmt(ts * 0.85),
        escape(&model.name)
    );
    let _ = writeln!(
        out,
        "<text x=\"{}\" y=\"{}\" font-size=\"{}\" fill=\"#555\">all sizes in mm</text>",
        fmt(margin),
        fmt(margin + ts * 3.8),
        fmt(ts * 0.85)
    );

    // Lines that tie the views together: the edges of the top view run down into
    // the front view, the levels of the front view run left into the left view.
    let guide = |out: &mut String, x1: f64, y1: f64, x2: f64, y2: f64| {
        let _ = writeln!(
            out,
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#8a9\" stroke-width=\"{}\" stroke-dasharray=\"{} {}\"/>",
            fmt(x1),
            fmt(y1),
            fmt(x2),
            fmt(y2),
            fmt(sw * 0.8),
            fmt(ts * 0.5),
            fmt(ts * 0.3)
        );
    };
    for x in [fx, fx + w] {
        guide(&mut out, x, ty + d, x, fy);
    }
    for y in [fy, fy + h] {
        guide(&mut out, lx + d, y, fx, y);
    }

    // A view: the boxes seen from one side, the far ones first.
    let view = |out: &mut String,
                label: &str,
                label_at: (f64, f64),
                depth: usize,
                far_is_high: bool,
                rect: &dyn Fn(&crate::Cuboid) -> (f64, f64, f64, f64)| {
        let _ = writeln!(
            out,
            "<text x=\"{}\" y=\"{}\" font-size=\"{}\" font-weight=\"bold\" fill=\"#222\">{label}</text>",
            fmt(label_at.0),
            fmt(label_at.1),
            fmt(ts * 0.9)
        );
        let mut order: Vec<usize> = (0..model.boxes.len()).collect();
        order.sort_by(|a, b| {
            let mid = |i: &usize| (model.boxes[*i].min[depth] + model.boxes[*i].max[depth]) / 2.0;
            let o = mid(a)
                .partial_cmp(&mid(b))
                .unwrap_or(std::cmp::Ordering::Equal);
            if far_is_high { o.reverse() } else { o }
        });
        for i in order {
            let b = &model.boxes[i];
            let (x, y, bw, bh) = rect(b);
            let _ = writeln!(
                out,
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"#222\" stroke-width=\"{}\"/>",
                fmt(x),
                fmt(y),
                fmt(bw),
                fmt(bh),
                hex(b.material),
                fmt(sw)
            );
        }
    };
    // Top: looking down, the high boxes are near, so the low ones are drawn first.
    view(
        &mut out,
        "TOP",
        (fx + ts * 0.3, ty - ts * 0.9),
        2,
        false,
        &|b| {
            (
                fx + b.min[0] - lo[0],
                ty + hi[1] - b.max[1],
                b.max[0] - b.min[0],
                b.max[1] - b.min[1],
            )
        },
    );
    // Front: looking from the front (small y), so the high y is far.
    view(
        &mut out,
        "FRONT",
        (fx + ts * 0.3, fy - ts * 0.9),
        1,
        true,
        &|b| {
            (
                fx + b.min[0] - lo[0],
                fy + hi[2] - b.max[2],
                b.max[0] - b.min[0],
                b.max[2] - b.min[2],
            )
        },
    );
    // Left: looking from the left (small x); the front of the part is on the
    // right, next to the front view.
    view(
        &mut out,
        "LEFT",
        (lx + ts * 0.3, fy - ts * 0.9),
        0,
        true,
        &|b| {
            (
                lx + hi[1] - b.max[1],
                fy + hi[2] - b.max[2],
                b.max[1] - b.min[1],
                b.max[2] - b.min[2],
            )
        },
    );

    // The board surface (z = 0) in the front and left views.
    if lo[2] <= 0.0 && hi[2] > 0.0 {
        for (x0, len) in [(fx, w), (lx, d)] {
            let y = fy + hi[2];
            let _ = writeln!(
                out,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#2a7\" stroke-width=\"{}\" stroke-dasharray=\"{} {} {} {}\"/>",
                fmt(x0 - ts * 0.5),
                fmt(y),
                fmt(x0 + len + ts * 0.5),
                fmt(y),
                fmt(sw),
                fmt(ts * 1.2),
                fmt(ts * 0.3),
                fmt(ts * 0.2),
                fmt(ts * 0.3)
            );
        }
    }

    // The overall size.
    let right = fx + w + ts * 2.8;
    let dims = [
        // Width, under the front view.
        Dimension {
            horizontal: true,
            at: fy + h + ts * 2.6,
            from: fx,
            to: fx + w,
            reach: fy + h,
            value: w,
        },
        // Depth, at the right of the top view.
        Dimension {
            horizontal: false,
            at: right,
            from: ty,
            to: ty + d,
            reach: fx + w,
            value: d,
        },
        // Height, at the right of the front view.
        Dimension {
            horizontal: false,
            at: right,
            from: fy,
            to: fy + h,
            reach: fx + w,
            value: h,
        },
    ];
    for dim in &dims {
        let arrow = ts * 0.6;
        // A point across and along the line, as x and y.
        let pt = |across: f64, along: f64| {
            if dim.horizontal {
                (along, across)
            } else {
                (across, along)
            }
        };
        let line = |out: &mut String, a: (f64, f64), b: (f64, f64), width: f64| {
            let _ = writeln!(
                out,
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{blue}\" stroke-width=\"{}\"/>",
                fmt(a.0),
                fmt(a.1),
                fmt(b.0),
                fmt(b.1),
                fmt(width)
            );
        };
        let beyond = dim.at + ts * 0.4 * (dim.at - dim.reach).signum();
        for along in [dim.from, dim.to] {
            line(&mut out, pt(dim.reach, along), pt(beyond, along), sw * 0.8);
        }
        line(&mut out, pt(dim.at, dim.from), pt(dim.at, dim.to), sw);
        // Arrows pointing outwards at both ends.
        for (along, dir) in [(dim.from, 1.0), (dim.to, -1.0)] {
            let tip = pt(dim.at, along);
            let base = pt(dim.at, along + dir * arrow);
            let side = arrow * 0.3;
            let (a, b) = if dim.horizontal {
                ((base.0, base.1 - side), (base.0, base.1 + side))
            } else {
                ((base.0 - side, base.1), (base.0 + side, base.1))
            };
            let _ = writeln!(
                out,
                "<path d=\"M{} {} L{} {} L{} {} Z\" fill=\"{blue}\"/>",
                fmt(tip.0),
                fmt(tip.1),
                fmt(a.0),
                fmt(a.1),
                fmt(b.0),
                fmt(b.1)
            );
        }
        let label = fmt(dim.value);
        let mid = (dim.from + dim.to) / 2.0;
        if dim.horizontal {
            let _ = writeln!(
                out,
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" fill=\"{blue}\">{label}</text>",
                fmt(mid),
                fmt(dim.at - ts * 0.35),
                fmt(ts)
            );
        } else {
            let (x, y) = (dim.at - ts * 0.35, mid);
            let _ = writeln!(
                out,
                "<text x=\"{}\" y=\"{}\" font-size=\"{}\" text-anchor=\"middle\" fill=\"{blue}\" transform=\"rotate(-90 {} {})\">{label}</text>",
                fmt(x),
                fmt(y),
                fmt(ts),
                fmt(x),
                fmt(y)
            );
        }
    }
    out.push_str("</svg>\n");
    out
}
