//! Built-in skills.

pub mod code_analysis;
pub mod data_transform;
pub mod file_operations;
pub mod web_research;

pub use code_analysis::CodeAnalysisSkill;
pub use data_transform::DataTransformSkill;
pub use file_operations::FileOperationsSkill;
pub use web_research::WebResearchSkill;

use crate::{Result, SkillRegistry};
use std::sync::Arc;

/// Register all built-in skills.
pub async fn register_builtin(registry: &SkillRegistry) -> Result<()> {
    registry.register(Arc::new(WebResearchSkill::new())).await?;
    registry
        .register(Arc::new(FileOperationsSkill::new()))
        .await?;
    registry
        .register(Arc::new(CodeAnalysisSkill::new()))
        .await?;
    registry
        .register(Arc::new(DataTransformSkill::new()))
        .await?;

    tracing::info!("Registered built-in skills");
    Ok(())
}
