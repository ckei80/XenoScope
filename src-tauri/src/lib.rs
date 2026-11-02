mod database;
#[allow(dead_code)]
mod research_assessment;
#[allow(dead_code)]
mod research_candidate;
#[allow(dead_code)]
mod research_candidate_read;
#[allow(dead_code)]
mod research_dataset;
mod research_dataset_import;
mod research_dataset_read;
#[allow(dead_code)]
mod research_decision;
mod research_records;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::Manager;

            let database_path = app.path().app_local_data_dir()?.join("xenoscope.sqlite3");
            let database = database::Database::open(&database_path).map_err(|error| {
                std::io::Error::other(format!(
                    "Failed to initialize XenoScope database at {}: {error}",
                    database_path.display()
                ))
            })?;

            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            greet,
            research_records::create_source,
            research_records::get_source,
            research_records::create_claim,
            research_records::get_claim,
            research_dataset_import::import_bundled_research_dataset,
            research_dataset_read::get_bundled_research_case,
            research_dataset_read::get_research_case_library,
            research_dataset_read::get_research_case,
            research_dataset_read::get_research_case_import,
            research_dataset_read::get_research_dataset_import_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
