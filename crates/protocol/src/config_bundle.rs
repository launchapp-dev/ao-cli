use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Represents a bundle of project configuration files to be pushed to cloud.
/// Includes workflow YAML files and project configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigBundle {
    /// Config files indexed by relative path from project root
    pub files: BTreeMap<String, String>,
}

impl ConfigBundle {
    /// Create a new empty config bundle
    pub fn new() -> Self {
        Self { files: BTreeMap::new() }
    }

    /// Add a file to the bundle
    pub fn add_file(&mut self, path: String, content: String) {
        self.files.insert(path, content);
    }

    /// Check if bundle has any files
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Get number of files in bundle
    pub fn file_count(&self) -> usize {
        self.files.len()
    }
}

impl Default for ConfigBundle {
    fn default() -> Self {
        Self::new()
    }
}
