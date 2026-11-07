// SPDX-License-Identifier: GPL-3.0-or-later

//! The logic behind the Kirpitch desktop app.
//!
//! The window shows a PartSpec: its review gates, pins, pads, the findings of
//! the checks and a drawing of the footprint. Everything the window needs is
//! computed here as plain data ([`SpecView`]) so that it can be tested without
//! a window; `main.rs` only connects these functions to the web view.

mod eval;
mod guide;
mod parts;
mod settings;
mod steps;

pub use settings::{
    ExtractionValues, FindInput, ModelInput, ModelTest, ModelView, PromptView, ProviderInfo,
    SettingsInput, SettingsView, config_from_input, current_settings, delete_api_key,
    find_model_details, find_models, save_settings, set_api_key, settings_view, test_model,
    user_folders,
};

pub use eval::{
    CaseResult, CaseView, EvalEvent, EvalInput, EvalRun, EvalStatus, EvalTools, MyCase,
    ResolvedCase, Totals, UsedModel, add_case, delete_run, eval_dir, list_cases, list_cases_in,
    list_runs, remove_case, resolve_cases, run_eval, set_case_pdf,
};

pub use serde_json::Value as JsonValue;

pub use steps::{
    ConfirmedInput, ReadStepInput, build_confirmed, check_step_values, preview_footprint_values,
    read_step, unused_path,
};

pub use guide::{
    GuideInput, Preview, RegionInput, StepSize, Suggested, Suggestions, preview_selection,
    suggest_pages_for, text_of_regions,
};

pub use parts::{
    DatasheetInput, DatasheetOutcome, Example, NewPartInput, build_from_datasheet,
    build_from_datasheet_with_progress, build_with_transport, build_with_transport_progress,
    find_testdata, list_examples, new_part, open_example, read_pages, save_copy,
};

use std::fs;
use std::path::Path;

use kirpitch_core::gates::{ApprovalError, Gate, GateStatus, approve, stage_findings};
use kirpitch_core::partspec::{Pad, PadKind, PadShape, PartSpec};
use kirpitch_core::{FabProfile, Finding, Level, validate};
use serde::{Deserialize, Serialize};

/// One review gate.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GateView {
    /// `pinout` or `footprint`.
    pub name: String,
    /// `open`, `approved` or `stale` (changed since it was approved).
    pub status: String,
    /// Number of errors in this stage.
    pub errors: usize,
    /// Number of warnings in this stage.
    pub warnings: usize,
    /// Note and time of the approval, if any.
    pub approved_at: Option<String>,
    /// The note saved with the approval.
    pub note: Option<String>,
}

/// One pin as shown in the table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PinView {
    /// Pin number.
    pub number: String,
    /// Pin name.
    pub name: String,
    /// Electrical type.
    pub electrical_type: String,
    /// Group, if any.
    pub group: Option<String>,
    /// Where it was read: `p.3`, `p.3 85%` or `no source`.
    pub source: String,
    /// True when the confidence is below 80 percent.
    pub low_confidence: bool,
    /// Datasheet page of the source, counting from 1.
    pub page: Option<u32>,
    /// Region of the source on the page as fractions `[left, top, right,
    /// bottom]` from the top-left corner.
    pub region: Option<[f64; 4]>,
}

/// One pad as shown in the table (lengths in millimetres, canonical text).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PadView {
    /// Pad number.
    pub number: String,
    /// Shape name.
    pub shape: String,
    /// Centre x.
    pub x: String,
    /// Centre y.
    pub y: String,
    /// Size along x.
    pub size_x: String,
    /// Size along y.
    pub size_y: String,
    /// Drill, for through-hole pads.
    pub drill: Option<String>,
    /// Source text as for pins.
    pub source: String,
    /// Datasheet page of the source, counting from 1.
    pub page: Option<u32>,
    /// Region of the source on the page, as for pins.
    pub region: Option<[f64; 4]>,
}

/// A finding of the checks.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FindingView {
    /// `error` or `warning`.
    pub level: String,
    /// Check identifier.
    pub check: String,
    /// JSON path of the field to fix.
    pub path: String,
    /// What is wrong.
    pub message: String,
    /// The stage it belongs to.
    pub gate: String,
    /// The override note that accepts a warning, if any.
    pub overridden_by: Option<String>,
}

