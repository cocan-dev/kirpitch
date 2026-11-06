// SPDX-License-Identifier: GPL-3.0-or-later

//! The review flow without a language model: build a PartSpec from a package
//! template and a pin table, look at each stage, edit it and approve it.
//!
//! ```text
//! kirpitch template dual-row spec.json --mpn ... --pins 8 ...   package + land pattern
//! kirpitch pins spec.json --import table.csv                    pin table
//! kirpitch status spec.json                                      where each gate stands
//! kirpitch review spec.json --stage pinout [--edit]              show / edit a stage
//! kirpitch approve spec.json --stage pinout                      approve a stage
//! ```
//!
//! Editing an approved stage (also by hand in the JSON file) re-opens it and
//! all later stages (see `kirpitch_core::gates`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Args, Subcommand, ValueEnum};
use kirpitch_core::gates::{ApprovalError, Gate, GateStatus, approve, stage_findings};
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{
    Approvals, LandPattern, Metadata, Origin, Package, PartSpec, Pin, SCHEMA_VERSION,
};
use kirpitch_core::pintable::{ImportOptions, parse_pin_table};
use kirpitch_core::templates::{
    DualRowGullWing, DualRowThroughHole, QuadFlatNoLead, Template, dual_row_gull_wing,
    dual_row_through_hole, quad_flat_no_lead, sot23,
};
use kirpitch_core::{BoundingBox, Source, validate};
use kirpitch_extract::{DEFAULT_COLUMN_GAP, PdfReader, table_from_lines};
use serde::{Deserialize, Serialize};

use crate::{Failure, load, print_findings};

/// The findings of the PartSpec checks and of the 3D fit check.
fn all_findings(spec: &PartSpec) -> Vec<kirpitch_core::Finding> {
    let mut findings = validate(spec, &crate::fab());
    findings.extend(kirpitch_gen_3d::fit_findings(spec));
    findings
}

/// A stage on the command line.
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum StageArg {
    /// Gate 1: pin numbers, names and types
    Pinout,
    /// Gate 2: package dimensions and pads
    Footprint,
    /// Gate 3: the 3D model on the footprint
    #[value(name = "3d")]
    Model3d,
}

impl StageArg {
    fn gate(self) -> Gate {
        match self {
            StageArg::Pinout => Gate::Pinout,
            StageArg::Footprint => Gate::Footprint,
            StageArg::Model3d => Gate::Model3d,
        }
    }
}

/// How much solder margin the pads get (IPC-7351 density level).
#[derive(Clone, Copy, ValueEnum)]
pub enum DensityArg {
    /// Level C, smallest pads
    Least,
    /// Level B, the usual choice
    Nominal,
    /// Level A, largest pads
    Most,
}

impl From<DensityArg> for Density {
    fn from(d: DensityArg) -> Density {
        match d {
            DensityArg::Least => Density::Least,
            DensityArg::Nominal => Density::Nominal,
            DensityArg::Most => Density::Most,
        }
    }
}

/// A range such as `5.8..6.2`, or a single value `6.4`.
#[derive(Clone, Copy)]
pub struct RangeArg(Range);

fn parse_range(text: &str) -> Result<RangeArg, String> {
    let number = |s: &str| {
        s.trim()
            .parse::<f64>()
            .map_err(|_| format!("`{s}` is not a number; use `min..max` or one value"))
    };
    match text.split_once("..") {
        Some((a, b)) => {
            let (a, b) = (number(a)?, number(b)?);
            if a > b {
                return Err(format!(
                    "`{text}`: the first value must not exceed the second"
                ));
            }
            Ok(RangeArg(Range::new(a, b)))
        }
        None => Ok(RangeArg(Range::exact(number(text)?))),
    }
}

/// Part identity for a new PartSpec.
#[derive(Args)]
pub struct MetadataArgs {
    /// Manufacturer part number (required when the file does not exist yet)
    #[arg(long)]
    mpn: Option<String>,
    /// Manufacturer (required when the file does not exist yet)
    #[arg(long)]
    manufacturer: Option<String>,
    /// One-line description (required when the file does not exist yet)
    #[arg(long)]
    description: Option<String>,
    /// Search keywords, separated by commas
    #[arg(long, value_delimiter = ',')]
    keywords: Vec<String>,
    /// URL of the datasheet
    #[arg(long)]
    datasheet_url: Option<String>,
}

