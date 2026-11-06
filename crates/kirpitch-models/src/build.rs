// SPDX-License-Identifier: GPL-3.0-or-later

//! From a datasheet's text to a PartSpec: runs the tasks with the configured
//! models and assembles the result.

use std::collections::BTreeMap;
use std::sync::Arc;

use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{
    DualRowGullWing, DualRowThroughHole, QuadFlatNoLead, Template, dual_row_gull_wing,
    dual_row_through_hole, quad_flat_no_lead, sot23,
};
use kirpitch_core::{BoundingBox, Confidence, Mm, Source, Sourced, Tolerance};

use crate::client::Client;
use crate::config::{Config, Task};
use crate::keys::{KeyStore, OsKeyStore};
use crate::tasks::{
    Cache, CallRecord, Ctx, MetadataAnswer, PackageAnswer, PageLines, PinsResult, Span, Stats,
    TaskError, extract_metadata, extract_package, extract_pins,
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
pub fn template_from_answer(a: &PackageAnswer) -> Result<Template, String> {
    let range = |s: Option<Span>, name: &str| {
        s.map(|s| Range::new(s.min, s.max))
            .ok_or_else(|| format!("`{name}` is missing"))
    };
    let made = match a.family.as_str() {
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
    default_page: u32,
    options: &BuildOptions,
) -> Result<(PartSpec, Vec<String>), String> {
    let mut template = template_from_answer(package)?;
    let mut notes = Vec::new();
    let page = package.page.unwrap_or(default_page);
    let package_source = source(page, [0.0, 0.0, 1.0, 1.0], package.confidence);

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
    for dimension in template.package.dimensions.values_mut() {
        dimension.source = Some(package_source);
    }
    template.land_pattern.source = Some(package_source);
    for pad in &mut template.land_pattern.pads {
        pad.source = Some(package_source);
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
    report
        .models
        .push((task, name.to_string(), config.stays_local(task)));
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
    };

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
            extract_metadata(&c, cache, &mut stats, pages, &ctx)
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
            pages,
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
            extract_package(&c, cache, &mut stats, pages, hint, &ctx).map_err(|e| e.to_string())
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

    if let (Some(m), Some(p), Some(k)) = (&metadata, &report.pins, &package) {
        note(
            on,
            "assemble",
            StageState::Started,
            "checking the pieces fit",
        );
        let default_page = pages.first().map_or(1, |p| p.page);
        match assemble(m, p, k, default_page, options) {
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
