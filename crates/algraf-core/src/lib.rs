//! Shared primitives for Algraf: source spans and diagnostics.
//!
//! See spec §23.2 (module boundaries), §11.2 (span type), and §12.15
//! (diagnostics).

pub mod diagnostic;
pub mod span;
pub mod util;

pub use diagnostic::{all_codes, codes, Diagnostic, DiagnosticCode, RelatedSpan, Severity};
pub use span::{ByteOffset, Span};
pub use util::{closest, edit_distance, is_url_like};
