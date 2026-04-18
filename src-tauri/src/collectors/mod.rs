pub mod filesystem;
pub mod start_menu;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionReport {
  pub source: String,
  pub mode: Option<String>,
  pub scanned_entries: u32,
  pub indexed_entries: u32,
  pub skipped_entries: u32,
  pub pruned_entries: u32,
  pub errors: Vec<String>,
}

impl CollectionReport {
  pub fn new(source: &str) -> Self {
    Self {
      source: source.to_string(),
      mode: None,
      scanned_entries: 0,
      indexed_entries: 0,
      skipped_entries: 0,
      pruned_entries: 0,
      errors: Vec::new(),
    }
  }
}
