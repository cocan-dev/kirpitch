// SPDX-License-Identifier: GPL-3.0-or-later

//! From a datasheet's text to a PartSpec: runs the tasks with the configured
//! models and assembles the result.

use std::collections::BTreeMap;
use std::sync::Arc;

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{
    Approvals, Metadata, Origin, PadShape, PartSpec, Pin, SCHEMA_VERSION,
};
use kirpitch_core::templates::{
    CustomPads, DualRowGullWing, DualRowThroughHole, GridArray, PadGroup, QuadFlatGullWing,
    QuadFlatNoLead, Template, custom_pads, dual_row_gull_wing, dual_row_through_hole, grid_array,
    quad_flat_gull_wing, quad_flat_no_lead, sot23,
};
use kirpitch_core::{BoundingBox, Confidence, Mm, Source, Sourced, Tolerance};

use crate::client::Client;
use crate::config::{Config, Task};
use crate::keys::{KeyStore, OsKeyStore};
use crate::tasks::{
    Cache, CallRecord, Ctx, FootprintAnswer, GRID_FAMILIES, MetadataAnswer, PackageAnswer,
    PageLines, Pinned, PinsResult, QUAD_FAMILIES, Span, Stats, TaskError, extract_footprint,
    extract_metadata, extract_package, extract_pins,
};
use crate::transport::Transport;

/// Options of a build.
#[derive(Debug, Clone, Default)]
pub struct BuildOptions {
    /// Part number to use instead of reading it.
    pub mpn: Option<String>,
    /// Manufacturer to use instead of reading it.
    pub manufacturer: Option<String>,
    /// Description to use instead of reading it.
    pub description: Option<String>,
    /// A hint for the models, for example "the pinout is on page 4".
    pub hint: Option<String>,
    /// Read the pin table twice and flag differences.
    pub second_read: bool,
    /// SHA-256 of the datasheet PDF.
    pub pdf_sha256: Option<String>,
    /// Where the datasheet came from.
    pub datasheet_url: Option<String>,
    /// Text the person pointed at, for each step: the step reads exactly this
    /// instead of searching the datasheet.
    pub given: GivenText,
}

/// The text of the places the person pointed at.
#[derive(Debug, Clone, Default)]
pub struct GivenText {
    /// For the part number, maker and description.
    pub metadata: Option<Vec<PageLines>>,
    /// For the pin table.
    pub pins: Option<Vec<PageLines>>,
    /// For the package drawing.
    pub package: Option<Vec<PageLines>>,
    /// For the recommended footprint.
    pub footprint: Option<Vec<PageLines>>,
}

/// What a build produced.
#[derive(Debug, Clone)]
pub struct BuildReport {
    /// The PartSpec, when pins and package could both be read.
    pub spec: Option<PartSpec>,
    /// The pins, when they could be read (also when the package could not).
    pub pins: Option<PinsResult>,
    /// Things to look at in review, one line each.
    pub notes: Vec<String>,
    /// Stages that failed, with the reason; the manual flow can finish them.
    pub failures: Vec<String>,
    /// Totals over all calls.
    pub stats: Stats,
    /// For each task, the model that did it and whether data stayed local.
    pub models: Vec<(Task, String, bool)>,
}