/// Everything the window shows about a PartSpec.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpecView {
    /// The file that was opened.
    pub path: String,
    /// Part number.
    pub mpn: String,
    /// Manufacturer.
    pub manufacturer: String,
    /// Description.
    pub description: String,
    /// Footprint name.
    pub package_name: String,
    /// Datasheet URL.
    pub datasheet_url: Option<String>,
    /// The names of the electrical types a pin can have, for the type menu.
    pub electrical_types: Vec<String>,
    /// The review gates in order.
    pub gates: Vec<GateView>,
    /// The pins.
    pub pins: Vec<PinView>,
    /// The pads.
    pub pads: Vec<PadView>,
    /// All findings.
    pub findings: Vec<FindingView>,
    /// A drawing of the footprint as SVG.
    pub footprint_svg: String,
    /// A drawing of the symbol as SVG; empty when the PartSpec has errors, so
    /// that no symbol can be made.
    pub symbol_svg: String,
    /// Flat views of the 3D model (top, front, side) as SVG; empty when no
    /// model can be built.
    pub model_views: Vec<ModelViewSvg>,
    /// One line about the size of the 3D model.
    pub model_summary: String,
}

/// One flat view of the 3D model.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ModelViewSvg {
    /// `top`, `front` or `side`.
    pub name: String,
    /// The drawing.
    pub svg: String,
}

/// The fab profile of this run: the file named in `KIRPITCH_FAB_PROFILE`, or
/// the generic profile (also when the file cannot be read).
fn fab() -> FabProfile {
    // The file named in the environment, else the one in the settings.
    if std::env::var_os(kirpitch_core::fab::ENV_FAB_PROFILE).is_some() {
        return FabProfile::from_env().unwrap_or_default();
    }
    user_folders()
        .fab_profile
        .and_then(|p| FabProfile::from_file(Path::new(&p)).ok())
        .unwrap_or_default()
}

/// Loads a PartSpec file.
pub fn load_spec(path: &Path) -> Result<PartSpec, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    PartSpec::from_json_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn source_text(source: &Option<kirpitch_core::Source>) -> (String, bool) {
    match source {
        None => ("no source".to_string(), false),
        Some(s) => {
            let mut text = format!("p.{}", s.page);
            let mut low = false;
            if let Some(c) = s.confidence {
                text.push_str(&format!(" {:.0}%", c.get() * 100.0));
                low = c.get() < 0.8;
            }
            (text, low)
        }
    }
}

fn status_name(s: GateStatus) -> &'static str {
    match s {
        GateStatus::Open => "open",
        GateStatus::Approved => "approved",
        GateStatus::Stale => "stale",
    }
}

