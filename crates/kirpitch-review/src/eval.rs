// SPDX-License-Identifier: GPL-3.0-or-later

//! Evaluating how well the models read datasheets with the current prompts.
//!
//! A **case** is a datasheet PDF together with the part it should produce: a
//! golden part of the repository (when its PDF is known) or a part you have
//! reviewed yourself. Running an evaluation builds each case with the models
//! and prompts in the settings, compares the result with the expected part
//! (pins right, package dimensions right, pads right) and saves the run with
//! the exact prompts, models and scores, so runs can be compared and a good
//! set of prompts can be brought back.
//!
//! Runs and cases are kept as JSON files in the `evals` folder of the user's
//! settings folder.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use kirpitch_core::PartSpec;
use kirpitch_models::config::Task;
use kirpitch_models::{
    BuildOptions, Cache, Config, KeyStore, Progress, Transport, build_with_progress, score,
    user_config_dir,
};
use serde::{Deserialize, Serialize};

use crate::{list_examples, now_rfc3339, read_pages};

/// The folder of cases and runs.
pub fn eval_dir() -> PathBuf {
    user_config_dir()
        .unwrap_or_else(|| std::env::temp_dir().join("kirpitch"))
        .join("evals")
}

// ---- Cases -----------------------------------------------------------------------------------

/// A case of the user's own: a reviewed part and its datasheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MyCase {
    /// Name shown in the lists.
    pub name: String,
    /// The reviewed PartSpec, which is the expected answer.
    pub spec: String,
    /// The datasheet PDF.
    pub pdf: String,
    /// A hint such as the package name or the pages.
    #[serde(default)]
    pub hint: String,
}

/// What is saved about cases.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct CaseStore {
    /// The user's own cases.
    #[serde(default)]
    mine: Vec<MyCase>,
    /// Where the PDF of a golden part is, by part name.
    #[serde(default)]
    pdfs: BTreeMap<String, String>,
}

