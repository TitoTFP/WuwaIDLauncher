// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(test))]
fn main() {
    wuwaid_launcher_lib::run::<tauri::Wry>(tauri::generate_context!());
}

#[cfg(test)]
fn main() {}
