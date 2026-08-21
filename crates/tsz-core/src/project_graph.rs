//! Configuration inheritance and project-reference metadata.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Stable ordinal within one resolved project graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ProjectConfigId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectReference {
    pub owner: ProjectConfigId,
    pub path: PathBuf,
    pub original_path: String,
    /// Byte span of the reference object in the owning JSONC source.
    pub source_start: u32,
    pub source_length: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub id: ProjectConfigId,
    pub path: PathBuf,
    pub extends: Vec<ProjectConfigId>,
    pub references: Vec<ProjectReference>,
}

/// Entry configuration plus the configurations it inherits from.
///
/// Project references are metadata edges. An ordinary project compilation
/// does not union the referenced project's roots into the entry program.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectGraph {
    pub entry: Option<ProjectConfigId>,
    pub configs: Vec<ProjectConfig>,
}

impl ProjectGraph {
    #[must_use]
    pub const fn config_count(&self) -> usize {
        self.configs.len()
    }

    #[must_use]
    pub fn reference_count(&self) -> usize {
        self.entry_config()
            .map_or(0, |config| config.references.len())
    }

    #[must_use]
    pub fn entry_config(&self) -> Option<&ProjectConfig> {
        let entry = self.entry?;
        self.configs.get(entry.0 as usize)
    }

    pub(crate) fn add_config(&mut self, path: PathBuf) -> ProjectConfigId {
        let id = ProjectConfigId(self.configs.len() as u32);
        self.configs.push(ProjectConfig {
            id,
            path,
            extends: Vec::new(),
            references: Vec::new(),
        });
        id
    }

    pub(crate) fn config_mut(&mut self, id: ProjectConfigId) -> &mut ProjectConfig {
        &mut self.configs[id.0 as usize]
    }
}