fn load_store(dir: &Path) -> CaseStore {
    fs::read_to_string(dir.join("cases.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_store(dir: &Path, store: &CaseStore) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(dir.join("cases.json"), text).map_err(|e| format!("{}: {e}", dir.display()))
}

/// A case as the screen shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaseView {
    /// Name.
    pub name: String,
    /// `golden` or `mine`.
    pub source: String,
    /// Part number.
    pub mpn: String,
    /// Package name.
    pub package: String,
    /// The datasheet PDF, if known.
    pub pdf: Option<String>,
    /// Whether the PDF is there.
    pub ready: bool,
    /// The hint.
    pub hint: String,
}

/// A case ready to run.
#[derive(Debug, Clone)]
pub struct ResolvedCase {
    /// Name.
    pub name: String,
    /// What the part should come out as.
    pub expected: PartSpec,
    /// The datasheet.
    pub pdf: PathBuf,
    /// A hint for the models.
    pub hint: Option<String>,
}

fn golden_hint(name: &str) -> String {
    let Some(data) = crate::find_testdata() else {
        return String::new();
    };
    fs::read_to_string(data.join("golden").join(name).join("datasheet.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| {
            v.get("package")
                .and_then(|p| p.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// The cases there are: the golden parts and the user's own.
pub fn list_cases() -> Vec<CaseView> {
    list_cases_in(&eval_dir())
}

/// Like [`list_cases`], with the folder given.
pub fn list_cases_in(dir: &Path) -> Vec<CaseView> {
    let store = load_store(dir);
    let mut out = Vec::new();
    for e in list_examples() {
        let pdf = store.pdfs.get(&e.name).cloned().or(e.pdf.clone());
        out.push(CaseView {
            ready: pdf.as_deref().is_some_and(|p| Path::new(p).is_file()),
            hint: golden_hint(&e.name),
            name: e.name,
            source: "golden".into(),
            mpn: e.mpn,
            package: e.package,
            pdf,
        });
    }
    for m in &store.mine {
        let spec = crate::load_spec(Path::new(&m.spec)).ok();
        out.push(CaseView {
            name: m.name.clone(),
            source: "mine".into(),
            mpn: spec
                .as_ref()
                .map_or(String::new(), |s| s.metadata.mpn.clone()),
            package: spec
                .as_ref()
                .map_or(String::new(), |s| s.package.name.clone()),
            ready: spec.is_some() && Path::new(&m.pdf).is_file(),
            pdf: Some(m.pdf.clone()),
            hint: m.hint.clone(),
        });
    }
    out
}

/// Adds a case of the user's own.
///
/// # Errors
///
/// The part cannot be read, the PDF is missing, or the name is used.
pub fn add_case(dir: &Path, name: &str, spec: &str, pdf: &str, hint: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err("give the case a name (no slashes)".into());
    }
    crate::load_spec(Path::new(spec)).map_err(|e| format!("{spec}: {e}"))?;
    if !Path::new(pdf).is_file() {
        return Err(format!("{pdf} is not a file"));
    }
    let mut store = load_store(dir);
    if store.mine.iter().any(|m| m.name == name) || list_examples().iter().any(|e| e.name == name) {
        return Err(format!("there is already a case called `{name}`"));
    }
    store.mine.push(MyCase {
        name: name.into(),
        spec: spec.into(),
        pdf: pdf.into(),
        hint: hint.trim().into(),
    });
    save_store(dir, &store)
}

/// Says where the PDF of a golden part is.
///
/// # Errors
///
/// There is no such golden part or the file is missing.
pub fn set_case_pdf(dir: &Path, name: &str, pdf: &str) -> Result<(), String> {
    if !list_examples().iter().any(|e| e.name == name) {
        return Err(format!("`{name}` is not a golden part"));
    }
    if !Path::new(pdf).is_file() {
        return Err(format!("{pdf} is not a file"));
    }
    let mut store = load_store(dir);
    store.pdfs.insert(name.into(), pdf.into());
    save_store(dir, &store)
}

/// Removes a case of the user's own, or forgets the PDF of a golden part.
///
/// # Errors
///
/// The files cannot be written.
pub fn remove_case(dir: &Path, name: &str) -> Result<(), String> {
    let mut store = load_store(dir);
    store.mine.retain(|m| m.name != name);
    store.pdfs.remove(name);
    save_store(dir, &store)
}

/// Finds the named cases, ready to run.
///
/// # Errors
///
/// A case is unknown, has no PDF, or its part cannot be read.
pub fn resolve_cases(dir: &Path, names: &[String]) -> Result<Vec<ResolvedCase>, String> {
    let store = load_store(dir);
    let mut out = Vec::new();
    for name in names {
        if let Some(m) = store.mine.iter().find(|m| &m.name == name) {
            out.push(ResolvedCase {
                name: name.clone(),
                expected: crate::load_spec(Path::new(&m.spec))?,
                pdf: PathBuf::from(&m.pdf),
                hint: (!m.hint.is_empty()).then(|| m.hint.clone()),
            });
            continue;
        }
        let data = crate::find_testdata()
            .ok_or("the golden parts are only available in a checkout of Kirpitch")?;
        let spec = data.join("golden").join(name).join("partspec.json");
        let pdf = store
            .pdfs
            .get(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| data.join("pdfs").join(format!("{name}.pdf")));
        let hint = golden_hint(name);
        out.push(ResolvedCase {
            name: name.clone(),
            expected: crate::load_spec(&spec).map_err(|e| format!("{name}: {e}"))?,
            pdf,
            hint: (!hint.is_empty()).then_some(hint),
        });
    }
    Ok(out)
}

// ---- Runs ------------------------------------------------------------------------------------

/// What to run.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct EvalInput {
    /// The cases, by name.
    pub cases: Vec<String>,
    /// Read the pin table twice.
    #[serde(default)]
    pub second_read: bool,
    /// A note for the history ("shorter pin prompt").
    #[serde(default)]
    pub label: String,
}

/// The result of one case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaseResult {
    /// The case.
    pub name: String,
    /// `scored`, `failed` (no part came out) or `skipped` (no PDF).
    pub status: String,
    /// Pins that are right.
    pub pins_correct: usize,
    /// Pins expected.
    pub pins_expected: usize,
    /// Pins found that are not expected.
    pub pins_extra: usize,
    /// Package dimensions that are right.
    pub dims_correct: usize,
    /// Package dimensions compared.
    pub dims_expected: usize,
    /// Pads in the right place and size.
    pub pads_correct: usize,
    /// Pads expected.
    pub pads_expected: usize,
    /// Dimensions that differ.
    pub mismatches: Vec<String>,
    /// What went wrong, when it did.
    pub failures: Vec<String>,
    /// Tokens sent and received.
    pub tokens: u64,
    /// Seconds in model calls.
    pub seconds: f64,
}

impl CaseResult {
    fn empty(name: &str, status: &str) -> CaseResult {
        CaseResult {
            name: name.into(),
            status: status.into(),
            pins_correct: 0,
            pins_expected: 0,
            pins_extra: 0,
            dims_correct: 0,
            dims_expected: 0,
            pads_correct: 0,
            pads_expected: 0,
            mismatches: vec![],
            failures: vec![],
            tokens: 0,
            seconds: 0.0,
        }
    }
}

/// Totals of a run, as shares from 0 to 1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Totals {
    /// Pins right over pins expected.
    pub pinout: f64,
    /// Dimensions right over dimensions compared.
    pub footprint: f64,
    /// Pads right over pads expected.
    pub pads: f64,
    /// Tokens used.
    pub tokens: u64,
    /// Seconds in model calls.
    pub seconds: f64,
    /// Cases that gave a part.
    pub scored: usize,
    /// Cases run.
    pub cases: usize,
}

/// The model that did a task in a run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsedModel {
    /// The task.
    pub task: String,
    /// The name in the settings.
    pub name: String,
    /// The model name sent to the server.
    pub model: String,
}