/// Turns a package answer into a template.
pub fn template_from_answer(
    a: &PackageAnswer,
    footprint: Option<&FootprintAnswer>,
) -> Result<Template, String> {
    let range = |s: Option<Span>, name: &str| {
        s.map(|s| Range::new(s.min, s.max))
            .ok_or_else(|| format!("`{name}` is missing"))
    };
    let made = match a.family.as_str() {
        f if QUAD_FAMILIES.contains(&f) => quad_flat_gull_wing(
            &QuadFlatGullWing {
                family: f.to_ascii_uppercase(),
                pins_x: a.pins_x.ok_or("`pins_x` is missing")?,
                pins_y: a.pins_y.ok_or("`pins_y` is missing")?,
                pitch: a.pitch,
                body_x: range(a.body_x, "body_x")?,
                body_y: range(a.body_y, "body_y")?,
                span_x: range(a.span_x, "span_x")?,
                span_y: range(a.span_y, "span_y")?,
                height_max: a.height_max,
                lead_length: range(a.lead_length, "lead_length")?,
                lead_width: range(a.lead_width, "lead_width")?,
                exposed_pad: a.exposed_pad,
            },
            Density::Nominal,
        ),
        f if GRID_FAMILIES.contains(&f) => grid_array(
            &GridArray {
                family: f.to_ascii_uppercase(),
                rows: a.rows.ok_or("`rows` is missing")?,
                cols: a.cols.ok_or("`cols` is missing")?,
                pitch: a.pitch,
                body_x: range(a.body_x, "body_x")?,
                body_y: range(a.body_y, "body_y")?,
                height_max: a.height_max,
                ball_diameter: range(a.lead_width, "ball diameter")?,
                standoff: a.standoff,
            },
            Density::Nominal,
        ),
        "custom" => {
            let name = a.package_name.clone().unwrap_or_else(|| "PKG".into());
            let groups: Vec<PadGroup> = footprint
                .map(|f| f.groups.as_slice())
                .unwrap_or_default()
                .iter()
                .map(|g| PadGroup {
                    first_number: g.first_number,
                    count: g.count,
                    x: g.x,
                    y: g.y,
                    dx: g.dx,
                    dy: g.dy,
                    size_x: g.size_x,
                    size_y: g.size_y,
                    shape: match g.shape.as_str() {
                        "roundrect" => PadShape::RoundRect,
                        "oval" => PadShape::Oval,
                        "circle" => PadShape::Circle,
                        _ => PadShape::Rect,
                    },
                    drill: g.drill,
                })
                .collect();
            if groups.is_empty() {
                return Err(format!(
                    "the package `{name}` has no template of its own and the recommended footprint gave no pads (groups) to draw it from"
                ));
            }
            custom_pads(&CustomPads {
                family: name,
                body_length: a.body_length.ok_or("`body_length` is missing")?,
                body_width: a.body_width.ok_or("`body_width` is missing")?,
                height_max: a.height_max,
                groups,
            })
        }
        "qfn" => quad_flat_no_lead(
            &QuadFlatNoLead {
                family: "QFN".into(),
                pins_x: a.pins_x.ok_or("`pins_x` is missing")?,
                pins_y: a.pins_y.ok_or("`pins_y` is missing")?,
                pitch: a.pitch,
                body_x: range(a.body_x, "body_x")?,
                body_y: range(a.body_y, "body_y")?,
                height_max: a.height_max,
                lead_length: range(a.lead_length, "lead_length")?,
                lead_width: range(a.lead_width, "lead_width")?,
                exposed_pad: a.exposed_pad,
            },
            Density::Nominal,
        ),
        "dip" => dual_row_through_hole(&DualRowThroughHole {
            family: "DIP".into(),
            pin_count: a.pin_count.ok_or("`pin_count` is missing")?,
            pitch: a.pitch,
            row_spacing: a.row_spacing.ok_or("`row_spacing` is missing")?,
            body_length: a.body_length.ok_or("`body_length` is missing")?,
            body_width: a.body_width.ok_or("`body_width` is missing")?,
            height_max: a.height_max,
            pad_diameter: a.pad_diameter.unwrap_or(1.6),
            drill: a.drill.unwrap_or(0.8),
        }),
        family => {
            let input = DualRowGullWing {
                family: if family == "sot23" {
                    "SOT-23".to_string()
                } else {
                    family.to_ascii_uppercase()
                },
                pin_count: a.pin_count.ok_or("`pin_count` is missing")?,
                pitch: a.pitch,
                body_length: a.body_length.ok_or("`body_length` is missing")?,
                body_width: a.body_width.ok_or("`body_width` is missing")?,
                height_max: a.height_max,
                lead_span: range(a.lead_span, "lead_span")?,
                lead_length: range(a.lead_length, "lead_length")?,
                lead_width: range(a.lead_width, "lead_width")?,
            };
            if family == "sot23" {
                sot23(&input, Density::Nominal)
            } else {
                dual_row_gull_wing(&input, Density::Nominal)
            }
        }
    };
    made.map_err(|e| e.to_string())
}

