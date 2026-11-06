// SPDX-License-Identifier: GPL-3.0-or-later

//! VRML 2.0 writer.

use std::fmt::Write;

use crate::{Cuboid, Material, Model};

/// KiCad reads one VRML unit as 2.54 mm (0.1 inch), so lengths are divided by
/// this before they are written.
pub const VRML_UNIT_MM: f64 = 2.54;

/// The corners of a box, numbered `x + 2 * y + 4 * z` with each of x, y and z
/// 0 (minimum) or 1 (maximum).
pub(crate) fn corners(b: &Cuboid) -> [[f64; 3]; 8] {
    let mut out = [[0.0; 3]; 8];
    for (i, c) in out.iter_mut().enumerate() {
        *c = [
            if i & 1 == 0 { b.min[0] } else { b.max[0] },
            if i & 2 == 0 { b.min[1] } else { b.max[1] },
            if i & 4 == 0 { b.min[2] } else { b.max[2] },
        ];
    }
    out
}

/// The six faces of a box as corner numbers, in counter-clockwise order seen
/// from outside, with the outward normal.
pub(crate) fn faces() -> [([usize; 4], [f64; 3]); 6] {
    let raw: [([usize; 4], [f64; 3]); 6] = [
        ([0, 2, 6, 4], [-1.0, 0.0, 0.0]),
        ([1, 3, 7, 5], [1.0, 0.0, 0.0]),
        ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
        ([2, 3, 7, 6], [0.0, 1.0, 0.0]),
        ([0, 1, 3, 2], [0.0, 0.0, -1.0]),
        ([4, 5, 7, 6], [0.0, 0.0, 1.0]),
    ];
    // Turn each quad so that its right-hand normal points outward. Use a unit
    // box to decide, since the order does not depend on the size.
    let unit = corners(&Cuboid {
        min: [0.0; 3],
        max: [1.0; 3],
        material: Material::Body,
    });
    let mut out = raw;
    for (quad, normal) in &mut out {
        let p = |i: usize| unit[quad[i]];
        let a = sub(p(1), p(0));
        let b = sub(p(2), p(1));
        let n = cross(a, b);
        if dot(n, *normal) < 0.0 {
            quad.reverse();
        }
    }
    out
}

pub(crate) fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A number with at most 6 decimals and no trailing zeros.
pub(crate) fn num(v: f64) -> String {
    kirpitch_core::format_number(v)
}

/// Writes the model as VRML 2.0 with one shape per material.
pub fn write_vrml(model: &Model) -> String {
    let mut out = String::from("#VRML V2.0 utf8\n");
    let _ = writeln!(out, "# {} (made by kirpitch)", model.name);
    for material in [Material::Body, Material::Marking, Material::Metal] {
        let boxes: Vec<&Cuboid> = model
            .boxes
            .iter()
            .filter(|b| b.material == material)
            .collect();
        if boxes.is_empty() {
            continue;
        }
        let (r, g, b) = material.rgb();
        let specular = if material == Material::Metal {
            0.6
        } else {
            0.2
        };
        let _ = writeln!(out, "Shape {{");
        let _ = writeln!(out, "  appearance Appearance {{");
        let _ = writeln!(out, "    material Material {{");
        let _ = writeln!(out, "      diffuseColor {} {} {}", num(r), num(g), num(b));
        let _ = writeln!(
            out,
            "      specularColor {} {} {}",
            num(specular),
            num(specular),
            num(specular)
        );
        let _ = writeln!(out, "      ambientIntensity 0.2");
        let _ = writeln!(out, "      shininess 0.4");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "  }}");
        let _ = writeln!(out, "  geometry IndexedFaceSet {{");
        let _ = writeln!(out, "    solid TRUE");
        let _ = writeln!(out, "    coord Coordinate {{");
        let _ = writeln!(out, "      point [");
        for b in &boxes {
            for c in corners(b) {
                let _ = writeln!(
                    out,
                    "        {} {} {},",
                    num(c[0] / VRML_UNIT_MM),
                    num(c[1] / VRML_UNIT_MM),
                    num(c[2] / VRML_UNIT_MM)
                );
            }
        }
        let _ = writeln!(out, "      ]");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "    coordIndex [");
        for (n, _) in boxes.iter().enumerate() {
            for (quad, _) in faces() {
                let base = n * 8;
                let _ = writeln!(
                    out,
                    "      {}, {}, {}, {}, -1,",
                    base + quad[0],
                    base + quad[1],
                    base + quad[2],
                    base + quad[3]
                );
            }
        }
        let _ = writeln!(out, "    ]");
        let _ = writeln!(out, "  }}");
        let _ = writeln!(out, "}}");
    }
    out
}
