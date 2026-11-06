// SPDX-License-Identifier: GPL-3.0-or-later

//! The Kirpitch desktop app: a window around the review flow.

// Do not open a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kirpitch_models::{CurlTransport, FoundModel, OsKeyStore, active_config_path};
use kirpitch_review::{
    DatasheetInput, DatasheetOutcome, Example, FindInput, ImportOutcome, ModelTest, NewPartInput,
    PageImage, PinInput, RegionTable, SettingsInput, SettingsView, SpecView, approve_stage,
    build_from_datasheet, export_files, import_pins, list_examples, new_part, now_rfc3339,
    open_example, open_spec, read_region, render_datasheet_page, save_copy, set_pins,
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

#[tauri::command]
fn page_image(pdf: String, page: u32, width: u32) -> Result<PageImage, String> {
    render_datasheet_page(pdf.trim(), page, width)
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
    tauri::async_runtime::spawn_blocking(move || build_from_datasheet(&input))
        .await
        .map_err(|e| e.to_string())?
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
            pick_file,
            pick_save,
            pick_folder,
            get_settings,
            put_settings,
            put_api_key,
            remove_api_key,
            check_model,
            find_models
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Kirpitch app");
}