/// Builds the view of a PartSpec.
pub fn view(path: &str, spec: &PartSpec) -> SpecView {
    let mut findings = validate(spec, &fab());
    findings.extend(kirpitch_gen_3d::fit_findings(spec));
    let (model_views, model_summary) = model_views(spec);
    let gates = Gate::ALL
        .iter()
        .map(|&gate| {
            let mine = stage_findings(gate, &findings);
            let errors = mine.iter().filter(|f| f.level == Level::Error).count();
            let approval = match gate {
                Gate::Pinout => spec.approvals.pinout.as_ref(),
                Gate::Footprint => spec.approvals.footprint.as_ref(),
                Gate::Model3d => spec.approvals.model_3d.as_ref(),
            };
            GateView {
                name: gate.name().to_string(),
                status: status_name(gate.status(spec)).to_string(),
                errors,
                warnings: mine.len() - errors,
                approved_at: approval.and_then(|a| a.at.clone()),
                note: approval.and_then(|a| a.note.clone()),
            }
        })
        .collect();
    SpecView {
        path: path.to_string(),
        mpn: spec.metadata.mpn.clone(),
        manufacturer: spec.metadata.manufacturer.clone(),
        description: spec.metadata.description.clone(),
        package_name: spec.package.name.clone(),
        datasheet_url: spec.metadata.datasheet_url.clone(),
        electrical_types: ELECTRICAL_TYPES
            .iter()
            .map(|t| t.as_str().to_string())
            .collect(),
        gates,
        pins: spec
            .pins
            .iter()
            .map(|p| {
                let (source, low_confidence) = source_text(&p.source);
                PinView {
                    number: p.number.clone(),
                    name: p.name.clone(),
                    electrical_type: p.electrical_type.as_str().to_string(),
                    group: p.group.clone(),
                    source,
                    low_confidence,
                    page: p.source.map(|s| s.page),
                    region: p.source.and_then(|s| s.bbox).map(|b| b.0),
                }
            })
            .collect(),
        pads: spec
            .land_pattern
            .pads
            .iter()
            .map(|p| PadView {
                number: p.number.clone(),
                shape: format!("{:?}", p.shape).to_lowercase(),
                x: p.x.format(),
                y: p.y.format(),
                size_x: p.size_x.format(),
                size_y: p.size_y.format(),
                drill: p.drill.map(|d| d.format()),
                source: source_text(&p.source).0,
                page: p.source.map(|s| s.page),
                region: p.source.and_then(|s| s.bbox).map(|b| b.0),
            })
            .collect(),
        findings: findings.iter().map(finding_view).collect(),
        footprint_svg: footprint_svg(spec),
        symbol_svg: symbol_svg(spec).unwrap_or_default(),
        model_views,
        model_summary,
    }
}

/// The flat views and a size line of the 3D model.
fn model_views(spec: &PartSpec) -> (Vec<ModelViewSvg>, String) {
    let Ok(model) = kirpitch_gen_3d::build(spec) else {
        return (Vec::new(), String::new());
    };
    let (lo, hi) = model.bounds();
    let views = kirpitch_gen_3d::View::ALL
        .iter()
        .map(|&v| ModelViewSvg {
            name: v.name().to_string(),
            svg: kirpitch_gen_3d::view_svg(&model, v),
        })
        .collect();
    let summary = format!(
        "{} parts, {:.2} x {:.2} mm, {:.2} mm high (limit {} mm)",
        model.boxes.len(),
        hi[0] - lo[0],
        hi[1] - lo[1],
        hi[2],
        spec.package.body.height_max.format()
    );
    (views, summary)
}

fn finding_view(f: &Finding) -> FindingView {
    let gate = Gate::ALL
        .iter()
        .find(|g| g.owns(f))
        .map_or("footprint", |g| g.name());
    FindingView {
        level: match f.level {
            Level::Error => "error",
            Level::Warning => "warning",
        }
        .to_string(),
        check: f.check.id().to_string(),
        path: f.path.clone(),
        message: f.message.clone(),
        gate: gate.to_string(),
        overridden_by: f.overridden_by.clone(),
    }
}

/// Opens a PartSpec file and returns what the window shows.
pub fn open_spec(path: &str) -> Result<SpecView, String> {
    let spec = load_spec(Path::new(path))?;
    Ok(view(path, &spec))
}

/// Approves a stage and saves the file. Errors in the stage, or an open earlier
/// stage, are returned as text that names what to fix.
pub fn approve_stage(
    path: &str,
    stage: &str,
    note: Option<String>,
    at: String,
) -> Result<SpecView, String> {
    let gate = Gate::parse(stage).ok_or_else(|| format!("there is no stage called `{stage}`"))?;
    let mut spec = load_spec(Path::new(path))?;
    let mut findings = validate(&spec, &fab());
    findings.extend(kirpitch_gen_3d::fit_findings(&spec));
    let note = note.filter(|n| !n.trim().is_empty());
    match approve(&mut spec, gate, &findings, note, at) {
        Ok(()) => {}
        Err(ApprovalError::EarlierStageOpen(g)) => {
            return Err(format!("approve the {} stage first", g.name()));
        }
        Err(ApprovalError::Errors { errors, .. }) => {
            let list: Vec<String> = errors
                .iter()
                .map(|e| format!("{}: {}", e.path, e.message))
                .collect();
            return Err(format!(
                "the {} stage has errors: {}",
                gate.name(),
                list.join("; ")
            ));
        }
    }
    fs::write(path, spec.to_json_string().as_bytes()).map_err(|e| format!("{path}: {e}"))?;
    Ok(view(path, &spec))
}

