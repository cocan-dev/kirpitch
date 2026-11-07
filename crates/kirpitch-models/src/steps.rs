// SPDX-License-Identifier: GPL-3.0-or-later

//! Reading a datasheet one step at a time, with a person confirming each result.
//!
//! A step is read from the text the person pointed at, the result is shown as
//! values that can be changed (or typed in when nothing could be read), and only
//! confirmed values go on. The part is then assembled from the confirmed values
//! alone, with no further call to a model.
//!
//! The values travel as JSON in the shape the models answer in, so a value that
//! a model wrote and a value a person typed go through the same checks.

use std::cell::RefCell;
use std::sync::Arc;

use serde::Serialize;
use serde_json::{Value, json};

use crate::build::{
    BuildOptions, Progress, StageState, apply_footprint, assemble, template_from_answer,
};
use crate::client::Client;
use crate::config::{Config, Task};
use crate::keys::KeyStore;
use crate::prompts::TYPE_HELP;
use crate::tasks::{
    Cache, CallRecord, Ctx, FootprintAnswer, MetadataAnswer, PackageAnswer, PageLines, Pinned,
    PinsResult, Stats, extract_footprint, extract_metadata, extract_package, extract_pins,
    footprint_from_json, narrow_to_package, package_from_json, pins_from_json, read_pins,
};
use crate::transport::Transport;

/// The steps of a reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Part number, maker and description.
    Metadata,
    /// The pin table.
    Pins,
    /// The package outline.
    Package,
    /// The recommended footprint.
    Footprint,
}

impl Step {
    /// The step called `name` (`metadata`, `pins`, `package`, `footprint`).
    ///
    /// # Errors
    ///
    /// The name is not a step.
    pub fn from_name(name: &str) -> Result<Step, String> {
        match name {
            "metadata" => Ok(Step::Metadata),
            "pins" => Ok(Step::Pins),
            "package" => Ok(Step::Package),
            "footprint" => Ok(Step::Footprint),
            other => Err(format!("`{other}` is not a step")),
        }
    }

    fn task(self) -> Task {
        match self {
            Step::Metadata => Task::Metadata,
            Step::Pins => Task::PinTables,
            Step::Package | Step::Footprint => Task::Drawings,
        }
    }
}

/// What reading a step gave.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepRun {
    /// The values, in the shape the checks and the assembly read.
    pub value: Value,
    /// Things to look at.
    pub notes: Vec<String>,
}

fn pins_json(result: &PinsResult) -> Value {
    json!({
        "pins": result.pins.iter().map(|p| json!({
            "number": p.pin.number,
            "name": p.pin.name,
            "type": p.pin.electrical_type.as_str(),
            "page": p.page,
            "bbox": p.bbox,
            "confidence": p.confidence,
            "grounded": p.grounded,
        })).collect::<Vec<_>>()
    })
}

/// Reads one step from `pages`, which holds exactly the text to read (when
/// `given` is false the pages are searched for what the step needs, as in an
/// automatic build). `on` hears of each call to a model.
///
/// # Errors
///
/// No model is configured for the step, or the answer cannot be used.
#[allow(clippy::too_many_arguments)]
pub fn run_step(
    config: &Config,
    transport: Arc<dyn Transport>,
    cache: &Cache,
    keys: &dyn KeyStore,
    step: Step,
    pages: &[PageLines],
    given: bool,
    hint: Option<&str>,
    second_read: bool,
    on: &dyn Fn(&Progress),
) -> Result<StepRun, String> {
    let (name, model) = config.model_for(step.task()).map_err(|e| e.to_string())?;
    let client = Client::connect(name, model, transport, keys).map_err(|e| e.to_string())?;
    let stage = match step {
        Step::Metadata => "metadata",
        Step::Pins => "pins",
        Step::Package => "package",
        Step::Footprint => "footprint",
    };
    let obs = |c: &CallRecord| {
        on(&Progress {
            stage,
            state: StageState::Call,
            detail: String::new(),
            call: Some(c.clone()),
        });
    };
    let raw = RefCell::new(None);
    let ctx = Ctx {
        prompts: &config.prompts,
        options: &config.extraction,
        obs: &obs,
        raw: Some(&raw),
        pinned: Pinned {
            metadata: given && step == Step::Metadata,
            pins: given && step == Step::Pins,
            package: given && step == Step::Package,
            footprint: given && step == Step::Footprint,
        },
    };
    let mut stats = Stats::default();
    let e = |e: crate::tasks::TaskError| e.to_string();
    match step {
        Step::Pins => {
            let result = extract_pins(&client, cache, &mut stats, pages, hint, second_read, &ctx)
                .map_err(e)?;
            Ok(StepRun {
                value: pins_json(&result),
                notes: result.notes,
            })
        }
        other => {
            match other {
                Step::Metadata => {
                    extract_metadata(&client, cache, &mut stats, pages, &ctx).map_err(e)?;
                }
                Step::Package => {
                    extract_package(&client, cache, &mut stats, pages, hint, &ctx).map_err(e)?;
                }
                _ => {
                    extract_footprint(&client, cache, &mut stats, pages, hint, &ctx).map_err(e)?;
                }
            }
            let value = raw
                .borrow_mut()
                .take()
                .ok_or("the model gave no answer to keep")?;
            Ok(StepRun {
                value,
                notes: vec![],
            })
        }
    }
}

