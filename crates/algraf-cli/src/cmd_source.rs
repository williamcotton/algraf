//! Shared source/data command-line arguments for source-consuming commands.

use std::path::PathBuf;

use algraf_data::Format;
use clap::Args;

/// Stream/data format override for caller-provided primary data.
#[derive(Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub(crate) enum DataFormatArg {
    Csv,
    Tsv,
    Json,
    Ndjson,
    Geojson,
    Topojson,
    Parquet,
    #[value(name = "arrow-stream", alias = "arrow")]
    ArrowStream,
}

impl From<DataFormatArg> for Format {
    fn from(value: DataFormatArg) -> Self {
        match value {
            DataFormatArg::Csv => Format::Csv,
            DataFormatArg::Tsv => Format::Tsv,
            DataFormatArg::Json => Format::Json,
            DataFormatArg::Ndjson => Format::NdJson,
            DataFormatArg::Geojson => Format::GeoJson,
            DataFormatArg::Topojson => Format::TopoJson,
            DataFormatArg::Parquet => Format::Parquet,
            DataFormatArg::ArrowStream => Format::ArrowStream,
        }
    }
}

#[derive(Args)]
pub(crate) struct SourceArgs {
    /// Source file, or `-` for stdin.
    pub(crate) input: Option<String>,
    /// Inline source text. Mutually exclusive with a source file or `-`.
    #[arg(short = 'e', long = "eval", conflicts_with = "input")]
    pub(crate) eval: Option<String>,
    #[arg(long)]
    pub(crate) base_dir: Option<PathBuf>,
    /// Data path, or `-` for stdin (overrides the chart's data argument).
    #[arg(long)]
    pub(crate) data: Option<String>,
    /// Explicit format for caller-provided primary data or --data paths.
    #[arg(long, value_enum)]
    pub(crate) data_format: Option<DataFormatArg>,
    /// Raw source variable assignment, repeated as --var key=value.
    #[arg(long = "var")]
    pub(crate) vars: Vec<String>,
}