fn source(page: u32, bbox: [f64; 4], confidence: f64) -> Source {
    Source {
        page,
        bbox: Some(BoundingBox(bbox)),
        confidence: Confidence::new(confidence.clamp(0.0, 1.0)).ok(),
    }
}

/// Assembles the PartSpec from what was read. Every value gets a source: the
/// pins the line they were found on, the package values the page of the
/// package drawing (the whole page, since no smaller region was found).
pub fn assemble(
    metadata: &MetadataAnswer,
    pins: &PinsResult,
    package: &PackageAnswer,
    footprint: Option<&FootprintAnswer>,
    default_page: u32,
    options: &BuildOptions,
) -> Result<(PartSpec, Vec<String>), String> {
    let mut template = template_from_answer(package, footprint)?;
    let mut notes = Vec::new();
    let page = package.page.unwrap_or(default_page);
    let package_source = source(page, [0.0, 0.0, 1.0, 1.0], package.confidence);
    // The pads: the recommended footprint of the datasheet wins over the
    // calculated land pattern.
    let mut land_source = package_source;
    if let Some(f) = footprint
        && package.family != "custom"
    {
        let changes = apply_footprint(&mut template, f);
        if !changes.is_empty() {
            land_source = source(f.page.unwrap_or(page), [0.0, 0.0, 1.0, 1.0], f.confidence);
            notes.extend(changes);
        }
    } else if footprint.is_some() {
        land_source = source(
            footprint.and_then(|f| f.page).unwrap_or(page),
            [0.0, 0.0, 1.0, 1.0],
            footprint.map_or(0.7, |f| f.confidence),
        );
    }
    // A ball grid has a pad for every position; the pin table says which balls
    // the package really has.
    if crate::tasks::GRID_FAMILIES.contains(&package.family.as_str()) {
        let names: std::collections::BTreeSet<&str> =
            pins.pins.iter().map(|p| p.pin.number.as_str()).collect();
        let before = template.land_pattern.pads.len();
        template
            .land_pattern
            .pads
            .retain(|p| names.contains(p.number.as_str()));
        let removed = before - template.land_pattern.pads.len();
        if removed > 0 {
            notes.push(format!(
                "{removed} of the {before} positions of the ball grid are not in the pin table and have no pad (check the ball map)"
            ));
        }
        template.package.pin_count = template.land_pattern.pads.len() as u32;
    }

    let mut list: Vec<Pin> = pins
        .pins
        .iter()
        .map(|p| Pin {
            number: p.pin.number.clone(),
            name: p.pin.name.clone(),
            electrical_type: p.pin.electrical_type,
            group: None,
            alt_functions: vec![],
            source: Some(source(p.page, p.bbox, p.confidence)),
        })
        .collect();
    let want = template.package.pin_count as usize;
    if list.len() + 1 == want
        && template
            .land_pattern
            .pads
            .iter()
            .any(|p| p.number == want.to_string())
    {
        // The table does not list the exposed pad.
        list.push(Pin {
            number: want.to_string(),
            name: "EP".into(),
            electrical_type: ElectricalType::Passive,
            group: None,
            alt_functions: vec![],
            source: Some(package_source),
        });
        notes.push(format!(
            "the exposed pad (pin {want}, `EP`) is not in the pin table; it was added as passive (check what it connects to)"
        ));
    }
    if list.len() != want {
        return Err(format!(
            "the pin table has {} pins but the package has {want}",
            list.len()
        ));
    }

    // The body sizes keep the range of the drawing (the template only knows the
    // rounded size it needs for the outline and the name).
    for (key, range) in [
        ("D", package.body_length_range),
        ("E1", package.body_width_range),
    ] {
        if let Some(s) = range
            && s.min < s.max
            && let (Ok(min), Ok(max), Ok(nom)) = (
                Mm::new(s.min),
                Mm::new(s.max),
                Mm::new((s.min + s.max) / 2.0),
            )
        {
            template.package.dimensions.insert(
                key.to_string(),
                Sourced::unsourced(Tolerance {
                    min: Some(min),
                    nom,
                    max: Some(max),
                }),
            );
        }
    }
    template.package.source = Some(package_source);
    // The outline drawing's other dimensions are kept: the 3D model uses them.
    for (key, value) in [
        ("A1", package.standoff),
        ("A2", package.body_thickness),
        ("c", package.lead_thickness),
    ] {
        if let Some(v) = value
            && let Ok(mm) = Mm::new(v)
        {
            template.package.dimensions.insert(
                key.to_string(),
                Sourced::unsourced(Tolerance {
                    min: None,
                    nom: mm,
                    max: None,
                }),
            );
        }
    }
    for dimension in template.package.dimensions.values_mut() {
        dimension.source = Some(package_source);
    }
    template.land_pattern.source = Some(land_source);
    for pad in &mut template.land_pattern.pads {
        pad.source = Some(land_source);
    }
    let pick =
        |given: &Option<String>, read: &str| given.clone().unwrap_or_else(|| read.to_string());
    let spec = PartSpec {
        schema_version: SCHEMA_VERSION,
        origin: Origin::Llm,
        metadata: Metadata {
            mpn: pick(&options.mpn, &metadata.mpn),
            manufacturer: pick(&options.manufacturer, &metadata.manufacturer),
            description: pick(&options.description, &metadata.description),
            keywords: vec![],
            datasheet_url: options.datasheet_url.clone(),
            datasheet_sha256: options.pdf_sha256.clone(),
        },
        package: template.package,
        pins: list,
        land_pattern: template.land_pattern,
        approvals: Approvals::default(),
        overrides: vec![],
    };
    Ok((spec, notes))
}