/// Draws the footprint: body outline, courtyard, pads and pad numbers.
/// Coordinates are millimetres with y pointing down, as in the footprint.
pub fn footprint_svg(spec: &PartSpec) -> String {
    let body = &spec.package.body;
    let (cx, cy) = body.center.map_or((0.0, 0.0), |c| (c.x.get(), c.y.get()));
    let (hx, hy) = (body.width.get() / 2.0, body.length.get() / 2.0);
    let mut min = (cx - hx, cy - hy);
    let mut max = (cx + hx, cy + hy);
    for pad in &spec.land_pattern.pads {
        let (x0, y0, x1, y1) = pad.bounds();
        min = (min.0.min(x0), min.1.min(y0));
        max = (max.0.max(x1), max.1.max(y1));
    }
    if let Some(c) = &spec.land_pattern.courtyard {
        min = (min.0.min(c.x_min.get()), min.1.min(c.y_min.get()));
        max = (max.0.max(c.x_max.get()), max.1.max(c.y_max.get()));
    }
    let margin = 0.6;
    let (x, y) = (min.0 - margin, min.1 - margin);
    let (w, h) = (max.0 - min.0 + 2.0 * margin, max.1 - min.1 + 2.0 * margin);
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" role=\"img\" aria-label=\"Footprint {}\">",
        num(x),
        num(y),
        num(w),
        num(h),
        escape(&spec.package.name)
    );
    svg.push_str(&format!(
        "<rect class=\"body\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
        num(cx - hx),
        num(cy - hy),
        num(2.0 * hx),
        num(2.0 * hy)
    ));
    if let Some(c) = &spec.land_pattern.courtyard {
        svg.push_str(&format!(
            "<rect class=\"courtyard\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>",
            num(c.x_min.get()),
            num(c.y_min.get()),
            num(c.x_max.get() - c.x_min.get()),
            num(c.y_max.get() - c.y_min.get())
        ));
    }
    let first = spec.pins.first().map(|p| p.number.as_str());
    for pad in &spec.land_pattern.pads {
        svg.push_str(&pad_svg(pad, Some(pad.number.as_str()) == first));
    }
    svg.push_str("</svg>");
    svg
}

fn pad_svg(pad: &Pad, pin_one: bool) -> String {
    let (x, y) = (pad.x.get(), pad.y.get());
    let (w, h) = (pad.size_x.get(), pad.size_y.get());
    let class = if pin_one { "pad pin1" } else { "pad" };
    let radius = match pad.shape {
        PadShape::Rect => 0.0,
        PadShape::RoundRect => pad.corner_ratio.unwrap_or(0.25) * w.min(h),
        PadShape::Oval | PadShape::Circle => w.min(h) / 2.0,
    };
    let transform = if pad.rotation_deg == 0.0 {
        String::new()
    } else {
        format!(
            " transform=\"rotate({} {} {})\"",
            num(pad.rotation_deg),
            num(x),
            num(y)
        )
    };
    let mut out = format!(
        "<g data-pin=\"{}\"{transform}><rect class=\"{class}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/>",
        escape(&pad.number),
        num(x - w / 2.0),
        num(y - h / 2.0),
        num(w),
        num(h),
        num(radius)
    );
    if pad.kind == PadKind::ThruHole
        && let Some(d) = pad.drill
    {
        out.push_str(&format!(
            "<circle class=\"hole\" cx=\"{}\" cy=\"{}\" r=\"{}\"/>",
            num(x),
            num(y),
            num(d.get() / 2.0)
        ));
    }
    let font = (w.min(h) * 0.6).clamp(0.25, 0.8);
    out.push_str(&format!(
        "<text class=\"label\" x=\"{}\" y=\"{}\" font-size=\"{}\">{}</text></g>",
        num(x),
        num(y),
        num(font),
        escape(&pad.number)
    ));
    out
}

