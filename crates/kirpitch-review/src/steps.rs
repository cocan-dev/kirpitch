// SPDX-License-Identifier: GPL-3.0-or-later

//! The step by step way to read a datasheet: each step is read from the place
//! the person pointed at, the person confirms or corrects the values, and the
//! part is made from the confirmed values alone.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use kirpitch_core::{Level, has_errors, validate};
use kirpitch_models::steps::{
    FootprintPreview, Step, StepRun, assemble_confirmed, check_step, preview_footprint, run_step,
};
use kirpitch_models::{
    BuildOptions, Cache, Config, CurlTransport, OsKeyStore, PageLines, Progress,
};
use serde::Deserialize;
use serde_json::Value;

use crate::guide::{RegionInput, text_of_regions};
use crate::{DatasheetOutcome, open_spec, read_pages};

/// What to read.
#[derive(Debug, Clone, Deserialize)]
pub struct ReadStepInput {
    /// The datasheet.
    pub pdf: String,
    /// \`metadata\`, \`pins\`, \`package\` or \`footprint\`.
    pub step: String,
    /// The places pointed at. Without any, the datasheet is searched.
    #[serde(default)]
    pub regions: Vec<RegionInput>,
    /// The hint of the dialog.
    #[serde(default)]
    pub hint: String,
    /// Read the pin table twice.
    #[serde(default)]
    pub second_read: bool,
}

fn config() -> Result<Config, String> {
    Config::discover()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            "no language model is configured: add one in Settings, or enter the values by hand"
                .into()
        })
}

/// The text of the places, or the whole datasheet when none were pointed at.
fn pages_for(bytes: &[u8], regions: &[RegionInput]) -> Result<(Vec<PageLines>, bool), String> {
    match text_of_regions(bytes, regions)? {
        Some(pages) => Ok((pages, true)),
        None => Ok((read_pages(bytes)?, false)),
    }
}

/// Reads one step with the configured models. \`on\` hears of each call.
///
/// # Errors
///
/// No model, an unreadable place, or an answer that cannot be used; the person
/// can then type the values in.
pub fn read_step(input: &ReadStepInput, on: &dyn Fn(&Progress)) -> Result<StepRun, String> {
    let step = Step::from_name(&input.step)?;
    let config = config()?;
    let bytes = fs::read(&input.pdf).map_err(|e| format!("{}: {e}", input.pdf))?;
    let (pages, given) = pages_for(&bytes, &input.regions)?;
    let cache = if config.extraction.cache() {
        Cache::in_dir(&std::env::temp_dir().join("kirpitch-cache"))
    } else {
        Cache::none()
    };
    run_step(
        &config,
        Arc::new(CurlTransport),
        &cache,
        &OsKeyStore,
        step,
        &pages,
        given,
        Some(input.hint.trim()).filter(|h| !h.is_empty()),
        input.second_read,
        on,
    )
}

/// Checks the values of a step that a person typed or changed. \`regions\` is
/// where the pins are traced back to.
///
/// # Errors
///
/// The values cannot be used; the message says what is wrong.
pub fn check_step_values(
    pdf: &str,
    step: &str,
    value: &Value,
    regions: &[RegionInput],
) -> Result<StepRun, String> {
    let step = Step::from_name(step)?;
    let pages = if step == Step::Pins && !regions.is_empty() {
        let bytes = fs::read(pdf).map_err(|e| format!("{pdf}: {e}"))?;
        text_of_regions(&bytes, regions)?.unwrap_or_default()
    } else {
        Vec::new()
    };
    check_step(step, value, &pages)
}

/// The footprint the values make, for looking at.
///
/// # Errors
///
/// The values cannot make a footprint.
pub fn preview_footprint_values(
    package: &Value,
    footprint: Option<&Value>,
) -> Result<FootprintPreview, String> {
    preview_footprint(package, footprint)
}

/// The confirmed values.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfirmedInput {
    /// The datasheet.
    pub pdf: String,
    /// Where to write the part.
    pub out: String,
    /// The hint (kept in the PartSpec's notes of origin, not used to read).
    #[serde(default)]
    pub hint: String,
    /// Part number, maker, description.
    pub metadata: Value,
    /// The pins.
    pub pins: Value,
    /// The package.
    pub package: Value,
    /// The recommended footprint, when it is used.
    #[serde(default)]
    pub footprint: Option<Value>,
    /// Where the pins were read, for tracing them back to the page.
    #[serde(default)]
    pub pin_regions: Vec<RegionInput>,
    /// Only check; write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

/// Makes the part from confirmed values, checks it the way the review will, and
/// writes it (unless this is a dry run). Nothing is asked of a model.
///
/// # Errors
///
/// The values do not fit together, or the file cannot be written.
pub fn build_confirmed(input: &ConfirmedInput) -> Result<DatasheetOutcome, String> {
    use sha2::{Digest, Sha256};
    if input.out.trim().is_empty() {
        return Err("choose where to save the part".into());
    }
    if !input.dry_run && Path::new(&input.out).exists() {
        return Err(format!("{} already exists; choose another name", input.out));
    }
    let bytes = fs::read(&input.pdf).map_err(|e| format!("{}: {e}", input.pdf))?;
    let pages = text_of_regions(&bytes, &input.pin_regions)?.unwrap_or_default();
    let options = BuildOptions {
        pdf_sha256: Some(
            Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
        ),
        ..Default::default()
    };
    let (spec, mut notes) = assemble_confirmed(
        &input.metadata,
        &input.pins,
        &input.package,
        input.footprint.as_ref(),
        &pages,
        &options,
    )?;
    // The checks of the review, before anything is written.
    let mut findings = validate(&spec, &crate::fab());
    findings.extend(kirpitch_gen_3d::fit_findings(&spec));
    let mut failures = Vec::new();
    for f in &findings {
        let line = format!("{}: {}", f.path, f.message);
        if matches!(f.level, Level::Error) {
            failures.push(line);
        } else {
            notes.push(line);
        }
    }
    let mut outcome = DatasheetOutcome {
        view: None,
        models: vec![],
        notes,
        failures,
        usage: "made from confirmed values, no model was asked".into(),
        pins_csv: None,
    };
    if input.dry_run {
        return Ok(outcome);
    }
    if has_errors(&findings) {
        return Ok(outcome);
    }
    if let Some(parent) = Path::new(&input.out)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(&input.out, spec.to_json_string().as_bytes())
        .map_err(|e| format!("{}: {e}", input.out))?;
    outcome.view = Some(open_spec(&input.out)?);
    Ok(outcome)
}