fn model_client(
    config: &Config,
    task: Task,
    transport: &Arc<dyn Transport>,
    keys: &dyn KeyStore,
    report: &mut BuildReport,
) -> Result<Client, String> {
    let (name, model) = config.model_for(task).map_err(|e| e.to_string())?;
    let client =
        Client::connect(name, model, transport.clone(), keys).map_err(|e| e.to_string())?;
    if !report
        .models
        .iter()
        .any(|(t, n, _)| *t == task && n == name)
    {
        report
            .models
            .push((task, name.to_string(), config.stays_local(task)));
    }
    Ok(client)
}

/// How far a stage of a build is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StageState {
    /// Started; the model is working on it.
    Started,
    /// Finished.
    Done,
    /// Did not work; the others go on.
    Failed,
    /// Not needed.
    Skipped,
    /// A report on a call to a model (the `call` of the report says more).
    Call,
}

/// A report on the progress of a build, for a screen that shows it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Progress {
    /// `metadata`, `pins`, `package` or `assemble`.
    pub stage: &'static str,
    /// How far it is.
    pub state: StageState,
    /// A line for the person: which model is asked, what was found, why it
    /// failed.
    pub detail: String,
    /// The call to a model, for reports with the state `Call`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<CallRecord>,
}

fn note(on: &dyn Fn(&Progress), stage: &'static str, state: StageState, detail: impl Into<String>) {
    on(&Progress {
        stage,
        state,
        detail: detail.into(),
        call: None,
    });
}

fn asking(config: &Config, task: Task) -> String {
    match config.model_for(task) {
        Ok((name, _)) => format!("asking {name}"),
        Err(_) => "no model".to_string(),
    }
}