/// Dimensions of a gull-wing package (JEDEC letters in brackets).
#[derive(Args)]
pub struct GullWingArgs {
    /// Family name used in the footprint name
    #[arg(long, default_value = "SOIC")]
    family: String,
    /// Number of pins
    #[arg(long)]
    pins: u32,
    /// Pitch (e)
    #[arg(long)]
    pitch: f64,
    /// Body length along the pin rows (D)
    #[arg(long)]
    body_length: f64,
    /// Body width across the rows (E1)
    #[arg(long)]
    body_width: f64,
    /// Largest height (A)
    #[arg(long)]
    height: f64,
    /// Overall width over the leads (E), as `min..max` or one value
    #[arg(long, value_parser = parse_range)]
    span: RangeArg,
    /// Foot length (L), as `min..max`
    #[arg(long, value_parser = parse_range)]
    lead_length: RangeArg,
    /// Lead width (b), as `min..max`
    #[arg(long, value_parser = parse_range)]
    lead_width: RangeArg,
    /// IPC-7351 density level
    #[arg(long, value_enum, default_value = "nominal")]
    density: DensityArg,
}

/// Dimensions of a through-hole DIP.
#[derive(Args)]
pub struct DipArgs {
    /// Family name used in the footprint name
    #[arg(long, default_value = "DIP")]
    family: String,
    /// Number of pins
    #[arg(long)]
    pins: u32,
    /// Pitch (e)
    #[arg(long, default_value_t = 2.54)]
    pitch: f64,
    /// Distance between the rows (7.62 for 300 mil)
    #[arg(long, default_value_t = 7.62)]
    row_spacing: f64,
    /// Body length along the rows (D)
    #[arg(long)]
    body_length: f64,
    /// Body width (E1)
    #[arg(long)]
    body_width: f64,
    /// Largest height (A)
    #[arg(long)]
    height: f64,
    /// Pad diameter
    #[arg(long, default_value_t = 1.6)]
    pad_diameter: f64,
    /// Drill diameter
    #[arg(long, default_value_t = 0.8)]
    drill: f64,
}

/// Dimensions of a QFN (JEDEC letters in brackets).
#[derive(Args)]
pub struct QfnArgs {
    /// Family name used in the footprint name
    #[arg(long, default_value = "QFN")]
    family: String,
    /// Terminals on each of the top and bottom sides
    #[arg(long)]
    pins_x: u32,
    /// Terminals on each of the left and right sides
    #[arg(long)]
    pins_y: u32,
    /// Pitch (e)
    #[arg(long)]
    pitch: f64,
    /// Body size along x (D), as `min..max` or one value
    #[arg(long, value_parser = parse_range)]
    body_x: RangeArg,
    /// Body size along y (E), as `min..max` or one value
    #[arg(long, value_parser = parse_range)]
    body_y: RangeArg,
    /// Largest height (A)
    #[arg(long)]
    height: f64,
    /// Terminal length (L), as `min..max`
    #[arg(long, value_parser = parse_range)]
    lead_length: RangeArg,
    /// Terminal width (b), as `min..max`
    #[arg(long, value_parser = parse_range)]
    lead_width: RangeArg,
    /// Exposed pad size as `width,height` in millimetres; leave out when there is none
    #[arg(long, value_parser = parse_pair)]
    exposed_pad: Option<(f64, f64)>,
    /// IPC-7351 density level
    #[arg(long, value_enum, default_value = "nominal")]
    density: DensityArg,
}

fn parse_pair(text: &str) -> Result<(f64, f64), String> {
    let (a, b) = text
        .split_once(',')
        .ok_or_else(|| format!("`{text}` is not `width,height`"))?;
    let n = |s: &str| {
        s.trim()
            .parse::<f64>()
            .map_err(|_| format!("`{s}` is not a number"))
    };
    Ok((n(a)?, n(b)?))
}

