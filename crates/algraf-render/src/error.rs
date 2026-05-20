//! Render errors (spec §23.4).

/// A fatal rendering error. User-facing problems are diagnostics, not errors;
/// this is reserved for conditions that prevent producing any SVG.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("chart has no valid data source")]
    MissingDataSource,
}
