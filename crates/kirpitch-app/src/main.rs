// SPDX-License-Identifier: GPL-3.0-or-later

//! The Kirpitch desktop app: a window around the review flow.

// Do not open a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kirpitch_models::{CurlTransport, FoundModel, OsKeyStore, active_config_path};
use kirpitch_review::{
    CaseView, DatasheetInput, DatasheetOutcome, EvalEvent, EvalInput, EvalRun, EvalStatus,
    EvalTools, Example, FindInput, ImportOutcome, ModelTest, NewPartInput, PageImage, PinInput,
    RegionTable, SettingsInput, SettingsView, SpecView, approve_stage,
    build_from_datasheet_with_progress, export_files, import_pins, list_examples, new_part,
    now_rfc3339, open_example, open_spec, read_region, render_datasheet_page, save_copy, set_pins,
};

#[tauri::command]
fn open(path: String) -> Result<SpecView, String> {
    open_spec(path.trim())
}

#[tauri::command]
fn approve(path: String, stage: String, note: Option<String>) -> Result<SpecView, String> {
    approve_stage(path.trim(), &stage, note, now_rfc3339())
}

#[tauri::command]
fn export(path: String, out_dir: String, target: Option<u32>) -> Result<Vec<String>, String> {
    export_files(path.trim(), out_dir.trim(), target)
}