/// Puts the values of the recommended footprint of the datasheet over the
/// calculated pads: the size of the pads, the distance between the rows (from
/// centres or outer edges), the round lands of a ball grid and the pad under
/// the body. Numbering and the other positions stay as the template made them.
/// Returns the notes to show: what was replaced, and what does not agree.
pub fn apply_footprint(template: &mut Template, fp: &FootprintAnswer) -> Vec<String> {
    let mut notes = Vec::new();
    let before: Vec<(f64, f64, f64, f64)> = template
        .land_pattern
        .pads
        .iter()
        .map(|p| (p.x.get(), p.y.get(), p.size_x.get(), p.size_y.get()))
        .collect();
    let family = template.package.family.to_ascii_uppercase();
    let grid = ["BGA", "LGA", "CSP"].iter().any(|f| family.contains(f));
    let mm = |v: f64| Mm::new(v).ok();
    let is_center = |p: &kirpitch_core::partspec::Pad| {
        p.x.get().abs() < 0.01
            && p.y.get().abs() < 0.01
            && p.size_x.get() >= 1.0
            && p.size_y.get() >= 1.0
    };
    let mut changed = false;
    if grid {
        if let Some(d) = fp.pad_diameter.and_then(mm) {
            for pad in &mut template.land_pattern.pads {
                pad.size_x = d;
                pad.size_y = d;
            }
            changed = true;
        }
    } else if let (Some(len), Some(wid)) = (fp.pad_length, fp.pad_width) {
        // The distance of the rows from the centres, or from the outer edges.
        let cx = fp.center_x.or(fp.outer_x.map(|o| o - len));
        let cy = fp.center_y.or(fp.outer_y.map(|o| o - len));
        let n = template.land_pattern.pads.len();
        for pad in &mut template.land_pattern.pads {
            if n > 2 && is_center(pad) {
                continue; // the pad under the body
            }
            let (sx, sy) = (pad.size_x.get(), pad.size_y.get());
            let sides = if (sx - sy).abs() > 1e-9 {
                sx > sy
            } else {
                pad.x.get().abs() >= pad.y.get().abs()
            };
            let (Some(l), Some(w)) = (mm(len), mm(wid)) else {
                continue;
            };
            if sides {
                pad.size_x = l;
                pad.size_y = w;
                if let Some(c) = cx.and_then(|c| mm(pad.x.get().signum() * c / 2.0)) {
                    pad.x = c;
                }
            } else {
                pad.size_y = l;
                pad.size_x = w;
                if let Some(c) = cy.or(cx).and_then(|c| mm(pad.y.get().signum() * c / 2.0)) {
                    pad.y = c;
                }
            }
            changed = true;
        }
    }
    if let Some((w, h)) = fp.exposed_pad {
        let (Some(mw), Some(mh)) = (mm(w), mm(h)) else {
            return notes;
        };
        if let Some(pad) = template.land_pattern.pads.iter_mut().find(|p| is_center(p)) {
            pad.size_x = mw;
            pad.size_y = mh;
            changed = true;
        } else if !grid && let (Some(zero), true) = (mm(0.0), template.land_pattern.pads.len() > 2)
        {
            // The datasheet draws a pad under the body that the outline did not
            // list: add it after the last terminal.
            let number = (template.package.pin_count + 1).to_string();
            template
                .land_pattern
                .pads
                .push(kirpitch_core::partspec::Pad {
                    number,
                    shape: PadShape::Rect,
                    x: zero,
                    y: zero,
                    size_x: mw,
                    size_y: mh,
                    rotation_deg: 0.0,
                    corner_ratio: None,
                    paste: true,
                    role: kirpitch_core::partspec::PadRole::Signal,
                    kind: kirpitch_core::partspec::PadKind::Smd,
                    drill: None,
                    source: None,
                });
            template.package.pin_count += 1;
            notes.push(format!(
                "the recommended footprint has a pad of {w} x {h} mm under the body that the package drawing did not list; it was added as pin {}",
                template.package.pin_count
            ));
            changed = true;
        }
    }
    if let Some(pitch) = fp.pitch
        && (pitch - template.package.pitch.get()).abs() > 0.01
    {
        notes.push(format!(
            "the recommended footprint has a pitch of {pitch} mm but the package drawing says {} mm",
            template.package.pitch.get()
        ));
    }
    if changed {
        // How far the datasheet's pads are from the calculated ones.
        let mut worst = 0.0f64;
        for (pad, old) in template.land_pattern.pads.iter().zip(&before) {
            worst = worst
                .max((pad.x.get() - old.0).abs())
                .max((pad.y.get() - old.1).abs())
                .max((pad.size_x.get() - old.2).abs())
                .max((pad.size_y.get() - old.3).abs());
        }
        notes.insert(
            0,
            format!(
                "the pads follow the recommended footprint of the datasheet; they differ from the calculated pads by up to {worst:.2} mm{}",
                if worst > 0.25 {
                    " (more than usual: check the footprint against the drawing)"
                } else {
                    ""
                }
            ),
        );
    }
    notes
}

