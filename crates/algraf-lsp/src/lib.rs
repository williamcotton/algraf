//! tower-lsp backend and feature modules.
//!
//! See spec §21 (LSP architecture), §23.2 (module boundaries), and §24.2
//! (LSP pipeline).

use std::io;

use tokio::runtime::Runtime;
use tower_lsp::{LspService, Server};

mod analysis;
mod backend;
mod code_actions;
mod completion;
mod diagnostics;
mod document;
mod hover;
mod inlay;
mod navigation;
mod positions;
mod preview;
mod semantic_tokens;
mod signature;
mod symbols;

pub use backend::Backend;

/// Run the Algraf language server over standard input and output.
pub fn run_stdio() -> io::Result<()> {
    Runtime::new()?.block_on(serve_stdio());
    Ok(())
}

/// Serve the Algraf language server over standard input and output.
pub async fn serve_stdio() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = build_service();
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// Build the LSP service with the standard methods plus the custom
/// `algraf/preview` render request (spec §21.18).
pub fn build_service() -> (LspService<Backend>, tower_lsp::ClientSocket) {
    LspService::build(Backend::new)
        .custom_method("algraf/preview", Backend::preview)
        .finish()
}