/// Rendering a page takes a moment, so it runs off the thread of the window
/// (the viewer asks for many pages while you scroll).
#[tauri::command]
async fn page_image(pdf: String, page: u32, width: u32) -> Result<PageImage, String> {
    tauri::async_runtime::spawn_blocking(move || render_datasheet_page(pdf.trim(), page, width))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn region_table(pdf: String, page: u32, region: [f64; 4]) -> Result<RegionTable, String> {
    read_region(pdf.trim(), page, region)
}

#[tauri::command]
fn pins_from_table(
    path: String,
    tsv: String,
    page: u32,
    region: Option<[f64; 4]>,
) -> Result<ImportOutcome, String> {
    import_pins(path.trim(), &tsv, page, region)
}

#[tauri::command]
fn save_pins(path: String, pins: Vec<PinInput>) -> Result<SpecView, String> {
    set_pins(path.trim(), pins)
}

#[tauri::command]
fn examples() -> Vec<Example> {
    list_examples()
}

#[tauri::command]
fn open_example_part(name: String, fresh: bool) -> Result<SpecView, String> {
    open_example(&name, fresh)
}

#[tauri::command]
fn create_part(input: NewPartInput) -> Result<SpecView, String> {
    new_part(&input)
}

#[tauri::command]
fn copy_part(path: String, dest: String) -> Result<SpecView, String> {
    save_copy(path.trim(), dest.trim())
}

/// Reading a datasheet with a model takes a while, so it runs off the thread
/// of the window.
#[tauri::command]
async fn build_part(input: DatasheetInput) -> Result<DatasheetOutcome, String> {
    if let Ok(mut log) = BUILD_LOG.lock() {
        log.clear();
    }
    tauri::async_runtime::spawn_blocking(move || {
        build_from_datasheet_with_progress(&input, &|progress| {
            if let Ok(mut log) = BUILD_LOG.lock() {
                log.push(progress.clone());
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What the build now running has reported so far. The window asks for it a
/// few times a second; `since` is how many reports it already has.
static BUILD_LOG: std::sync::Mutex<Vec<kirpitch_models::Progress>> =
    std::sync::Mutex::new(Vec::new());

#[tauri::command]
fn build_progress(since: usize) -> Vec<kirpitch_models::Progress> {
    BUILD_LOG
        .lock()
        .map(|log| log.iter().skip(since).cloned().collect())
        .unwrap_or_default()
}

/// What kind of file a dialog asks for.
fn filter(kind: &str) -> (&'static str, &'static [&'static str]) {
    match kind {
        "pdf" => ("Datasheet PDF", &["pdf"]),
        "toml" => ("Model settings", &["toml"]),
        _ => ("PartSpec", &["json"]),
    }
}

fn dialog(start_dir: &Option<String>) -> rfd::AsyncFileDialog {
    let mut d = rfd::AsyncFileDialog::new();
    if let Some(dir) = start_dir
        .as_deref()
        .filter(|d| std::path::Path::new(d).is_dir())
    {
        d = d.set_directory(dir);
    }
    d
}

#[tauri::command]
async fn pick_file(kind: String, start_dir: Option<String>) -> Option<String> {
    let (name, ext) = filter(&kind);
    let picked = dialog(&start_dir).add_filter(name, ext).pick_file().await?;
    Some(picked.path().display().to_string())
}

#[tauri::command]
async fn pick_save(kind: String, name: String, start_dir: Option<String>) -> Option<String> {
    let (label, ext) = filter(&kind);
    let picked = dialog(&start_dir)
        .add_filter(label, ext)
        .set_file_name(&name)
        .save_file()
        .await?;
    Some(picked.path().display().to_string())
}

#[tauri::command]
async fn pick_folder(start_dir: Option<String>) -> Option<String> {
    let picked = dialog(&start_dir).pick_folder().await?;
    Some(picked.path().display().to_string())
}

#[tauri::command]
fn get_settings() -> SettingsView {
    kirpitch_review::current_settings()
}

#[tauri::command]
fn put_settings(input: SettingsInput) -> Result<SettingsView, String> {
    kirpitch_review::save_settings(&active_config_path(), &input, &OsKeyStore)
}

#[tauri::command]
fn put_api_key(name: String, key: String) -> Result<SettingsView, String> {
    kirpitch_review::set_api_key(&active_config_path(), &name, &key, &OsKeyStore)
}

#[tauri::command]
fn remove_api_key(name: String) -> Result<SettingsView, String> {
    kirpitch_review::delete_api_key(&active_config_path(), &name, &OsKeyStore)
}

/// Testing a model sends requests and can take a while, so it runs off the
/// thread of the window.
#[tauri::command]
async fn check_model(name: String) -> Result<ModelTest, String> {
    tauri::async_runtime::spawn_blocking(move || {
        kirpitch_review::test_model(
            &active_config_path(),
            &name,
            std::sync::Arc::new(CurlTransport),
            &OsKeyStore,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn find_models(input: FindInput) -> Result<Vec<FoundModel>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        kirpitch_review::find_models(&input, &CurlTransport, &OsKeyStore)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Evaluating prompts and models ------------------------------------------------------

/// What a running (or finished) evaluation has reported.
#[derive(Default)]
struct EvalState {
    running: bool,
    log: Vec<EvalEvent>,
    error: Option<String>,
    run_id: Option<String>,
}

static EVAL: std::sync::Mutex<EvalState> = std::sync::Mutex::new(EvalState {
    running: false,
    log: Vec::new(),
    error: None,
    run_id: None,
});
static EVAL_STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
fn eval_cases() -> Vec<CaseView> {
    kirpitch_review::list_cases()
}

#[tauri::command]
fn eval_add_case(
    name: String,
    spec: String,
    pdf: String,
    hint: String,
) -> Result<Vec<CaseView>, String> {
    kirpitch_review::add_case(&kirpitch_review::eval_dir(), &name, &spec, &pdf, &hint)?;
    Ok(kirpitch_review::list_cases())
}

#[tauri::command]
fn eval_set_pdf(name: String, pdf: String) -> Result<Vec<CaseView>, String> {
    kirpitch_review::set_case_pdf(&kirpitch_review::eval_dir(), &name, &pdf)?;
    Ok(kirpitch_review::list_cases())
}

#[tauri::command]
fn eval_remove_case(name: String) -> Result<Vec<CaseView>, String> {
    kirpitch_review::remove_case(&kirpitch_review::eval_dir(), &name)?;
    Ok(kirpitch_review::list_cases())
}

#[tauri::command]
fn eval_runs() -> Vec<EvalRun> {
    kirpitch_review::list_runs(&kirpitch_review::eval_dir())
}

#[tauri::command]
fn eval_delete(id: String) -> Result<Vec<EvalRun>, String> {
    kirpitch_review::delete_run(&kirpitch_review::eval_dir(), &id)?;
    Ok(kirpitch_review::list_runs(&kirpitch_review::eval_dir()))
}

/// Starts an evaluation in the background; the window asks `eval_status` how it
/// goes.
#[tauri::command]
fn eval_start(input: EvalInput) -> Result<(), String> {
    let dir = kirpitch_review::eval_dir();
    let config = kirpitch_models::Config::discover()
        .map_err(|e| e.to_string())?
        .ok_or("no language model is configured: add one in Settings first")?;
    let cases = kirpitch_review::resolve_cases(&dir, &input.cases)?;
    {
        let mut state = EVAL.lock().map_err(|e| e.to_string())?;
        if state.running {
            return Err("an evaluation is already running".into());
        }
        *state = EvalState {
            running: true,
            ..EvalState::default()
        };
    }
    EVAL_STOP.store(false, std::sync::atomic::Ordering::SeqCst);
    std::thread::spawn(move || {
        let on = |event: &EvalEvent| {
            if let Ok(mut state) = EVAL.lock() {
                state.log.push(event.clone());
            }
        };
        let tools = EvalTools {
            transport: std::sync::Arc::new(CurlTransport),
            keys: &OsKeyStore,
            on: &on,
            stop: &|| EVAL_STOP.load(std::sync::atomic::Ordering::SeqCst),
        };
        let result = kirpitch_review::run_eval(&dir, &config, &cases, &input, &tools);
        if let Ok(mut state) = EVAL.lock() {
            state.running = false;
            match result {
                Ok(run) => state.run_id = Some(run.id),
                Err(e) => state.error = Some(e),
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn eval_stop() {
    EVAL_STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// The reports of the evaluation since `since` (how many the window has).
#[tauri::command]
fn eval_status(since: usize) -> EvalStatus {
    match EVAL.lock() {
        Ok(state) => EvalStatus {
            running: state.running,
            events: state.log.iter().skip(since).cloned().collect(),
            error: state.error.clone(),
            run_id: state.run_id.clone(),
        },
        Err(_) => EvalStatus {
            running: false,
            events: vec![],
            error: Some("the evaluation state is not available".into()),
            run_id: None,
        },
    }
}

/// Pages the program would look at for each step, to suggest them.
#[tauri::command]
async fn suggest_pages(pdf: String, hint: String) -> Result<kirpitch_review::Suggestions, String> {
    tauri::async_runtime::spawn_blocking(move || {
        kirpitch_review::suggest_pages_for(pdf.trim(), &hint)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// How much text the chosen places hold, against what the search would send.
#[tauri::command]
async fn preview_guide(
    pdf: String,
    guide: kirpitch_review::GuideInput,
    hint: String,
) -> Result<kirpitch_review::Preview, String> {
    tauri::async_runtime::spawn_blocking(move || {
        kirpitch_review::preview_selection(pdf.trim(), &guide, &hint)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ---- Reading one step at a time ---------------------------------------------------------

/// Reads one step from the place pointed at. The calls to the model are reported
/// in the same log that `build_progress` serves, so the window shows them.
#[tauri::command]
async fn read_step(
    input: kirpitch_review::ReadStepInput,
) -> Result<kirpitch_models::steps::StepRun, String> {
    if let Ok(mut log) = BUILD_LOG.lock() {
        log.clear();
    }
    tauri::async_runtime::spawn_blocking(move || {
        kirpitch_review::read_step(&input, &|progress| {
            if let Ok(mut log) = BUILD_LOG.lock() {
                log.push(progress.clone());
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Checks values that were typed or changed, by the rules of the models' answers.
#[tauri::command]
fn check_step_values(
    pdf: String,
    step: String,
    value: kirpitch_review::JsonValue,
    regions: Vec<kirpitch_review::RegionInput>,
) -> Result<kirpitch_models::steps::StepRun, String> {
    kirpitch_review::check_step_values(pdf.trim(), &step, &value, &regions)
}

/// Keeps the pins of one package from the pins of a table for several packages.
#[tauri::command]
async fn narrow_pins(
    pdf: String,
    regions: Vec<kirpitch_review::RegionInput>,
    value: kirpitch_review::JsonValue,
    want: usize,
) -> Result<kirpitch_models::steps::StepRun, String> {
    tauri::async_runtime::spawn_blocking(move || {
        kirpitch_review::narrow_pins_values(pdf.trim(), &regions, &value, want)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The footprint that the values make, to look at.
#[tauri::command]
fn preview_footprint(
    package: kirpitch_review::JsonValue,
    footprint: Option<kirpitch_review::JsonValue>,
) -> Result<kirpitch_models::steps::FootprintPreview, String> {
    kirpitch_review::preview_footprint_values(&package, footprint.as_ref())
}

/// Makes the part from confirmed values (or only checks, in a dry run).
#[tauri::command]
async fn build_confirmed(
    input: kirpitch_review::ConfirmedInput,
) -> Result<DatasheetOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || kirpitch_review::build_confirmed(&input))
        .await
        .map_err(|e| e.to_string())?
}

/// The parts found in some folders, for the "My parts" screen.
#[tauri::command]
async fn list_parts(dirs: Vec<String>) -> Vec<kirpitch_review::PartSummary> {
    let mut dirs = dirs;
    // The default folder for parts from Settings is always looked in.
    if let Some(parts) = kirpitch_review::user_folders().parts {
        dirs.push(parts);
    }
    tauri::async_runtime::spawn_blocking(move || kirpitch_review::list_parts(&dirs))
        .await
        .unwrap_or_default()
}

/// Gives a part file another name (same folder, same ending); returns the new path.
#[tauri::command]
fn rename_part(path: String, new_name: String) -> Result<String, String> {
    kirpitch_review::rename_part(&path, &new_name)
}

/// Deletes a part file (not the KiCad files made from it).
#[tauri::command]
fn delete_part(path: String) -> Result<(), String> {
    kirpitch_review::delete_part(&path)
}

/// The name a part can be saved under: the one given, or the same with -2, -3, ...
/// when a file is already there.
#[tauri::command]
fn unused_path(path: String) -> String {
    kirpitch_review::unused_path(&path)
}

/// What the server says about one model: its context window and abilities.
#[tauri::command]
async fn model_details(input: FindInput) -> Result<kirpitch_models::ModelDetails, String> {
    tauri::async_runtime::spawn_blocking(move || {
        kirpitch_review::find_model_details(&input, &CurlTransport, &OsKeyStore)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            open,
            approve,
            export,
            page_image,
            region_table,
            pins_from_table,
            save_pins,
            examples,
            open_example_part,
            create_part,
            copy_part,
            build_part,
            build_progress,
            pick_file,
            pick_save,
            pick_folder,
            get_settings,
            put_settings,
            put_api_key,
            remove_api_key,
            check_model,
            find_models,
            model_details,
            suggest_pages,
            read_step,
            check_step_values,
            narrow_pins,
            preview_footprint,
            build_confirmed,
            unused_path,
            list_parts,
            rename_part,
            delete_part,
            preview_guide,
            eval_cases,
            eval_add_case,
            eval_set_pdf,
            eval_remove_case,
            eval_runs,
            eval_delete,
            eval_start,
            eval_stop,
            eval_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Kirpitch app");
}
