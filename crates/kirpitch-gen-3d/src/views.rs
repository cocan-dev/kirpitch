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
