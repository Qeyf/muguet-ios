// Exercise the production importer without Apple's platform-only dependencies.
include!("../../src/import.rs");

#[cfg(test)]
mod config {
    pub fn game_dir() -> std::path::PathBuf { std::env::temp_dir().join("muguet-import-harness-unused") }
    pub fn game_ready() -> bool { false }
}

#[cfg(test)]
fn has_game_code() -> bool { false }