/// Runs the whole extraction: metadata, pins and package. A stage that fails
/// is listed in `failures` and does not stop the others, so the review flow
/// can finish it by hand.
pub fn build(
    config: &Config,
    transport: Arc<dyn Transport>,
    cache: &Cache,
    pages: &[PageLines],
    options: &BuildOptions,
) -> BuildReport {
    build_with_keys(config, transport, cache, pages, options, &OsKeyStore)
}

/// Like [`build`], with the place where keys are looked up given (tests give
/// their own).
pub fn build_with_keys(
    config: &Config,
    transport: Arc<dyn Transport>,
    cache: &Cache,
    pages: &[PageLines],
    options: &BuildOptions,
    keys: &dyn KeyStore,
) -> BuildReport {
    build_with_progress(config, transport, cache, pages, options, keys, &|_| {})
}

/// Like [`build_with_keys`], and tells `on` when each stage starts and ends.
pub fn build_with_progress(
    config: &Config,
    transport: Arc<dyn Transport>,
    cache: &Cache,
    pages: &[PageLines],
    options: &BuildOptions,
    keys: &dyn KeyStore,
    on: &dyn Fn(&Progress),
) -> BuildReport {
    let mut report = BuildReport {
        spec: None,
        pins: None,
        notes: vec![],
        failures: vec![],
        stats: Stats::default(),
        models: vec![],
    };
    let mut stats = Stats::default();
    let hint = options.hint.as_deref();
    let obs = |c: &CallRecord| {
        let stage = match c.label.split(' ').next().unwrap_or("") {
            "metadata" => "metadata",
            "package" => "package",
            "footprint" => "footprint",
            _ => "pins",
        };
        on(&Progress {
            stage,
            state: StageState::Call,
            detail: String::new(),
            call: Some(c.clone()),
        });
    };
    let ctx = Ctx {
        prompts: &config.prompts,
        options: &config.extraction,
        obs: &obs,
        raw: None,
        pinned: Pinned {
            metadata: options.given.metadata.is_some(),
            pins: options.given.pins.is_some(),
            package: options.given.package.is_some(),
            footprint: options.given.footprint.is_some(),
        },
    };
    let metadata_pages = options.given.metadata.as_deref().unwrap_or(pages);
    let pin_pages = options.given.pins.as_deref().unwrap_or(pages);
    let package_pages = options.given.package.as_deref().unwrap_or(pages);
    let footprint_pages = options.given.footprint.as_deref().unwrap_or(pages);

    let metadata = if options.mpn.is_some()
        && options.manufacturer.is_some()
        && options.description.is_some()
    {
        note(on, "metadata", StageState::Skipped, "given");
        Some(MetadataAnswer {
            mpn: options.mpn.clone().unwrap_or_default(),
            manufacturer: options.manufacturer.clone().unwrap_or_default(),
            description: options.description.clone().unwrap_or_default(),
        })
    } else {
        note(
            on,
            "metadata",
            StageState::Started,
            asking(config, Task::Metadata),
        );
        match model_client(config, Task::Metadata, &transport, keys, &mut report).and_then(|c| {
            extract_metadata(&c, cache, &mut stats, metadata_pages, &ctx)
                .map_err(|e: TaskError| e.to_string())
        }) {
            Ok(m) => {
                note(
                    on,
                    "metadata",
                    StageState::Done,
                    format!("{}, {}", m.mpn, m.manufacturer),
                );
                Some(m)
            }
            Err(e) => {
                note(on, "metadata", StageState::Failed, e.clone());
                report.failures.push(format!("metadata: {e}"));
                None
            }
        }
    };

    note(
        on,
        "pins",
        StageState::Started,
        format!(
            "{}{}",
            asking(config, Task::PinTables),
            if options.second_read { ", twice" } else { "" }
        ),
    );
    match model_client(config, Task::PinTables, &transport, keys, &mut report).and_then(|c| {
        extract_pins(
            &c,
            cache,
            &mut stats,
            pin_pages,
            hint,
            options.second_read,
            &ctx,
        )
        .map_err(|e| e.to_string())
    }) {
        Ok(p) => {
            note(
                on,
                "pins",
                StageState::Done,
                format!("{} pins", p.pins.len()),
            );
            report.notes.extend(p.notes.iter().cloned());
            report.pins = Some(p);
        }
        Err(e) => {
            note(on, "pins", StageState::Failed, e.clone());
            report.failures.push(format!("pins: {e}"));
        }
    }

    note(
        on,
        "package",
        StageState::Started,
        asking(config, Task::Drawings),
    );

    let package = match model_client(config, Task::Drawings, &transport, keys, &mut report)
        .and_then(|c| {
            extract_package(&c, cache, &mut stats, package_pages, hint, &ctx)
                .map_err(|e| e.to_string())
        }) {
        Ok(p) => {
            note(on, "package", StageState::Done, "dimensions read");
            Some(p)
        }
        Err(e) => {
            note(on, "package", StageState::Failed, e.clone());
            report.failures.push(format!("package: {e}"));
            None
        }
    };

    // The recommended footprint of the datasheet: asked for when the person
    // pointed at it or a page looks like one, and always for a package that
    // has no template of its own.
    let footprint = match &package {
        None => {
            note(on, "footprint", StageState::Skipped, "no package");
            None
        }
        Some(k) => {
            let custom = k.family == "custom";
            let looks_there = options.given.footprint.is_some()
                || !crate::tasks::suggest_pages(pages, "footprint", hint).is_empty();
            if !custom && !looks_there {
                note(
                    on,
                    "footprint",
                    StageState::Skipped,
                    "no recommended footprint found: the pads are calculated (IPC-7351)",
                );
                None
            } else {
                note(
                    on,
                    "footprint",
                    StageState::Started,
                    asking(config, Task::Drawings),
                );
                match model_client(config, Task::Drawings, &transport, keys, &mut report).and_then(
                    |c| {
                        extract_footprint(&c, cache, &mut stats, footprint_pages, hint, &ctx)
                            .map_err(|e| e.to_string())
                    },
                ) {
                    Ok(f) => {
                        note(on, "footprint", StageState::Done, "pads read");
                        Some(f)
                    }
                    Err(e) => {
                        note(on, "footprint", StageState::Failed, e.clone());
                        if custom {
                            report.failures.push(format!("footprint: {e}"));
                        } else {
                            report.notes.push(format!(
                                "the recommended footprint could not be read ({e}); the pads were calculated from the package dimensions"
                            ));
                        }
                        None
                    }
                }
            }
        }
    };

    if let (Some(m), Some(p), Some(k)) = (&metadata, &report.pins, &package) {
        note(
            on,
            "assemble",
            StageState::Started,
            "checking the pieces fit",
        );
        let default_page = pages.first().map_or(1, |p| p.page);
        match assemble(m, p, k, footprint.as_ref(), default_page, options) {
            Ok((spec, notes)) => {
                note(on, "assemble", StageState::Done, "pins and package agree");
                report.notes.extend(notes);
                report.spec = Some(spec);
            }
            Err(e) => {
                note(on, "assemble", StageState::Failed, e.to_string());
                report.failures.push(format!("assembly: {e}"));
            }
        }
    } else {
        note(
            on,
            "assemble",
            StageState::Skipped,
            "a stage before it failed",
        );
    }
    report.stats = stats;
    report
}

