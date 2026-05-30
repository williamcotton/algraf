//! Shared editor intelligence used by the native LSP server and the browser
//! Monaco/WASM adapter.
//!
//! This crate deliberately owns only the pure request logic: parsing,
//! schema-backed analysis, hover, completion, formatting, navigation, symbols,
//! semantic tokens, code actions, and inlay hints. Native JSON-RPC serving stays
//! in `algraf-lsp`; browser pointer/length ABI stays in `algraf-wasm`.

pub mod analysis;
pub mod code_actions;
pub mod completion;
pub mod diagnostics;
pub mod document;
pub mod hover;
pub mod inlay;
pub mod navigation;
pub mod positions;
pub mod semantic_tokens;
pub mod signature;
pub mod symbols;

pub mod service;

pub use document::{AnalysisState, DocumentState, ParseState};