/// A saved run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalRun {
    /// File name without extension; sorts by time.
    pub id: String,
    /// When it started.
    pub when: String,
    /// The note.
    pub label: String,
    /// The code of the prompt set.
    pub prompts_fingerprint: String,
    /// The prompts that were used, by key (all of them, defaults filled in).
    pub prompts: BTreeMap<String, String>,
    /// The models.
    pub models: Vec<UsedModel>,
    /// The pin table was read twice.
    pub second_read: bool,
    /// The cases.
    pub cases: Vec<CaseResult>,
    /// The totals.
    pub totals: Totals,
}

fn ratio(a: usize, b: usize) -> f64 {
    if b == 0 { 1.0 } else { a as f64 / b as f64 }
}

fn totals(cases: &[CaseResult]) -> Totals {
    let sum = |f: fn(&CaseResult) -> usize| cases.iter().map(f).sum::<usize>();
    Totals {
        pinout: ratio(sum(|c| c.pins_correct), sum(|c| c.pins_expected)),
        footprint: ratio(sum(|c| c.dims_correct), sum(|c| c.dims_expected)),
        pads: ratio(sum(|c| c.pads_correct), sum(|c| c.pads_expected)),
        tokens: cases.iter().map(|c| c.tokens).sum(),
        seconds: cases.iter().map(|c| c.seconds).sum(),
        scored: cases.iter().filter(|c| c.status == "scored").count(),
        cases: cases.iter().filter(|c| c.status != "skipped").count(),
    }
}

/// A report on a run in progress.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EvalEvent {
    /// The case.
    pub case: String,
    /// `started` or `finished`.
    pub state: String,
    /// The stage of the build now going on (while started).
    pub stage: String,
    /// The result (when finished).
    pub result: Option<CaseResult>,
}

/// How a background evaluation is going, for the window that asks.
#[derive(Debug, Clone, Serialize)]
pub struct EvalStatus {
    /// Still running.
    pub running: bool,
    /// The reports the window does not have yet.
    pub events: Vec<EvalEvent>,
    /// Why it stopped, when it failed as a whole.
    pub error: Option<String>,
    /// The saved run, when it is finished.
    pub run_id: Option<String>,
}

/// What a run needs besides the cases.
pub struct EvalTools<'a> {
    /// How the servers are reached.
    pub transport: Arc<dyn Transport>,
    /// Where the keys are.
    pub keys: &'a dyn KeyStore,
    /// Hears of each case as it starts, goes on and ends.
    pub on: &'a dyn Fn(&EvalEvent),
    /// Asked between cases; a run that is stopped keeps what it has.
    pub stop: &'a dyn Fn() -> bool,
}