/// The package kinds with a template.
#[derive(Subcommand)]
pub enum TemplateKind {
    /// Two rows of gull-wing leads: SOIC, SOP, SSOP, TSSOP, MSOP
    DualRow(GullWingArgs),
    /// SOT-23 with 3, 5 or 6 leads
    Sot23(GullWingArgs),
    /// Through-hole dual in-line package
    Dip(DipArgs),
    /// Quad flat no-lead package (QFN) with an optional exposed pad
    Qfn(QfnArgs),
}

/// `kirpitch template`
#[derive(Args)]
pub struct TemplateArgs {
    /// The PartSpec file; created if missing, otherwise only its package and
    /// land pattern are replaced
    spec: PathBuf,
    #[command(flatten)]
    metadata: MetadataArgs,
    #[command(subcommand)]
    kind: TemplateKind,
}

/// `kirpitch pins`
#[derive(Args)]
pub struct PinsArgs {
    /// The PartSpec file
    spec: PathBuf,
    /// Pin table to read: CSV, tab separated, or pasted text
    #[arg(long, conflicts_with_all = ["pdf", "region"])]
    import: Option<PathBuf>,
    /// Datasheet PDF to read the pin table from (needs --page and --region and
    /// the pdfium library, see KIRPITCH_PDFIUM_LIB_DIR)
    #[arg(long, requires_all = ["page", "region"])]
    pdf: Option<PathBuf>,
    /// Datasheet page of the pin table, recorded as the source of every pin
    #[arg(long)]
    page: Option<u32>,
    /// Region of the page holding the table, as fractions from the top-left
    /// corner: `left,top,right,bottom`, for example `0.1,0.2,0.9,0.5`
    #[arg(long, value_parser = parse_region)]
    region: Option<RegionArg>,
}

/// A region of a page.
#[derive(Clone, Copy)]
pub struct RegionArg(BoundingBox);

fn parse_region(text: &str) -> Result<RegionArg, String> {
    let numbers: Result<Vec<f64>, _> = text.split(',').map(|s| s.trim().parse::<f64>()).collect();
    let numbers =
        numbers.map_err(|_| format!("`{text}` is not four numbers separated by commas"))?;
    let [l, t, r, b] = numbers[..] else {
        return Err("a region needs four numbers: left,top,right,bottom".into());
    };
    let bbox = BoundingBox([l, t, r, b]);
    if bbox.is_valid() {
        Ok(RegionArg(bbox))
    } else {
        Err("the numbers must be between 0 and 1, with left <= right and top <= bottom".into())
    }
}

/// `kirpitch status`
#[derive(Args)]
pub struct StatusArgs {
    /// The PartSpec file
    spec: PathBuf,
}

/// `kirpitch review`
#[derive(Args)]
pub struct ReviewArgs {
    /// The PartSpec file
    spec: PathBuf,
    /// The stage to look at
    #[arg(long, value_enum)]
    stage: StageArg,
    /// Open the values of the stage in $EDITOR (or $VISUAL) and save the result
    #[arg(long)]
    edit: bool,
}

/// `kirpitch approve`
#[derive(Args)]
pub struct ApproveArgs {
    /// The PartSpec file
    spec: PathBuf,
    /// The stage to approve
    #[arg(long, value_enum)]
    stage: StageArg,
    /// A short note saved with the approval
    #[arg(long)]
    note: Option<String>,
}

fn io_error(path: &Path, e: impl std::fmt::Display) -> Failure {
    Failure::Message(format!("{}: {e}", path.display()))
}

fn save(path: &Path, spec: &PartSpec) -> Result<(), Failure> {
    fs::write(path, spec.to_json_string().as_bytes()).map_err(|e| io_error(path, e))
}

fn flow_err(e: impl std::fmt::Display) -> Failure {
    Failure::Message(e.to_string())
}

// ----------------------------------------------------------------- template