fn num(v: f64) -> String {
    kirpitch_core::format_number(v)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Writes the KiCad symbol, footprint and 3D model of a PartSpec into `out_dir`
/// (`<MPN>.kicad_sym`, `kirpitch.pretty/<package>.kicad_mod` and
/// `kirpitch.3dshapes/<package>.step` and `.wrl`). Returns the
/// paths written. The PartSpec is validated first by the generators.
pub fn export_files(path: &str, out_dir: &str, target: Option<u32>) -> Result<Vec<String>, String> {
    use kirpitch_kicad::KicadTarget;
    let spec = load_spec(Path::new(path))?;
    let target = match target.or(user_folders().kicad_target) {
        Some(major) => KicadTarget::from_major(major).map_err(|e| e.to_string())?,
        None => KicadTarget::detect(kirpitch_kicad::KicadCli::discover().as_ref()),
    };
    let dir = Path::new(out_dir);
    fs::create_dir_all(dir).map_err(|e| format!("{out_dir}: {e}"))?;
    let (step_name, wrl_name) = kirpitch_gen_3d::file_names(&spec);
    let symbol =
        kirpitch_gen_symbol::generate(&spec, &kirpitch_gen_symbol::Options { target, fab: fab() })
            .map_err(|e| e.to_string())?;
    let footprint = kirpitch_gen_footprint::generate(
        &spec,
        &kirpitch_gen_footprint::Options {
            target,
            fab: fab(),
            model: Some(format!("${{KIPRJMOD}}/kirpitch.3dshapes/{}", step_name)),
        },
    )
    .map_err(|e| e.to_string())?;
    let model = kirpitch_gen_3d::build(&spec).map_err(|e| e.to_string())?;
    if let Some(issue) = kirpitch_gen_3d::check_fit(&spec, &model).first() {
        return Err(format!("3D model: {}", issue.message));
    }
    let shapes = dir.join("kirpitch.3dshapes");
    fs::create_dir_all(&shapes).map_err(|e| format!("{}: {e}", shapes.display()))?;
    let step_path = shapes.join(&step_name);
    let wrl_path = shapes.join(&wrl_name);
    fs::write(&step_path, kirpitch_gen_3d::write_step(&model).as_bytes())
        .map_err(|e| format!("{}: {e}", step_path.display()))?;
    fs::write(&wrl_path, kirpitch_gen_3d::write_vrml(&model).as_bytes())
        .map_err(|e| format!("{}: {e}", wrl_path.display()))?;
    let sym_path = dir.join(kirpitch_gen_symbol::file_name(&spec));
    let pretty = dir.join("kirpitch.pretty");
    fs::create_dir_all(&pretty).map_err(|e| format!("{}: {e}", pretty.display()))?;
    let mod_path = pretty.join(kirpitch_gen_footprint::file_name(&spec));
    fs::write(&sym_path, symbol.as_bytes()).map_err(|e| format!("{}: {e}", sym_path.display()))?;
    fs::write(&mod_path, footprint.as_bytes())
        .map_err(|e| format!("{}: {e}", mod_path.display()))?;
    Ok(vec![
        sym_path.display().to_string(),
        mod_path.display().to_string(),
        step_path.display().to_string(),
        wrl_path.display().to_string(),
    ])
}

/// The current time as `2026-11-06T12:00:00Z`.
pub fn now_rfc3339() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
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

/// A rendered datasheet page for the window.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PageImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The pixels as RGBA, row by row, encoded as base64 text.
    pub rgba_base64: String,
    /// Number of pages of the document.
    pub page_count: u32,
}

/// Looks for the pdfium library: the folder in `KIRPITCH_PDFIUM_LIB_DIR`, else
/// the folder that `cargo xtask fetch-pdfium` fills in the current directory.
pub fn find_pdfium_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("KIRPITCH_PDFIUM_LIB_DIR") {
        return Some(dir.into());
    }
    if let Some(dir) = user_folders().pdfium_dir.map(std::path::PathBuf::from)
        && dir.is_dir()
    {
        return Some(dir);
    }
    let (platform, lib) = if cfg!(windows) {
        ("win-x64", "bin")
    } else if cfg!(target_os = "macos") {
        (
            if cfg!(target_arch = "aarch64") {
                "mac-arm64"
            } else {
                "mac-x64"
            },
            "lib",
        )
    } else {
        ("linux-x64", "lib")
    };
    let relative = Path::new("pdfium").join(platform).join(lib);
    let dir = Path::new("testdata").join(&relative);
    if dir.is_dir() {
        return Some(dir);
    }
    let dir = find_testdata()?.join(relative);
    dir.is_dir().then_some(dir)
}