/// How well a build agrees with a known-good PartSpec, per review stage.
#[derive(Debug, Clone, PartialEq)]
pub struct Score {
    /// Pins whose number, name and type all match, out of the expected pins.
    pub pins_correct: usize,
    /// Pins expected.
    pub pins_expected: usize,
    /// Pins found that are not expected.
    pub pins_extra: usize,
    /// Package dimensions whose minimum, nominal-or-maximum and maximum match
    /// within 0.02 mm, out of those compared.
    pub dimensions_correct: usize,
    /// Dimensions compared.
    pub dimensions_expected: usize,
    /// Pads that match in place and size (0.01 mm), out of the expected pads.
    pub pads_correct: usize,
    /// Pads expected.
    pub pads_expected: usize,
    /// The dimensions that differ, one line each.
    pub mismatches: Vec<String>,
}

impl Score {
    /// Share of the expected pins that are right, 0 to 1.
    pub fn pinout(&self) -> f64 {
        ratio(self.pins_correct, self.pins_expected)
    }

    /// Share of the package dimensions that are right, 0 to 1. The pads are
    /// not part of it: they follow from the dimensions by the IPC-7351
    /// formulas, while a golden part may carry the maker's own land pattern.
    pub fn footprint(&self) -> f64 {
        ratio(self.dimensions_correct, self.dimensions_expected)
    }

