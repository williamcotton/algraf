//! `algraf lsp` — run the language server over stdio (spec §22).
//!
//! The CLI owns the transport and process lifecycle; the language server
//! itself lives in `algraf-lsp`.

use crate::error::CliError;

pub(crate) fn lsp_cmd() -> Result<(), CliError> {
    algraf_lsp::run_stdio()
        .map_err(|e| CliError::Internal(format!("failed to start language server: {e}")))
}