/// Renders one page of a datasheet PDF, `width_px` pixels wide.
pub fn render_datasheet_page(pdf: &str, page: u32, width_px: u32) -> Result<PageImage, String> {
    let bytes = fs::read(pdf).map_err(|e| format!("{pdf}: {e}"))?;
    let reader = pdf_reader()?;
    let count = reader.page_count(&bytes).map_err(|e| e.to_string())?;
    let page = page.clamp(1, count.max(1));
    let image = reader
        .render_page(&bytes, page, width_px.clamp(200, 2400))
        .map_err(|e| e.to_string())?;
    Ok(PageImage {
        width: image.width,
        height: image.height,
        rgba_base64: base64(&image.rgba),
        page_count: count,
    })
}

/// Standard base64 with padding.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Draws the symbol that the generator makes for a PartSpec: body, pins with
/// their names and numbers. Pins carry `data-pin` so the window can highlight
/// them. Returns an error when the symbol cannot be generated.
pub fn symbol_svg(spec: &PartSpec) -> Result<String, String> {
    use kirpitch_kicad::{List, Node};
    let text = kirpitch_gen_symbol::generate(spec, &kirpitch_gen_symbol::Options::default())
        .map_err(|e| e.to_string())?;
    let tree = kirpitch_kicad::sexpr::parse(&text).map_err(|e| e.to_string())?;
    fn child<'a>(l: &'a List, name: &str) -> Option<&'a List> {
        l.items.iter().find_map(|n| match n {
            Node::List(c) if c.name() == Some(name) => Some(c),
            _ => None,
        })
    }
    fn word(l: &List, i: usize) -> Option<&str> {
        match l.items.get(i) {
            Some(Node::Atom(s)) | Some(Node::Str(s)) => Some(s),
            _ => None,
        }
    }
    fn nums(l: &List) -> Vec<f64> {
        (1..l.items.len())
            .filter_map(|i| word(l, i)?.parse().ok())
            .collect()
    }
    let symbol = child(&tree, "symbol").ok_or("no symbol")?;
    let unit = symbol
        .items
        .iter()
        .find_map(|n| match n {
            Node::List(c) if c.name() == Some("symbol") => Some(c),
            _ => None,
        })
        .ok_or("no symbol unit")?;
    let rect = child(unit, "rectangle").ok_or("no body")?;
    let start = nums(child(rect, "start").ok_or("no start")?);
    let end = nums(child(rect, "end").ok_or("no end")?);
    let (x0, x1) = (start[0].min(end[0]), start[0].max(end[0]));
    let (y0, y1) = (start[1].min(end[1]), start[1].max(end[1]));
    let mut min = (x0, -y1);
    let mut max = (x1, -y0);
    let mut pins = String::new();
    let mut seen: Vec<String> = Vec::new();
    for n in &unit.items {
        let Node::List(pin) = n else { continue };
        if pin.name() != Some("pin") {
            continue;
        }
        let at = nums(child(pin, "at").ok_or("no pin position")?);
        let length = nums(child(pin, "length").ok_or("no pin length")?)[0];
        let name = child(pin, "name").and_then(|l| word(l, 1)).unwrap_or("");
        let number = child(pin, "number").and_then(|l| word(l, 1)).unwrap_or("");
        // Stacked power pins share a place; draw the visible one only.
        let hidden = child(pin, "hide").is_some()
            || pin
                .items
                .iter()
                .any(|n| matches!(n, Node::Atom(a) if a == "hide"));
        if hidden {
            continue;
        }
        let (dx, dy) = match at[2] as i32 {
            0 => (1.0, 0.0),
            90 => (0.0, 1.0),
            180 => (-1.0, 0.0),
            _ => (0.0, -1.0),
        };
        // KiCad symbols have y pointing up; the drawing has y pointing down.
        let (ax, ay) = (at[0], -at[1]);
        let (bx, by) = (ax + dx * length, ay - dy * length);
        for (px, py) in [(ax, ay), (bx, by)] {
            min = (min.0.min(px), min.1.min(py));
            max = (max.0.max(px), max.1.max(py));
        }
        let (nx, ny) = (bx + dx * 1.016, by - dy * 1.016);
        let (anchor, rotate) = match (dx as i32, dy as i32) {
            (1, 0) => ("start", false),
            (-1, 0) => ("end", false),
            (0, 1) => ("start", true),
            _ => ("end", true),
        };
        // Names are centred on the pin line: a little below it for horizontal pins,
        // beside it for vertical ones.
        let (nx, ny) = if rotate {
            (nx + 0.4, ny)
        } else {
            (nx, ny + 0.4)
        };
        let name_transform = if rotate {
            format!(" transform=\"rotate(-90 {} {})\"", num(nx), num(ny))
        } else {
            String::new()
        };
        let (mx, my) = ((ax + bx) / 2.0, (ay + by) / 2.0);
        let (tx, ty) = if rotate {
            (mx - 0.5, my)
        } else {
            (mx, my - 0.5)
        };
        let num_transform = if rotate {
            format!(" transform=\"rotate(-90 {} {})\"", num(tx), num(ty))
        } else {
            String::new()
        };
        seen.push(number.to_string());
        pins.push_str(&format!(
            "<g class=\"spin\" data-pin=\"{num_e}\"><line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"/><rect class=\"hit\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/><text class=\"pname\" x=\"{}\" y=\"{}\" text-anchor=\"{anchor}\"{name_transform}>{}</text><text class=\"pnum\" x=\"{}\" y=\"{}\" text-anchor=\"middle\"{num_transform}>{}</text></g>",
            num(ax), num(ay), num(bx), num(by),
            num(ax.min(bx) - 0.4), num(ay.min(by) - 0.8),
            num((ax - bx).abs() + 0.8), num((ay - by).abs() + 1.6),
            num(nx), num(ny), escape(name),
            num(tx), num(ty), escape(number),
            num_e = escape(number),
        ));
    }
    let margin = 2.0;
    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" role=\"img\" aria-label=\"Symbol {}\"><rect class=\"sbody\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"/>{pins}</svg>",
        num(min.0 - margin),
        num(min.1 - margin),
        num(max.0 - min.0 + 2.0 * margin),
        num(max.1 - min.1 + 2.0 * margin),
        escape(&spec.metadata.mpn),
        num(x0),
        num(-y1),
        num(x1 - x0),
        num(y1 - y0),
    ))
}

