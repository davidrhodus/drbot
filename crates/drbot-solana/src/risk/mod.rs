//! Risk management and analysis.
//!
//! This module provides portfolio-level risk metrics, correlation analysis,
//! protocol dependency tracking, and risk alerts.

pub mod alerts;
pub mod correlation;
pub mod dependencies;
pub mod portfolio;

pub use alerts::*;
pub use correlation::*;
pub use dependencies::*;
pub use portfolio::*;
