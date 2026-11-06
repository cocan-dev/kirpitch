// SPDX-License-Identifier: GPL-3.0-or-later

//! STEP (ISO 10303-21, application protocol 214) writer.
//!
//! Every box is written as a manifold solid with planar faces. The same model
//! always gives the same bytes (the time stamp is fixed).

use std::fmt::Write;

use crate::vrml::{corners, dot, faces, num, sub};
use crate::{Cuboid, Material, Model};

struct Data {
    lines: Vec<String>,
}

impl Data {
    /// Adds an entity and returns its number.
    fn add(&mut self, text: impl Into<String>) -> usize {
        self.lines.push(text.into());
        self.lines.len()
    }
}

fn refs(ids: &[usize]) -> String {
    ids.iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn text(name: &str) -> String {
    name.replace('\'', "''")
}

fn point(d: &mut Data, p: [f64; 3]) -> usize {
    d.add(format!(
        "CARTESIAN_POINT('',({},{},{}))",
        num(p[0]),
        num(p[1]),
        num(p[2])
    ))
}

fn direction(d: &mut Data, v: [f64; 3]) -> usize {
    d.add(format!(
        "DIRECTION('',({},{},{}))",
        num(v[0]),
        num(v[1]),
        num(v[2])
    ))
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let n = dot(v, v).sqrt();
    [v[0] / n, v[1] / n, v[2] / n]
}

/// Writes one box as a solid and returns the solid's number.
fn solid(d: &mut Data, b: &Cuboid) -> usize {
    let c = corners(b);
    let pts: Vec<usize> = c.iter().map(|p| point(d, *p)).collect();
    let verts: Vec<usize> = pts
        .iter()
        .map(|p| d.add(format!("VERTEX_POINT('',#{p})")))
        .collect();
    // Edges between corners that differ in one coordinate, from the lower
    // to the higher corner number.
    let mut edges = std::collections::BTreeMap::new();
    for lo in 0..8usize {
        for bit in [1usize, 2, 4] {
            if lo & bit == 0 {
                let hi = lo | bit;
                let v = sub(c[hi], c[lo]);
                let length = dot(v, v).sqrt();
                let dir = direction(d, unit(v));
                let vec = d.add(format!("VECTOR('',#{dir},{})", num(length)));
                let line = d.add(format!("LINE('',#{},#{vec})", pts[lo]));
                let edge = d.add(format!(
                    "EDGE_CURVE('',#{},#{},#{line},.T.)",
                    verts[lo], verts[hi]
                ));
                edges.insert((lo, hi), edge);
            }
        }
    }
    let mut face_ids = Vec::new();
    for (quad, normal) in faces() {
        let mut oriented = Vec::new();
        for k in 0..4 {
            let (u, v) = (quad[k], quad[(k + 1) % 4]);
            let (key, forward) = if u < v {
                ((u, v), true)
            } else {
                ((v, u), false)
            };
            let sense = if forward { ".T." } else { ".F." };
            oriented.push(d.add(format!("ORIENTED_EDGE('',*,*,#{},{sense})", edges[&key])));
        }
        let lp = d.add(format!("EDGE_LOOP('',({}))", refs(&oriented)));
        let bound = d.add(format!("FACE_OUTER_BOUND('',#{lp},.T.)"));
        let origin = point(d, c[quad[0]]);
        let axis = direction(d, normal);
        let first = unit(sub(c[quad[1]], c[quad[0]]));
        let ref_dir = direction(d, first);
        let placement = d.add(format!(
            "AXIS2_PLACEMENT_3D('',#{origin},#{axis},#{ref_dir})"
        ));
        let plane = d.add(format!("PLANE('',#{placement})"));
        face_ids.push(d.add(format!("ADVANCED_FACE('',(#{bound}),#{plane},.T.)")));
    }
    let shell = d.add(format!("CLOSED_SHELL('',({}))", refs(&face_ids)));
    d.add(format!("MANIFOLD_SOLID_BREP('',#{shell})"))
}

/// Writes the model as a STEP file.
pub fn write_step(model: &Model) -> String {
    let mut d = Data { lines: Vec::new() };
    let name = text(&model.name);
    let app = d.add("APPLICATION_CONTEXT('core data for automotive mechanical design processes')");
    d.add(format!(
        "APPLICATION_PROTOCOL_DEFINITION('international standard','automotive_design',2000,#{app})"
    ));
    let pc = d.add(format!("PRODUCT_CONTEXT('',#{app},'mechanical')"));
    let product = d.add(format!("PRODUCT('{name}','{name}','',(#{pc}))"));
    d.add(format!(
        "PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#{product}))"
    ));
    let pdc = d.add(format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')"
    ));
    let pdf = d.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{product})"));
    let pd = d.add(format!("PRODUCT_DEFINITION('design','',#{pdf},#{pdc})"));
    let pds = d.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{pd})"));

    let lu = d.add("(LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.))");
    let au = d.add("(NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.))");
    let su = d.add("(NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT())");
    let unc = d.add(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-05),#{lu},'distance_accuracy_value','confusion accuracy')"
    ));
    let ctx = d.add(format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3) GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{unc})) GLOBAL_UNIT_ASSIGNED_CONTEXT((#{lu},#{au},#{su})) REPRESENTATION_CONTEXT('{name}','3D'))"
    ));

    // One colour style per material.
    let mut styles = std::collections::BTreeMap::new();
    for (key, material) in [Material::Body, Material::Marking, Material::Metal]
        .into_iter()
        .enumerate()
    {
        if model.boxes.iter().all(|b| b.material != material) {
            continue;
        }
        let (r, g, b) = material.rgb();
        let rgb = d.add(format!("COLOUR_RGB('',{},{},{})", num(r), num(g), num(b)));
        let fsc = d.add(format!("FILL_AREA_STYLE_COLOUR('',#{rgb})"));
        let fas = d.add(format!("FILL_AREA_STYLE('',(#{fsc}))"));
        let ssfa = d.add(format!("SURFACE_STYLE_FILL_AREA(#{fas})"));
        let sss = d.add(format!("SURFACE_SIDE_STYLE('',(#{ssfa}))"));
        let ssu = d.add(format!("SURFACE_STYLE_USAGE(.BOTH.,#{sss})"));
        let psa = d.add(format!("PRESENTATION_STYLE_ASSIGNMENT((#{ssu}))"));
        styles.insert(key, psa);
    }

    let origin = point(&mut d, [0.0; 3]);
    let z = direction(&mut d, [0.0, 0.0, 1.0]);
    let x = direction(&mut d, [1.0, 0.0, 0.0]);
    let axis = d.add(format!("AXIS2_PLACEMENT_3D('',#{origin},#{z},#{x})"));
    let mut items = vec![axis];
    let mut styled = Vec::new();
    for b in &model.boxes {
        let s = solid(&mut d, b);
        items.push(s);
        let key = [Material::Body, Material::Marking, Material::Metal]
            .iter()
            .position(|m| *m == b.material)
            .unwrap_or(0);
        styled.push(d.add(format!("STYLED_ITEM('color',(#{}),#{s})", styles[&key])));
    }
    let rep = d.add(format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('{name}',({}),#{ctx})",
        refs(&items)
    ));
    d.add(format!("SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})"));
    d.add(format!(
        "MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('',({}),#{ctx})",
        refs(&styled)
    ));

    let mut out = String::new();
    out.push_str("ISO-10303-21;\nHEADER;\n");
    out.push_str("FILE_DESCRIPTION(('3D model made by kirpitch'),'2;1');\n");
    let _ = writeln!(
        out,
        "FILE_NAME('{name}.step','1970-01-01T00:00:00',('kirpitch'),(''),'kirpitch','kirpitch','');"
    );
    out.push_str("FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));\nENDSEC;\nDATA;\n");
    for (i, line) in d.lines.iter().enumerate() {
        let _ = writeln!(out, "#{}={};", i + 1, line);
    }
    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    out
}
