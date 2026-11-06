// SPDX-License-Identifier: GPL-3.0-or-later

//! Starting points for the window: the example parts, a new part from a package
//! form, copies, and building a part from a datasheet with a language model.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kirpitch_core::BoundingBox;
use kirpitch_core::ElectricalType;
use kirpitch_core::ipc::{Density, Range};
use kirpitch_core::partspec::{Approvals, Metadata, Origin, PartSpec, Pin, SCHEMA_VERSION};
use kirpitch_core::templates::{
    DualRowGullWing, DualRowThroughHole, QuadFlatNoLead, Template, dual_row_gull_wing,
    dual_row_through_hole, quad_flat_no_lead, sot23,
};
use kirpitch_models::{BuildOptions, Cache, Config, CurlTransport, PageLines, TextLine, Transport};
use serde::{Deserialize, Serialize};

use crate::{SpecView, load_spec, open_spec, pdf_reader};

/// A folder that holds `testdata`, found from the current folder or from the
/// folder of the program, going up. `None` when the app runs outside a checkout.
pub fn find_testdata() -> Option<PathBuf> {
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        starts.push(cwd);
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        starts.push(dir);
    }
    for start in starts {
        for dir in start.ancestors() {
            let candidate = dir.join("testdata");
            if candidate.join("golden").is_dir() {
                return Some(candidate);
            }
        }
    }
    None
}

/// One example part.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Example {
    /// Folder name, for example `lm358-soic8`.
    pub name: String,
    /// Part number.
    pub mpn: String,
    /// Package name.
    pub package: String,
    /// Description.
    pub description: String,
    /// The datasheet PDF, when it has been downloaded (`cargo xtask
    /// fetch-datasheets`).
    pub pdf: Option<String>,
}

