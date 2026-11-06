// SPDX-License-Identifier: GPL-3.0-or-later

//! The logic behind the Kirpitch desktop app.
//!
//! The window shows a PartSpec: its review gates, pins, pads, the findings of
//! the checks and a drawing of the footprint. Everything the window needs is
//! computed here as plain data ([`SpecView`]) so that it can be tested without
//! a window; `main.rs` only connects these functions to the web view.

use std::fs;
use std::path::Path;

use kirpitch_core::gates::{ApprovalError, Gate, GateStatus, approve, stage_findings};
use kirpitch_core::partspec::{Pad, PadKind, PadShape, PartSpec};
use kirpitch_core::{FabProfile, Finding, Level, validate};
use serde::Serialize;

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
    let findings = validate(spec, &FabProfile::generic());
    let gates = Gate::ALL
        .iter()
        .map(|&gate| {
            let mine = stage_findings(gate, &findings);
            let errors = mine.iter().filter(|f| f.level == Level::Error).count();
            let approval = match gate {
                Gate::Pinout => spec.approvals.pinout.as_ref(),
                Gate::Footprint => spec.approvals.footprint.as_ref(),
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
    }
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
    let findings = validate(&spec, &FabProfile::generic());
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
        "<g{transform}><rect class=\"{class}\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{}\"/>",
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

/// Writes the KiCad symbol and footprint of a PartSpec into `out_dir`
/// (`<MPN>.kicad_sym` and `kirpitch.pretty/<package>.kicad_mod`). Returns the
/// paths written. The PartSpec is validated first by the generators.
pub fn export_files(path: &str, out_dir: &str, target: Option<u32>) -> Result<Vec<String>, String> {
    use kirpitch_kicad::KicadTarget;
    let spec = load_spec(Path::new(path))?;
    let target = match target {
        Some(major) => KicadTarget::from_major(major).map_err(|e| e.to_string())?,
        None => KicadTarget::detect(kirpitch_kicad::KicadCli::discover().as_ref()),
    };
    let dir = Path::new(out_dir);
    fs::create_dir_all(dir).map_err(|e| format!("{out_dir}: {e}"))?;
    let symbol = kirpitch_gen_symbol::generate(
        &spec,
        &kirpitch_gen_symbol::Options {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let footprint = kirpitch_gen_footprint::generate(
        &spec,
        &kirpitch_gen_footprint::Options {
            target,
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
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
    let dir = Path::new("testdata/pdfium").join(platform).join(lib);
    dir.is_dir().then_some(dir)
}

/// Renders one page of a datasheet PDF, `width_px` pixels wide.
pub fn render_datasheet_page(pdf: &str, page: u32, width_px: u32) -> Result<PageImage, String> {
    let bytes = fs::read(pdf).map_err(|e| format!("{pdf}: {e}"))?;
    let dir = find_pdfium_dir();
    let reader = kirpitch_extract::PdfReader::new(dir.as_deref()).map_err(|e| {
        format!("{e}. Run `cargo xtask fetch-pdfium` and set KIRPITCH_PDFIUM_LIB_DIR to the folder it prints.")
    })?;
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