pub fn template(args: TemplateArgs, out: &mut dyn Write) -> Result<(), Failure> {
    let made = match &args.kind {
        TemplateKind::DualRow(a) => {
            dual_row_gull_wing(&gull_wing_input(a), a.density.into()).map_err(flow_err)?
        }
        TemplateKind::Sot23(a) => sot23(&gull_wing_input(a), a.density.into()).map_err(flow_err)?,
        TemplateKind::Qfn(a) => quad_flat_no_lead(
            &QuadFlatNoLead {
                family: a.family.clone(),
                pins_x: a.pins_x,
                pins_y: a.pins_y,
                pitch: a.pitch,
                body_x: a.body_x.0,
                body_y: a.body_y.0,
                height_max: a.height,
                lead_length: a.lead_length.0,
                lead_width: a.lead_width.0,
                exposed_pad: a.exposed_pad,
            },
            a.density.into(),
        )
        .map_err(flow_err)?,
        TemplateKind::Dip(a) => dual_row_through_hole(&DualRowThroughHole {
            family: a.family.clone(),
            pin_count: a.pins,
            pitch: a.pitch,
            row_spacing: a.row_spacing,
            body_length: a.body_length,
            body_width: a.body_width,
            height_max: a.height,
            pad_diameter: a.pad_diameter,
            drill: a.drill,
        })
        .map_err(flow_err)?,
    };
    let spec = if args.spec.exists() {
        let mut spec = load(&args.spec)?;
        apply_template(&mut spec, made);
        spec
    } else {
        let m = &args.metadata;
        let need = |v: &Option<String>, name: &str| {
            v.clone().ok_or_else(|| {
                Failure::Message(format!(
                    "{} does not exist yet; give --{name} to create it",
                    args.spec.display()
                ))
            })
        };
        let mut spec = PartSpec {
            schema_version: SCHEMA_VERSION,
            origin: Origin::Hand,
            metadata: Metadata {
                mpn: need(&m.mpn, "mpn")?,
                manufacturer: need(&m.manufacturer, "manufacturer")?,
                description: need(&m.description, "description")?,
                keywords: m.keywords.clone(),
                datasheet_url: m.datasheet_url.clone(),
                datasheet_sha256: None,
            },
            package: made.package.clone(),
            pins: Vec::new(),
            land_pattern: made.land_pattern.clone(),
            approvals: Approvals::default(),
            overrides: Vec::new(),
        };
        apply_template(&mut spec, made);
        spec
    };
    save(&args.spec, &spec)?;
    writeln!(
        out,
        "wrote {} ({}, {} pads)",
        args.spec.display(),
        spec.package.name,
        spec.land_pattern.pads.len()
    )
    .map_err(flow_err)?;
    Ok(())
}

fn gull_wing_input(a: &GullWingArgs) -> DualRowGullWing {
    DualRowGullWing {
        family: a.family.clone(),
        pin_count: a.pins,
        pitch: a.pitch,
        body_length: a.body_length,
        body_width: a.body_width,
        height_max: a.height,
        lead_span: a.span.0,
        lead_length: a.lead_length.0,
        lead_width: a.lead_width.0,
    }
}

fn apply_template(spec: &mut PartSpec, made: Template) {
    spec.package = made.package;
    spec.land_pattern = made.land_pattern;
}

// --------------------------------------------------------------------- pins

pub fn pins(args: PinsArgs, out: &mut dyn Write, err: &mut dyn Write) -> Result<(), Failure> {
    let mut spec = load(&args.spec)?;
    let (text, source, origin) = match (&args.import, &args.pdf) {
        (Some(path), None) => (
            fs::read_to_string(path).map_err(|e| io_error(path, e))?,
            args.page.map(|page| Source {
                page,
                bbox: None,
                confidence: None,
            }),
            path.display().to_string(),
        ),
        (None, Some(pdf)) => {
            let (Some(page), Some(region)) = (args.page, args.region) else {
                return Err(Failure::Message("--pdf needs --page and --region".into()));
            };
            let bytes = fs::read(pdf).map_err(|e| io_error(pdf, e))?;
            let dir = std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR").map(PathBuf::from);
            let reader = PdfReader::new(dir.as_deref()).map_err(flow_err)?;
            let lines = reader
                .lines_in_region(&bytes, page, region.0)
                .map_err(flow_err)?;
            let table = table_from_lines(&lines, DEFAULT_COLUMN_GAP);
            (
                table.to_tsv(),
                Some(Source {
                    page,
                    bbox: Some(region.0),
                    confidence: None,
                }),
                format!("{} page {page}", pdf.display()),
            )
        }
        _ => {
            return Err(Failure::Message(
                "give --import FILE, or --pdf FILE with --page and --region".into(),
            ));
        }
    };
    let result = parse_pin_table(&text, &ImportOptions { source })
        .map_err(|e| Failure::Message(format!("{origin}: {e}")))?;
    for w in &result.warnings {
        writeln!(err, "warning: {w}").map_err(flow_err)?;
    }
    spec.pins = result.pins;
    save(&args.spec, &spec)?;
    writeln!(
        out,
        "read {} pins into {}",
        spec.pins.len(),
        args.spec.display()
    )
    .map_err(flow_err)?;
    Ok(())
}