    /// Share of the expected pads that are at the same place and size, 0 to 1.
    pub fn pads(&self) -> f64 {
        ratio(self.pads_correct, self.pads_expected)
    }
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

/// Compares a built PartSpec with the expected one.
pub fn score(built: &PartSpec, expected: &PartSpec) -> Score {
    let mut pins_correct = 0;
    for e in &expected.pins {
        if built.pins.iter().any(|b| {
            b.number == e.number && b.name == e.name && b.electrical_type == e.electrical_type
        }) {
            pins_correct += 1;
        }
    }
    let pins_extra = built
        .pins
        .iter()
        .filter(|b| !expected.pins.iter().any(|e| e.number == b.number))
        .count();
    let mut dimensions_correct = 0;
    let mut dimensions_expected = 0;
    let built_dims: &BTreeMap<_, _> = &built.package.dimensions;
    let mut mismatches = Vec::new();
    // Only the dimensions that both PartSpecs record are compared: the
    // templates keep the letters they need (A, D, E, E1, L, b, e), a maker may
    // list more (A1, c, ...).
    for (key, e) in &expected.package.dimensions {
        if let Some(b) = built_dims.get(key) {
            dimensions_expected += 1;
            let close = |x: Option<kirpitch_core::Mm>, y: Option<kirpitch_core::Mm>| match (x, y) {
                (Some(x), Some(y)) => (x.get() - y.get()).abs() <= 0.02,
                (None, None) => true,
                _ => false,
            };
            // A maker that prints only a nominal or a maximum: the built
            // range is a single value too, or its nominal is compared.
            let same = if e.value.min.is_none() && e.value.max.is_none() {
                (b.value.nom.get() - e.value.nom.get()).abs() <= 0.02
            } else if e.value.min.is_none() {
                close(b.value.max, e.value.max)
                    || (b.value.nom.get() - e.value.nom.get()).abs() <= 0.02
            } else {
                close(b.value.min, e.value.min) && close(b.value.max, e.value.max)
            };
            if same {
                dimensions_correct += 1;
            } else {
                let show = |t: &kirpitch_core::Tolerance| {
                    format!(
                        "{} / {} / {}",
                        t.min.map_or("-".into(), |v| v.format()),
                        t.nom.format(),
                        t.max.map_or("-".into(), |v| v.format())
                    )
                };
                mismatches.push(format!(
                    "{key}: built (min / nom / max) {}, expected {}",
                    show(&b.value),
                    show(&e.value)
                ));
            }
        }
    }
    let mut pads_correct = 0;
    for e in &expected.land_pattern.pads {
        if built.land_pattern.pads.iter().any(|b| {
            b.number == e.number
                && (b.x.get() - e.x.get()).abs() <= 0.01
                && (b.y.get() - e.y.get()).abs() <= 0.01
                && (b.size_x.get() - e.size_x.get()).abs() <= 0.01
                && (b.size_y.get() - e.size_y.get()).abs() <= 0.01
        }) {
            pads_correct += 1;
        }
    }
    Score {
        pins_correct,
        pins_expected: expected.pins.len(),
        pins_extra,
        dimensions_correct,
        dimensions_expected,
        pads_correct,
        pads_expected: expected.land_pattern.pads.len(),
        mismatches,
    }
}