/// Runs the cases with the models and prompts of `config` and saves the run in
/// `dir`.
///
/// # Errors
///
/// No case, or the run cannot be saved.
pub fn run_eval(
    dir: &Path,
    config: &Config,
    cases: &[ResolvedCase],
    input: &EvalInput,
    tools: &EvalTools<'_>,
) -> Result<EvalRun, String> {
    let EvalTools {
        transport,
        keys,
        on,
        stop,
    } = tools;
    if cases.is_empty() {
        return Err("choose at least one case".into());
    }
    let when = now_rfc3339();
    let mut results = Vec::new();
    for case in cases {
        if stop() {
            break;
        }
        on(&EvalEvent {
            case: case.name.clone(),
            state: "started".into(),
            stage: "read".into(),
            result: None,
        });
        let result = run_case(
            config,
            transport.clone(),
            *keys,
            case,
            input,
            &|p: &Progress| {
                on(&EvalEvent {
                    case: case.name.clone(),
                    state: "started".into(),
                    stage: p.stage.to_string(),
                    result: None,
                });
            },
        );
        on(&EvalEvent {
            case: case.name.clone(),
            state: "finished".into(),
            stage: String::new(),
            result: Some(result.clone()),
        });
        results.push(result);
    }
    let models = [Task::Metadata, Task::PinTables, Task::Drawings]
        .iter()
        .filter_map(|t| {
            config.model_for(*t).ok().map(|(name, m)| UsedModel {
                task: t.name().into(),
                name: name.into(),
                model: m.model.clone(),
            })
        })
        .collect();
    let run = EvalRun {
        id: when
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == 'T')
            .collect::<String>(),
        when,
        label: input.label.trim().into(),
        prompts_fingerprint: config.prompts.fingerprint(),
        prompts: kirpitch_models::PROMPT_LIST
            .iter()
            .map(|p| (p.key.to_string(), config.prompts.get(p.key).to_string()))
            .collect(),
        models,
        second_read: input.second_read,
        totals: totals(&results),
        cases: results,
    };
    let runs = dir.join("runs");
    fs::create_dir_all(&runs).map_err(|e| format!("{}: {e}", runs.display()))?;
    let text = serde_json::to_string_pretty(&run).map_err(|e| e.to_string())?;
    let file = runs.join(format!("{}.json", run.id));
    fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(run)
}

fn run_case(
    config: &Config,
    transport: Arc<dyn Transport>,
    keys: &dyn KeyStore,
    case: &ResolvedCase,
    input: &EvalInput,
    on: &dyn Fn(&Progress),
) -> CaseResult {
    let Ok(bytes) = fs::read(&case.pdf) else {
        let mut r = CaseResult::empty(&case.name, "skipped");
        r.failures
            .push(format!("the datasheet {} is not there", case.pdf.display()));
        return r;
    };
    let pages = match read_pages(&bytes) {
        Ok(p) if p.iter().any(|p| !p.lines.is_empty()) => p,
        Ok(_) => {
            let mut r = CaseResult::empty(&case.name, "failed");
            r.failures.push("the PDF has no text".into());
            return r;
        }
        Err(e) => {
            let mut r = CaseResult::empty(&case.name, "failed");
            r.failures.push(e);
            return r;
        }
    };
    let options = BuildOptions {
        hint: case.hint.clone(),
        second_read: input.second_read,
        ..Default::default()
    };
    let report = build_with_progress(
        config,
        transport,
        &Cache::none(),
        &pages,
        &options,
        keys,
        on,
    );
    let mut r = CaseResult::empty(&case.name, "failed");
    r.tokens = report.stats.usage.input_tokens + report.stats.usage.output_tokens;
    r.seconds = report.stats.seconds;
    r.failures = report.failures.clone();
    match &report.spec {
        Some(spec) => {
            let s = score(spec, &case.expected);
            r.status = "scored".into();
            r.pins_correct = s.pins_correct;
            r.pins_expected = s.pins_expected;
            r.pins_extra = s.pins_extra;
            r.dims_correct = s.dimensions_correct;
            r.dims_expected = s.dimensions_expected;
            r.pads_correct = s.pads_correct;
            r.pads_expected = s.pads_expected;
            r.mismatches = s.mismatches;
        }
        None => {
            // A part that did not come out scores nothing of what it should
            // have had.
            r.pins_expected = case.expected.pins.len();
            r.dims_expected = case.expected.package.dimensions.len();
            r.pads_expected = case.expected.land_pattern.pads.len();
        }
    }
    r
}

/// The saved runs, newest first.
pub fn list_runs(dir: &Path) -> Vec<EvalRun> {
    let mut runs: Vec<EvalRun> = fs::read_dir(dir.join("runs"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str(&t).ok())
        .collect();
    runs.sort_by(|a, b| b.id.cmp(&a.id));
    runs
}

/// Deletes a saved run.
///
/// # Errors
///
/// The name is not a run id or the file cannot be removed.
pub fn delete_run(dir: &Path, id: &str) -> Result<(), String> {
    if !id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err("not a run".into());
    }
    let file = dir.join("runs").join(format!("{id}.json"));
    fs::remove_file(&file).map_err(|e| format!("{}: {e}", file.display()))
}