// ------------------------------------------------------------------- status

fn status_word(s: GateStatus) -> &'static str {
    match s {
        GateStatus::Open => "open",
        GateStatus::Approved => "approved",
        GateStatus::Stale => "open again (changed since it was approved)",
    }
}

pub fn status(args: StatusArgs, out: &mut dyn Write) -> Result<(), Failure> {
    let spec = load(&args.spec)?;
    let findings = all_findings(&spec);
    for gate in Gate::ALL {
        let mine = stage_findings(gate, &findings);
        let errors = mine
            .iter()
            .filter(|f| f.level == kirpitch_core::Level::Error)
            .count();
        writeln!(
            out,
            "{:<10} {} ({} error(s), {} warning(s))",
            gate.name(),
            status_word(gate.status(&spec)),
            errors,
            mine.len() - errors
        )
        .map_err(flow_err)?;
    }
    Ok(())
}

// ------------------------------------------------------------------- review

pub fn review(args: ReviewArgs, out: &mut dyn Write) -> Result<(), Failure> {
    let gate = args.stage.gate();
    let mut spec = load(&args.spec)?;
    if args.edit {
        edit_stage(&args.spec, &mut spec, gate)?;
    }
    writeln!(
        out,
        "Stage: {} ({})",
        gate.name(),
        status_word(gate.status(&spec))
    )
    .map_err(flow_err)?;
    match gate {
        Gate::Pinout => print_pinout(&spec, out),
        Gate::Footprint => print_footprint(&spec, out),
        Gate::Model3d => print_model3d(&spec, out),
    }
    .map_err(flow_err)?;
    let findings = all_findings(&spec);
    let mine: Vec<_> = stage_findings(gate, &findings)
        .into_iter()
        .cloned()
        .collect();
    writeln!(out).map_err(flow_err)?;
    print_findings(&mine, out).map_err(flow_err)?;
    Ok(())
}

/// Confidence below this is marked in the tables.
const LOW_CONFIDENCE: f64 = 0.8;

fn source_text(source: &Option<Source>) -> String {
    match source {
        None => "no source".to_string(),
        Some(s) => {
            let mut t = format!("p.{}", s.page);
            if let Some(c) = s.confidence {
                t.push_str(&format!(" {:.0}%", c.get() * 100.0));
                if c.get() < LOW_CONFIDENCE {
                    t.push_str(" !LOW");
                }
            }
            t
        }
    }
}

fn print_pinout(spec: &PartSpec, out: &mut dyn Write) -> std::io::Result<()> {
    writeln!(
        out,
        "{} pins (package says {})",
        spec.pins.len(),
        spec.package.pin_count
    )?;
    writeln!(
        out,
        "{:<6} {:<14} {:<15} {:<14} source",
        "pin", "name", "type", "group"
    )?;
    for p in &spec.pins {
        writeln!(
            out,
            "{:<6} {:<14} {:<15} {:<14} {}",
            p.number,
            p.name,
            p.electrical_type.as_str(),
            p.group.as_deref().unwrap_or("-"),
            source_text(&p.source)
        )?;
    }
    Ok(())
}

