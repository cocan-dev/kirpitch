// SPDX-License-Identifier: GPL-3.0-or-later

//! The Kirpitch desktop app: a window around the review flow.

// Do not open a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kirpitch_app::{
    ImportOutcome, PageImage, RegionTable, SpecView, approve_stage, export_files, import_pins,
    now_rfc3339, open_spec, read_region, render_datasheet_page,
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            open,
            approve,
            export,
            page_image,
            region_table,
            pins_from_table
        ])
        .run(tauri::generate_context!())
        .expect("error while running the Kirpitch app");
}
