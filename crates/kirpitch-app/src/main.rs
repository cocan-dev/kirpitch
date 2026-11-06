// SPDX-License-Identifier: GPL-3.0-or-later

//! The Kirpitch desktop app: a window around the review flow.

// Do not open a console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use kirpitch_app::{
    PageImage, SpecView, approve_stage, export_files, now_rfc3339, open_spec, render_datasheet_page,
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

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![open, approve, export, page_image])
        .run(tauri::generate_context!())
        .expect("error while running the Kirpitch app");
}