fn print_footprint(spec: &PartSpec, out: &mut dyn Write) -> std::io::Result<()> {
    let p = &spec.package;
    writeln!(
        out,
        "{}  pitch {} mm  body {} x {} mm (D x E1)  height <= {} mm",
        p.name,
        p.pitch.format(),
        p.body.length.format(),
        p.body.width.format(),
        p.body.height_max.format()
    )?;
    for (key, d) in &p.dimensions {
        let t = d.value;
        let range = match (t.min, t.max) {
            (Some(lo), Some(hi)) => format!("{} .. {}", lo.format(), hi.format()),
            (Some(lo), None) => format!("{} ..", lo.format()),
            (None, Some(hi)) => format!(".. {}", hi.format()),
            (None, None) => String::new(),
        };
        writeln!(
            out,
            "  {:<4} {:>8} mm  {:<16} {}",
            key,
            t.nom.format(),
            range,
            source_text(&d.source)
        )?;
    }
    writeln!(
        out,
        "{:<6} {:<10} {:>9} {:>9} {:>8} {:>8} {:>6}  source",
        "pad", "shape", "x", "y", "size x", "size y", "drill"
    )?;
    for pad in &spec.land_pattern.pads {
        writeln!(
            out,
            "{:<6} {:<10} {:>9} {:>9} {:>8} {:>8} {:>6}  {}",
            pad.number,
            format!("{:?}", pad.shape).to_lowercase(),
            pad.x.format(),
            pad.y.format(),
            pad.size_x.format(),
            pad.size_y.format(),
            pad.drill.map_or("-".to_string(), |d| d.format()),
            source_text(&pad.source)
        )?;
    }
    Ok(())
}

fn print_model3d(spec: &PartSpec, out: &mut dyn Write) -> std::io::Result<()> {
    let model = match kirpitch_gen_3d::build(spec) {
        Ok(m) => m,
        Err(e) => return writeln!(out, "no 3D model: {e}"),
    };
    let (lo, hi) = model.bounds();
    let body = &model.boxes[0];
    let size = body.size();
    writeln!(out, "{}: {} parts", model.name, model.boxes.len())?;
    writeln!(
        out,
        "  body      {:.2} x {:.2} x {:.2} mm, from {:.2} to {:.2} mm above the board",
        size[0], size[1], size[2], body.min[2], body.max[2]
    )?;
    writeln!(
        out,
        "  overall   {:.2} x {:.2} mm, {:.2} mm high (limit {} mm), lowest point {:.2} mm",
        hi[0] - lo[0],
        hi[1] - lo[1],
        hi[2],
        spec.package.body.height_max.format(),
        lo[2]
    )?;
    writeln!(
        out,
        "  The model is built from the package and the pads; to change it, edit the footprint stage."
    )
}

/// The part of a PartSpec that the footprint stage edits.
#[derive(Serialize, Deserialize)]
struct FootprintSlice {
    package: Package,
    land_pattern: LandPattern,
}

/// What `--edit` opens in the editor, as JSON text.
fn stage_json(spec: &PartSpec, gate: Gate) -> String {
    let text = match gate {
        Gate::Pinout => serde_json::to_string_pretty(&spec.pins),
        Gate::Footprint => serde_json::to_string_pretty(&FootprintSlice {
            package: spec.package.clone(),
            land_pattern: spec.land_pattern.clone(),
        }),
        Gate::Model3d => Ok("null".to_string()),
    };
    let mut text = text.expect("serialisation cannot fail");
    text.push('\n');
    text
}

/// Puts edited stage JSON back into the PartSpec.
fn apply_stage_json(spec: &mut PartSpec, gate: Gate, text: &str) -> Result<(), String> {
    match gate {
        Gate::Pinout => {
            spec.pins = serde_json::from_str::<Vec<Pin>>(text).map_err(|e| e.to_string())?;
        }
        Gate::Model3d => {
            return Err("the 3D model has no values of its own; edit the footprint stage".into());
        }
        Gate::Footprint => {
            let slice: FootprintSlice = serde_json::from_str(text).map_err(|e| e.to_string())?;
            spec.package = slice.package;
            spec.land_pattern = slice.land_pattern;
        }
    }
    Ok(())
}

fn edit_stage(path: &Path, spec: &mut PartSpec, gate: Gate) -> Result<(), Failure> {
    if gate == Gate::Model3d {
        return Err(Failure::Message(
            "the 3D model has no values of its own; edit the footprint stage".into(),
        ));
    }
    let tmp = std::env::temp_dir().join(format!(
        "kirpitch-{}-{}.json",
        gate.name(),
        std::process::id()
    ));
    fs::write(&tmp, stage_json(spec, gate)).map_err(|e| io_error(&tmp, e))?;
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| if cfg!(windows) { "notepad" } else { "vi" }.to_string());
    let mut parts = editor.split_whitespace();
    let program = parts.next().unwrap_or("vi");
    let status = Process::new(program)
        .args(parts)
        .arg(&tmp)
        .status()
        .map_err(|e| Failure::Message(format!("cannot start the editor `{editor}`: {e}")))?;
    let edited = fs::read_to_string(&tmp).map_err(|e| io_error(&tmp, e));
    let _ = fs::remove_file(&tmp);
    if !status.success() {
        return Err(Failure::Message(format!(
            "the editor `{editor}` failed; nothing was changed"
        )));
    }
    apply_stage_json(spec, gate, &edited?).map_err(|e| {
        Failure::Message(format!(
            "the edited values are not valid, nothing was saved: {e}"
        ))
    })?;
    save(path, spec)
}

