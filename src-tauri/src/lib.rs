#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri::Manager;

mod clipboard;

/// Run the Clipbox desktop application.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database_directory = app.path().app_data_dir()?;
            std::fs::create_dir_all(&database_directory)?;

            let database_path = database_directory.join("clipbox.sqlite3");
            clipboard::start(database_path);

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Clipbox");
}
