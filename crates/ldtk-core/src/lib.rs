mod autotile;
pub mod clipboard;
mod definitions;
mod edit;
pub mod enum_sync;
pub mod exports;
mod field_edit;
mod grid_presence;
mod history;
pub mod imports;
mod naming;
mod project;
mod selection;
mod selection_group;
mod workflow;

pub use autotile::{
    DeferredAutoLayers, auto_rule_pattern_is_symmetric, auto_rule_perlin_value, trim_auto_rule_pattern,
};
pub use definitions::{BakeMode, TilesetRemap};
pub use edit::{EntityPlacement, TilePaint};
pub use field_edit::FieldArrayEdit;
pub use history::History;
pub use project::{
    BackupInfo, EntityMatch, ExternalLevels, FilesystemSnapshot, LevelKey, Project, SaveReport,
    SaveStage,
};
pub use selection_group::{SelectionDragReport, selection_group_is_entity_only};
pub use workflow::{constrained_entity_size, snapped_entity_position};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Invalid JSON in {path}: {source}")]
    Json {
        path: std::path::PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("Invalid LDtk data: {0}")]
    Invalid(String),
    #[error("Unsupported operation: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, Error>;