fn text_of(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Cuts the pins of one package out of the pins of a table that covers several
/// (see [`narrow_to_package`]), with their number in that package. No model is
/// asked.
///
/// # Errors
///
/// The values are not pins, or the table has no column that fits `want` pins.
pub fn narrow_pins(value: &Value, pages: &[PageLines], want: usize) -> Result<StepRun, String> {
    let pins = pins_from_json(value)?;
    let narrowed = narrow_to_package(&pins, pages, want)?;
    let default_page = pages.first().map_or(1, |p| p.page);
    let result = read_pins(narrowed.pins, pages, default_page, false);
    let mut notes = narrowed.notes;
    notes.extend(result.notes.iter().cloned());
    Ok(StepRun {
        value: pins_json(&result),
        notes,
    })
}

/// Reads confirmed metadata.
///
/// # Errors
///
/// A field is empty.
pub fn metadata_from_value(v: &Value) -> Result<MetadataAnswer, String> {
    let get = |k: &str| text_of(v, k).ok_or_else(|| format!("`{k}` is empty"));
    Ok(MetadataAnswer {
        mpn: get("mpn")?,
        manufacturer: get("manufacturer")?,
        description: get("description")?,
    })
}

/// Checks the values of a step, typed in or read, by the same rules the models'
/// answers go through, and returns them in a normal form with notes to look at.
/// `pages` is the text the pins are looked for in.
///
/// # Errors
///
/// The values cannot be used; the message says what is wrong.
pub fn check_step(step: Step, value: &Value, pages: &[PageLines]) -> Result<StepRun, String> {
    match step {
        Step::Metadata => {
            let m = metadata_from_value(value)?;
            Ok(StepRun {
                value: json!({"mpn": m.mpn, "manufacturer": m.manufacturer, "description": m.description}),
                notes: vec![],
            })
        }
        Step::Pins => {
            let pins = pins_from_json(value)?;
            for p in &pins {
                if p.number.trim().is_empty() || p.name.trim().is_empty() {
                    return Err(format!(
                        "pin `{}` `{}` needs a number and a name",
                        p.number, p.name
                    ));
                }
            }
            let default_page = pages.first().map_or(1, |p| p.page);
            let result = read_pins(pins, pages, default_page, true);
            Ok(StepRun {
                value: pins_json(&result),
                notes: result.notes,
            })
        }
        Step::Package => {
            let a = package_from_json(value)?;
            let mut notes = Vec::new();
            if a.family == "custom" {
                notes.push(
                    "this package has no template: its pads come from the recommended footprint"
                        .to_string(),
                );
            }
            Ok(StepRun {
                value: value.clone(),
                notes,
            })
        }
        Step::Footprint => {
            footprint_from_json(value)?;
            Ok(StepRun {
                value: value.clone(),
                notes: vec![],
            })
        }
    }
}

/// A pad, for drawing the footprint that is about to be made.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreviewPad {
    /// Pad number.
    pub number: String,
    /// Centre x.
    pub x: f64,
    /// Centre y (down).
    pub y: f64,
    /// Size along x.
    pub size_x: f64,
    /// Size along y.
    pub size_y: f64,
    /// `rect`, `roundrect`, `oval` or `circle`.
    pub shape: String,
}