pub(crate) fn pdf_reader() -> Result<kirpitch_extract::PdfReader, String> {
    let dir = find_pdfium_dir();
    kirpitch_extract::PdfReader::new(dir.as_deref()).map_err(|e| {
        format!("{e}. Run `cargo xtask fetch-pdfium` and set KIRPITCH_PDFIUM_LIB_DIR to the folder it prints.")
    })
}

/// The text of a region of a datasheet page, split into rows and columns.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RegionTable {
    /// The cells, row by row.
    pub rows: Vec<Vec<String>>,
    /// The same as tab-separated text, which the pin import reads.
    pub tsv: String,
}

fn check_region(region: [f64; 4]) -> Result<kirpitch_core::BoundingBox, String> {
    let bbox = kirpitch_core::BoundingBox(region);
    if bbox.is_valid() && region[2] - region[0] > 0.001 && region[3] - region[1] > 0.001 {
        Ok(bbox)
    } else {
        Err("the selected region is empty or outside the page".to_string())
    }
}

/// Reads the text inside `region` (fractions from the top-left corner of the
/// page) and finds its columns.
pub fn read_region(pdf: &str, page: u32, region: [f64; 4]) -> Result<RegionTable, String> {
    let bbox = check_region(region)?;
    let bytes = fs::read(pdf).map_err(|e| format!("{pdf}: {e}"))?;
    let lines = pdf_reader()?
        .lines_in_region(&bytes, page, bbox)
        .map_err(|e| e.to_string())?;
    if lines.is_empty() {
        return Err("there is no text in the selected region (a scanned page needs OCR)".into());
    }
    let table = kirpitch_extract::table_from_lines(&lines, kirpitch_extract::DEFAULT_COLUMN_GAP);
    Ok(RegionTable {
        tsv: table.to_tsv(),
        rows: table.rows,
    })
}

