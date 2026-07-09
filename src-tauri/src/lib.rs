mod commands;
mod config_store;
mod cover_generator;
mod invoice_parser;
mod pdf_merge;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            get_initial_paths,
            pick_directory,
            open_directory,
            list_pdfs,
            import_invoices,
            read_mapping,
            write_mapping,
            import_mapping,
            export_mapping,
            get_mapping_path,
            read_category,
            merge_pdfs,
            debug_pdf,
            generate_cover_pdf,
            generate_ledger_pdf,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