// ------------------------------------------------------------------ approve

pub fn approve_cmd(
    args: ApproveArgs,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), Failure> {
    let gate = args.stage.gate();
    let mut spec = load(&args.spec)?;
    let findings = all_findings(&spec);
    match approve(&mut spec, gate, &findings, args.note, now_rfc3339()) {
        Ok(()) => {
            save(&args.spec, &spec)?;
            writeln!(out, "approved {}", gate.name()).map_err(flow_err)?;
            let warnings: Vec<_> = stage_findings(gate, &findings)
                .into_iter()
                .filter(|f| f.overridden_by.is_none())
                .cloned()
                .collect();
            if !warnings.is_empty() {
                writeln!(out, "open warnings:").map_err(flow_err)?;
                print_findings(&warnings, out).map_err(flow_err)?;
            }
            Ok(())
        }
        Err(ApprovalError::Errors { errors, .. }) => {
            writeln!(err, "cannot approve {}: fix these first", gate.name()).map_err(flow_err)?;
            print_findings(&errors, err).map_err(flow_err)?;
            Err(Failure::Invalid)
        }
        Err(e @ ApprovalError::EarlierStageOpen(_)) => {
            writeln!(err, "cannot approve {}: {e}", gate.name()).map_err(flow_err)?;
            Err(Failure::Invalid)
        }
    }
}

/// The current time as `2026-11-06T12:00:00Z`.
pub fn now_rfc3339() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format_utc(secs)
}

/// Formats seconds since 1970-01-01 as an RFC 3339 UTC time.
pub fn format_utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Civil date from days since 1970-01-01 (proleptic Gregorian calendar).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_formatting() {
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(format_utc(1_793_966_400), "2026-11-06T12:00:00Z");
        assert_eq!(format_utc(86_399), "1970-01-01T23:59:59Z");
    }

    const FIXTURE: &str = include_str!("../../kirpitch-core/tests/fixtures/soic8_example.json");

    #[test]
    fn stage_values_round_trip_through_the_editor_text() {
        let mut spec = PartSpec::from_json_str(FIXTURE).unwrap();
        for gate in [Gate::Pinout, Gate::Footprint] {
            let before = spec.clone();
            let text = stage_json(&spec, gate);
            apply_stage_json(&mut spec, gate, &text).unwrap();
            assert_eq!(spec, before);
        }
        // The 3D model has nothing to edit.
        assert!(apply_stage_json(&mut spec, Gate::Model3d, "null").is_err());
        // An edited pin name is taken over, other stages are untouched.
        let text = stage_json(&spec, Gate::Pinout).replace("OUT1", "OUT_A");
        apply_stage_json(&mut spec, Gate::Pinout, &text).unwrap();
        assert_eq!(spec.pins[0].name, "OUT_A");
    }

    #[test]
    fn broken_editor_text_is_refused_and_changes_nothing() {
        let mut spec = PartSpec::from_json_str(FIXTURE).unwrap();
        let before = spec.clone();
        assert!(apply_stage_json(&mut spec, Gate::Pinout, "[ not json").is_err());
        assert!(apply_stage_json(&mut spec, Gate::Footprint, "{\"package\": 1}").is_err());
        assert_eq!(spec, before);
    }

    #[test]
    fn ranges_on_the_command_line() {
        assert_eq!(parse_range("5.8..6.2").unwrap().0, Range::new(5.8, 6.2));
        assert_eq!(parse_range("6.4").unwrap().0, Range::exact(6.4));
        assert!(parse_range("6.2..5.8").is_err());
        assert!(parse_range("abc").is_err());
    }
}