/// The footprint that the confirmed values make.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FootprintPreview {
    /// The name of the footprint.
    pub name: String,
    /// Body size along x.
    pub body_width: f64,
    /// Body size along y.
    pub body_length: f64,
    /// Pin count of the package.
    pub pin_count: u32,
    /// The pads.
    pub pads: Vec<PreviewPad>,
    /// Where the pads come from, and how they differ from the calculated ones.
    pub notes: Vec<String>,
}

/// Makes the footprint from a package and, when given, the recommended
/// footprint, so it can be looked at before it is confirmed.
///
/// # Errors
///
/// The values cannot make a footprint.
pub fn preview_footprint(
    package: &Value,
    footprint: Option<&Value>,
) -> Result<FootprintPreview, String> {
    let a: PackageAnswer = package_from_json(package)?;
    let fp: Option<FootprintAnswer> = footprint.map(footprint_from_json).transpose()?;
    let mut template = template_from_answer(&a, fp.as_ref())?;
    let mut notes = Vec::new();
    if let Some(f) = &fp {
        if a.family == "custom" {
            notes.push("the pads are the ones of the recommended footprint".into());
        } else {
            let changes = apply_footprint(&mut template, f);
            if changes.is_empty() {
                notes.push(
                    "the recommended footprint gave nothing to change: the pads are calculated"
                        .into(),
                );
            }
            notes.extend(changes);
        }
    } else if a.family != "custom" {
        notes.push("the pads are calculated from the package dimensions (IPC-7351B)".into());
    }
    let shape = |s: kirpitch_core::partspec::PadShape| {
        match s {
            kirpitch_core::partspec::PadShape::Rect => "rect",
            kirpitch_core::partspec::PadShape::RoundRect => "roundrect",
            kirpitch_core::partspec::PadShape::Oval => "oval",
            kirpitch_core::partspec::PadShape::Circle => "circle",
        }
        .to_string()
    };
    Ok(FootprintPreview {
        name: template.package.name.clone(),
        body_width: template.package.body.width.get(),
        body_length: template.package.body.length.get(),
        pin_count: template.package.pin_count,
        pads: template
            .land_pattern
            .pads
            .iter()
            .map(|p| PreviewPad {
                number: p.number.clone(),
                x: p.x.get(),
                y: p.y.get(),
                size_x: p.size_x.get(),
                size_y: p.size_y.get(),
                shape: shape(p.shape),
            })
            .collect(),
        notes,
    })
}

/// Makes the part from confirmed values only. `pages` is the text the pins are
/// traced back to (it may be empty).
///
/// # Errors
///
/// A value is not usable, or the pieces do not fit together (for example the
/// pin table and the package disagree on the number of pins).
pub fn assemble_confirmed(
    metadata: &Value,
    pins: &Value,
    package: &Value,
    footprint: Option<&Value>,
    pages: &[PageLines],
    options: &BuildOptions,
) -> Result<(kirpitch_core::partspec::PartSpec, Vec<String>), String> {
    let m = metadata_from_value(metadata)?;
    let pin_list = pins_from_json(pins)?;
    let default_page = pages.first().map_or(1, |p| p.page);
    let pins = read_pins(pin_list, pages, default_page, true);
    let package = package_from_json(package)?;
    let footprint = footprint.map(footprint_from_json).transpose()?;
    assemble(
        &m,
        &pins,
        &package,
        footprint.as_ref(),
        default_page,
        options,
    )
}

/// The electrical types a pin may have, for forms.
pub fn pin_types() -> Vec<&'static str> {
    TYPE_HELP
        .split(',')
        .map(|t| t.trim().split(' ').next().unwrap_or(""))
        .filter(|t| !t.is_empty())
        .collect()
}