/// The result of importing a pin table.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImportOutcome {
    /// The PartSpec as it is after the import.
    pub view: SpecView,
    /// Types that were guessed and other things to look at.
    pub warnings: Vec<String>,
}

/// Replaces the pins of a PartSpec by the pins in `tsv` (tab separated, or any
/// text the pin import understands) and saves the file. The page and region are
/// saved as the source of every pin. Approvals of the pinout and later stages
/// become stale because the content changed.
pub fn import_pins(
    path: &str,
    tsv: &str,
    page: u32,
    region: Option<[f64; 4]>,
) -> Result<ImportOutcome, String> {
    let bbox = region.map(check_region).transpose()?;
    let mut spec = load_spec(Path::new(path))?;
    let source = kirpitch_core::Source {
        page,
        bbox,
        confidence: None,
    };
    let result = kirpitch_core::pintable::parse_pin_table(
        tsv,
        &kirpitch_core::pintable::ImportOptions {
            source: Some(source),
        },
    )
    .map_err(|e| e.to_string())?;
    spec.pins = result.pins;
    fs::write(path, spec.to_json_string().as_bytes()).map_err(|e| format!("{path}: {e}"))?;
    Ok(ImportOutcome {
        view: view(path, &spec),
        warnings: result.warnings,
    })
}

/// The electrical types of pins, in the order of the type menu.
const ELECTRICAL_TYPES: [kirpitch_core::ElectricalType; 12] = {
    use kirpitch_core::ElectricalType::*;
    [
        Input,
        Output,
        Bidirectional,
        TriState,
        Passive,
        PowerIn,
        PowerOut,
        OpenCollector,
        OpenEmitter,
        NoConnect,
        Free,
        Unspecified,
    ]
};

/// A pin as edited in the window.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PinInput {
    /// Pin number.
    pub number: String,
    /// Pin name.
    pub name: String,
    /// Electrical type name, for example `power_in`.
    pub electrical_type: String,
    /// Group, if any.
    pub group: Option<String>,
    /// Datasheet page of the source, if the pin has one.
    pub page: Option<u32>,
    /// Region of the source, if the pin has one.
    pub region: Option<[f64; 4]>,
}

/// Replaces the pins of a PartSpec by the edited list and saves the file.
///
/// The list may be invalid (for example a duplicate number); that is shown as
/// findings so the user can fix it, and the pinout cannot be approved until
/// they have. Approvals of the pinout and later stages become stale because the
/// content changed. Nothing is saved when a type name is wrong.
pub fn set_pins(path: &str, pins: Vec<PinInput>) -> Result<SpecView, String> {
    let mut spec = load_spec(Path::new(path))?;
    let mut out = Vec::with_capacity(pins.len());
    for (i, p) in pins.into_iter().enumerate() {
        let electrical_type = ELECTRICAL_TYPES
            .iter()
            .copied()
            .find(|t| t.as_str() == p.electrical_type)
            .ok_or_else(|| {
                format!(
                    "pin {}: there is no electrical type `{}`",
                    i + 1,
                    p.electrical_type
                )
            })?;
        let bbox = p.region.map(check_region).transpose()?;
        out.push(kirpitch_core::partspec::Pin {
            number: p.number.trim().to_string(),
            name: p.name.trim().to_string(),
            electrical_type,
            group: p
                .group
                .map(|g| g.trim().to_string())
                .filter(|g| !g.is_empty()),
            alt_functions: spec
                .pins
                .iter()
                .find(|old| old.number == p.number.trim())
                .map(|old| old.alt_functions.clone())
                .unwrap_or_default(),
            source: p.page.map(|page| kirpitch_core::Source {
                page,
                bbox,
                confidence: None,
            }),
        });
    }
    spec.pins = out;
    fs::write(path, spec.to_json_string().as_bytes()).map_err(|e| format!("{path}: {e}"))?;
    Ok(view(path, &spec))
}