/// The example (golden) parts that can be opened.
pub fn list_examples() -> Vec<Example> {
    let Some(data) = find_testdata() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(data.join("golden")) else {
        return out;
    };
    for entry in entries.flatten() {
        let spec_path = entry.path().join("partspec.json");
        let Ok(spec) = load_spec(&spec_path) else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let pdf = data.join("pdfs").join(format!("{name}.pdf"));
        out.push(Example {
            name,
            mpn: spec.metadata.mpn,
            package: spec.package.name,
            description: spec.metadata.description,
            pdf: pdf.is_file().then(|| pdf.display().to_string()),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Opens an example as a working copy, so approvals and edits never change the
/// original. The copy lives in the temporary folder and is kept between runs;
/// `fresh` replaces it by a new copy.
pub fn open_example(name: &str, fresh: bool) -> Result<SpecView, String> {
    let data =
        find_testdata().ok_or("the example parts are only available in a checkout of Kirpitch")?;
    if name.contains(['/', '\\']) || name.contains("..") {
        return Err("not an example name".into());
    }
    let from = data.join("golden").join(name).join("partspec.json");
    if !from.is_file() {
        return Err(format!("there is no example called `{name}`"));
    }
    let dir = std::env::temp_dir().join("kirpitch-examples").join(name);
    let to = dir.join("partspec.json");
    if fresh || !to.is_file() {
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        fs::copy(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    open_spec(&to.display().to_string())
}

/// Writes a copy of a PartSpec file and returns the view of the copy. The
/// copy keeps the approvals it had.
pub fn save_copy(path: &str, dest: &str) -> Result<SpecView, String> {
    let spec = load_spec(Path::new(path))?;
    if let Some(parent) = Path::new(dest)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(dest, spec.to_json_string().as_bytes()).map_err(|e| format!("{dest}: {e}"))?;
    open_spec(dest)
}

/// The answers of the "new part" form.
#[derive(Debug, Clone, Deserialize)]
pub struct NewPartInput {
    /// Where to write the PartSpec.
    pub path: String,
    /// Manufacturer part number.
    pub mpn: String,
    /// Manufacturer.
    pub manufacturer: String,
    /// One-line description.
    pub description: String,
    /// `dual-row` (SOIC, SSOP, TSSOP, MSOP), `sot23`, `dip` or `qfn`.
    pub kind: String,
    /// Name of the family in the footprint name, for example `TSSOP`.
    #[serde(default)]
    pub family: Option<String>,
    /// Terminals of a dual-row package.
    #[serde(default)]
    pub pin_count: Option<u32>,
    /// QFN terminals on the top and bottom sides.
    #[serde(default)]
    pub pins_x: Option<u32>,
    /// QFN terminals on the left and right sides.
    #[serde(default)]
    pub pins_y: Option<u32>,
    /// Pitch (e).
    pub pitch: f64,
    /// Largest height (A).
    pub height: f64,
    /// Body length along the pin rows (D).
    #[serde(default)]
    pub body_length: Option<f64>,
    /// Body width across the rows (E1).
    #[serde(default)]
    pub body_width: Option<f64>,
    /// Lead span tip to tip (E) as min, max.
    #[serde(default)]
    pub lead_span: Option<[f64; 2]>,
    /// Lead or terminal length (L) as min, max.
    #[serde(default)]
    pub lead_length: Option<[f64; 2]>,
    /// Lead or terminal width (b) as min, max.
    #[serde(default)]
    pub lead_width: Option<[f64; 2]>,
    /// QFN body size along x (D) as min, max.
    #[serde(default)]
    pub body_x: Option<[f64; 2]>,
    /// QFN body size along y (E) as min, max.
    #[serde(default)]
    pub body_y: Option<[f64; 2]>,
    /// QFN exposed pad width and height.
    #[serde(default)]
    pub exposed_pad: Option<[f64; 2]>,
    /// DIP row spacing.
    #[serde(default)]
    pub row_spacing: Option<f64>,
    /// DIP drill diameter.
    #[serde(default)]
    pub drill: Option<f64>,
    /// DIP pad diameter.
    #[serde(default)]
    pub pad_diameter: Option<f64>,
}

fn need<T: Copy>(value: Option<T>, what: &str) -> Result<T, String> {
    value.ok_or_else(|| format!("{what} is missing"))
}

fn range(r: Option<[f64; 2]>, what: &str) -> Result<Range, String> {
    let [min, max] = need(r, what)?;
    Ok(Range::new(min.min(max), min.max(max)))
}

fn template_from_form(i: &NewPartInput) -> Result<Template, String> {
    let family = i.family.clone().unwrap_or_default();
    let made = match i.kind.as_str() {
        "dual-row" | "sot23" => {
            let input = DualRowGullWing {
                family: if family.is_empty() {
                    if i.kind == "sot23" {
                        "SOT-23".into()
                    } else {
                        "SOIC".into()
                    }
                } else {
                    family
                },
                pin_count: need(i.pin_count, "the number of pins")?,
                pitch: i.pitch,
                body_length: need(i.body_length, "the body length (D)")?,
                body_width: need(i.body_width, "the body width (E1)")?,
                height_max: i.height,
                lead_span: range(i.lead_span, "the lead span (E)")?,
                lead_length: range(i.lead_length, "the lead length (L)")?,
                lead_width: range(i.lead_width, "the lead width (b)")?,
            };
            if i.kind == "sot23" {
                sot23(&input, Density::Nominal)
            } else {
                dual_row_gull_wing(&input, Density::Nominal)
            }
        }
        "dip" => dual_row_through_hole(&DualRowThroughHole {
            family: if family.is_empty() {
                "DIP".into()
            } else {
                family
            },
            pin_count: need(i.pin_count, "the number of pins")?,
            pitch: i.pitch,
            row_spacing: need(i.row_spacing, "the row spacing")?,
            body_length: need(i.body_length, "the body length (D)")?,
            body_width: need(i.body_width, "the body width (E1)")?,
            height_max: i.height,
            pad_diameter: i.pad_diameter.unwrap_or(1.6),
            drill: i.drill.unwrap_or(0.8),
        }),
        "qfn" => quad_flat_no_lead(
            &QuadFlatNoLead {
                family: if family.is_empty() {
                    "QFN".into()
                } else {
                    family
                },
                pins_x: need(i.pins_x, "the terminals on the top and bottom (pins x)")?,
                pins_y: need(i.pins_y, "the terminals on the left and right (pins y)")?,
                pitch: i.pitch,
                body_x: range(i.body_x, "the body size D")?,
                body_y: range(i.body_y, "the body size E")?,
                height_max: i.height,
                lead_length: range(i.lead_length, "the terminal length (L)")?,
                lead_width: range(i.lead_width, "the terminal width (b)")?,
                exposed_pad: i.exposed_pad.map(|p| (p[0], p[1])),
            },
            Density::Nominal,
        ),
        other => return Err(format!("unknown package kind `{other}`")),
    };
    made.map_err(|e| e.to_string())
}

/// Makes a new PartSpec from a package form and writes it. The pins are
/// placeholders (`PIN1`, `PIN2`, ... of unspecified type; an exposed pad is
/// `EP`) to be named in the pin table. An existing file is never replaced.
pub fn new_part(input: &NewPartInput) -> Result<SpecView, String> {
    if input.mpn.trim().is_empty() {
        return Err("give the part number".into());
    }
    if input.path.trim().is_empty() {
        return Err("choose where to save the part".into());
    }
    if Path::new(&input.path).exists() {
        return Err(format!(
            "{} already exists; choose another name",
            input.path
        ));
    }
    let template = template_from_form(input)?;
    let total = template.package.pin_count;
    let terminals = total
        - u32::from(
            template
                .land_pattern
                .pads
                .iter()
                .any(|p| p.number == total.to_string())
                && input.exposed_pad.is_some(),
        );
    let pins: Vec<Pin> = (1..=total)
        .map(|n| {
            let ep = n > terminals;
            Pin {
                number: n.to_string(),
                name: if ep { "EP".into() } else { format!("PIN{n}") },
                electrical_type: if ep {
                    ElectricalType::Passive
                } else {
                    ElectricalType::Unspecified
                },
                group: None,
                alt_functions: vec![],
                source: None,
            }
        })
        .collect();
    let spec = PartSpec {
        schema_version: SCHEMA_VERSION,
        origin: Origin::Hand,
        metadata: Metadata {
            mpn: input.mpn.trim().to_string(),
            manufacturer: input.manufacturer.trim().to_string(),
            description: input.description.trim().to_string(),
            keywords: vec![],
            datasheet_url: None,
            datasheet_sha256: None,
        },
        package: template.package,
        pins,
        land_pattern: template.land_pattern,
        approvals: Approvals::default(),
        overrides: vec![],
    };
    if let Some(parent) = Path::new(&input.path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(&input.path, spec.to_json_string().as_bytes())
        .map_err(|e| format!("{}: {e}", input.path))?;
    open_spec(&input.path)
}

/// The text of every page of a PDF, with the place of each line.
pub fn read_pages(bytes: &[u8]) -> Result<Vec<PageLines>, String> {
    let reader = pdf_reader()?;
    let count = reader.page_count(bytes).map_err(|e| e.to_string())?;
    let whole = BoundingBox([0.0, 0.0, 1.0, 1.0]);
    let mut pages = Vec::new();
    for page in 1..=count {
        let lines = reader
            .lines_in_region(bytes, page, whole)
            .map_err(|e| e.to_string())?;
        pages.push(PageLines {
            page,
            lines: lines
                .iter()
                .filter_map(|l| {
                    let (first, last) = (l.words.first()?, l.words.last()?);
                    Some(TextLine {
                        text: l.text(),
                        y: l.y,
                        left: first.left,
                        right: last.right,
                    })
                })
                .collect(),
        });
    }
    Ok(pages)
}

/// The answers of the "from datasheet" form.
#[derive(Debug, Clone, Deserialize)]
pub struct DatasheetInput {
    /// The datasheet PDF.
    pub pdf: String,
    /// Where to write the PartSpec.
    pub out: String,
    /// Where to look or which package, for example `SOIC-8, drawing D0008A`.
    #[serde(default)]
    pub hint: String,
    /// Read the pin table twice.
    #[serde(default)]
    pub second_read: bool,
    /// A `kirpitch.toml` (default: the one in the current folder).
    #[serde(default)]
    pub config: String,
    /// Where model answers are remembered (default: a folder in the temporary
    /// folder). Tests give their own.
    #[serde(default)]
    pub cache_dir: String,
}

/// What a build from a datasheet gave.
#[derive(Debug, Clone, Serialize)]
pub struct DatasheetOutcome {
    /// The new part, when it could be built.
    pub view: Option<SpecView>,
    /// For each task: model, and whether data stayed local.
    pub models: Vec<String>,
    /// Things to look at in review.
    pub notes: Vec<String>,
    /// Stages that failed.
    pub failures: Vec<String>,
    /// Calls, tokens and time, in one line.
    pub usage: String,
    /// The pins file written when only the pins could be read.
    pub pins_csv: Option<String>,
}

/// Builds a part from a datasheet with the configured language models. This
/// can take minutes and sends text of the datasheet to the configured models
/// (the result says which ones are local).
pub fn build_from_datasheet(input: &DatasheetInput) -> Result<DatasheetOutcome, String> {
    build_with_transport(input, Arc::new(CurlTransport))
}

/// Like [`build_from_datasheet`] with a given transport (for tests).
pub fn build_with_transport(
    input: &DatasheetInput,
    transport: Arc<dyn Transport>,
) -> Result<DatasheetOutcome, String> {
    use sha2::{Digest, Sha256};
    if input.out.trim().is_empty() {
        return Err("choose where to save the part".into());
    }
    if Path::new(&input.out).exists() {
        return Err(format!("{} already exists; choose another name", input.out));
    }
    let config = if input.config.trim().is_empty() {
        Config::discover().map_err(|e| e.to_string())?
    } else {
        Some(Config::from_file(Path::new(input.config.trim())).map_err(|e| e.to_string())?)
    }
    .ok_or("no language model is configured: put a kirpitch.toml in the folder where the app was started, or choose one (see kirpitch.example.toml). Without a model, use New part and import the pins")?;
    let bytes = fs::read(&input.pdf).map_err(|e| format!("{}: {e}", input.pdf))?;
    let pages = read_pages(&bytes)?;
    if pages.iter().all(|p| p.lines.is_empty()) {
        return Err("the PDF has no text on any page (a scan); this version cannot read scans. Use New part instead".into());
    }
    let options = BuildOptions {
        hint: Some(input.hint.trim().to_string()).filter(|h| !h.is_empty()),
        second_read: input.second_read,
        pdf_sha256: Some(
            Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        ),
        ..Default::default()
    };
    let cache = Cache::in_dir(&if input.cache_dir.trim().is_empty() {
        std::env::temp_dir().join("kirpitch-cache")
    } else {
        PathBuf::from(input.cache_dir.trim())
    });
    let report = kirpitch_models::build(&config, transport, &cache, &pages, &options);
    let models = report
        .models
        .iter()
        .map(|(task, name, local)| {
            format!(
                "{}: {name} ({})",
                task.name(),
                if *local {
                    "stays on this machine or network"
                } else {
                    "data leaves this machine"
                }
            )
        })
        .collect();
    let s = &report.stats;
    let usage = format!(
        "{} model call(s), {} from the cache, {} tokens in, {} out, {:.0} s",
        s.calls, s.cached, s.usage.input_tokens, s.usage.output_tokens, s.seconds
    );
    let mut outcome = DatasheetOutcome {
        view: None,
        models,
        notes: report.notes.clone(),
        failures: report.failures.clone(),
        usage,
        pins_csv: None,
    };
    if let Some(spec) = &report.spec {
        if let Some(parent) = Path::new(&input.out)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&input.out, spec.to_json_string().as_bytes())
            .map_err(|e| format!("{}: {e}", input.out))?;
        outcome.view = Some(open_spec(&input.out)?);
    } else if let Some(pins) = &report.pins {
        let mut csv = String::from("Pin,Name,Type\n");
        for p in &pins.pins {
            csv.push_str(&format!(
                "{},{},{}\n",
                p.pin.number,
                p.pin.name.replace(',', ";"),
                p.pin.electrical_type.as_str()
            ));
        }
        let path = Path::new(&input.out).with_extension("pins.csv");
        fs::write(&path, csv).map_err(|e| format!("{}: {e}", path.display()))?;
        outcome.pins_csv = Some(path.display().to_string());
    }
    Ok(outcome)
}
