use crate::codex_protection::{self, Editor, Snapshot};
use crate::store::AppState;

#[tauri::command]
pub fn get_codex_protection() -> Result<Snapshot, String> {
    codex_protection::snapshot().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_codex_protection(
    paths: Vec<Vec<String>>,
    expected_revision: String,
) -> Result<String, String> {
    codex_protection::save_rules(paths, &expected_revision).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_codex_protected_editor() -> Result<Editor, String> {
    codex_protection::read_editor().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_codex_protected_editor(
    state: tauri::State<'_, AppState>,
    config_path: String,
    text: String,
    expected_revision: Option<String>,
) -> Result<(), String> {
    codex_protection::save_editor(&state.db, &config_path, text, expected_revision)
        .map_err(|e| e.to_string())
}
