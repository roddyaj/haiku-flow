mod commands;
mod git;
mod gitlab;
mod scan;
mod store;
mod token;

use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let tokens = Arc::new(token::Tokens::new());
            let warm = tokens.clone();
            std::thread::spawn(move || warm.warm_up());
            app.manage(commands::AppState::load(dir, tokens));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::default_config,
            commands::save_config,
            commands::token_status,
            commands::save_token,
            commands::clear_token,
            commands::list_repos,
            commands::repo_status,
            commands::fetch_repo,
            commands::list_branches,
            commands::checkout_branch,
            commands::pull_repo,
            commands::set_hidden,
            commands::set_manual_issue,
            commands::issue_details,
            commands::list_done,
            commands::mark_done,
            commands::undo_done,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
