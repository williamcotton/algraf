use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Instant;

use algraf_core::{codes, DiagnosticCode};
use algraf_data::{read_parquet_path_projected, DataValueRef, Table};
use arrow_array::{
    ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray,
};
use arrow_schema::{DataType as ArrowDataType, Field, Schema, TimeUnit};
use chrono::Utc;
use clap::{Parser, Subcommand, ValueEnum};
use parquet::arrow::ArrowWriter;

const TLC_URL: &str =
    "https://d37ci6vzurychx.cloudfront.net/trip-data/yellow_tripdata_2024-01.parquet";
const TLC_ZONES_URL: &str = "https://d37ci6vzurychx.cloudfront.net/misc/taxi_zone_lookup.csv";
const SFO_URL: &str = "https://static.sfomuseum.org/parquet/sfomuseum-data-flights-2026-03.parquet";

const TLC_COLUMNS: [&str; 4] = [
    "trip_distance",
    "total_amount",
    "payment_type",
    "tpep_pickup_datetime",
];
const SFO_COLUMNS: [&str; 6] = [
    "flysfo:date",
    "flysfo:event",
    "flysfo:airline",
    "flysfo:journey",
    "geom:longitude",
    "geom:latitude",
];
const JAN_2024_START_MICROS: i64 = 1_704_067_200_000_000;
const FEB_2024_START_MICROS: i64 = 1_706_745_600_000_000;

const REPORT_HEADER: [&str; 25] = [
    "repo",
    "tool",
    "run_label",
    "suite",
    "workload",
    "dataset",
    "tier",
    "input_format",
    "output_format",
    "status",
    "command",
    "output_path",
    "log_path",
    "input_rows",
    "output_rows",
    "marks",
    "output_bytes",
    "elapsed_ms",
    "parse_ms",
    "prepare_ms",
    "render_ms",
    "timing_total_ms",
    "run_timestamp_utc",
    "git_ref",
    "notes",
];

fn main() {
    if let Err(err) = run() {
        eprintln!("algraf-bench: {err}");
        std::process::exit(1);
    }
}

#[derive(Parser)]
#[command(name = "algraf-bench")]
#[command(about = "Algraf benchmark data lifecycle and before/after run reporting")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate deterministic local benchmark datasets under bench/data/generated.
    Generate {
        #[arg(long, value_enum, default_value_t = Tier::Stress)]
        tier: Tier,
        /// Override the tier's default row count.
        #[arg(long)]
        rows: Option<usize>,
    },
    /// Download external raw benchmark sources under bench/data/raw.
    Download {
        #[arg(long, value_enum, default_value_t = ExternalDataset::All)]
        dataset: ExternalDataset,
        #[arg(long)]
        force: bool,
    },
    /// Prepare downloaded raw sources into chart-ready files under bench/data/prepared.
    Prepare {
        #[arg(long, value_enum, default_value_t = ExternalDataset::All)]
        dataset: ExternalDataset,
    },
    /// Run benchmark workloads and write bench/runs/<run-label>/report.csv.
    Run {
        #[arg(long, value_enum, default_value_t = Suite::Large)]
        suite: Suite,
        #[arg(long, value_enum, default_value_t = Tier::Stress)]
        tier: Tier,
        #[arg(long)]
        run_label: Option<String>,
        #[arg(long, value_enum, default_value_t = BuildProfile::Debug)]
        profile: BuildProfile,
        /// Do not generate missing synthetic benchmark data before running.
        #[arg(long)]
        no_generate: bool,
        /// Do not prepare downloaded external sources even if raw files exist.
        #[arg(long)]
        no_prepare: bool,
    },
    /// Compare elapsed times for two run reports.
    Compare {
        #[arg(long)]
        before: String,
        #[arg(long)]
        after: String,
    },
    /// Copy an ignored run report into a tracked baseline directory.
    Snapshot {
        #[arg(long)]
        run_label: String,
        #[arg(long)]
        baseline: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Suite {
    Large,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ExternalDataset {
    All,
    Tlc,
    Sfo,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Tier {
    Smoke,
    Local,
    Stress,
}

impl Tier {
    fn as_str(self) -> &'static str {
        match self {
            Tier::Smoke => "smoke",
            Tier::Local => "local",
            Tier::Stress => "stress",
        }
    }

    fn rows(self) -> usize {
        match self {
            Tier::Smoke => 1_000,
            Tier::Local => 100_000,
            Tier::Stress => 1_000_000,
        }
    }

    fn wide_columns(self) -> usize {
        match self {
            Tier::Smoke => 32,
            Tier::Local => 64,
            Tier::Stress => 96,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BuildProfile {
    Debug,
    Release,
}

impl BuildProfile {
    fn dir(self) -> &'static str {
        match self {
            BuildProfile::Debug => "debug",
            BuildProfile::Release => "release",
        }
    }
}

#[derive(Clone, Copy)]
enum Expected {
    Success,
    Diagnostic(DiagnosticCode),
}

struct Workload {
    name: &'static str,
    chart: &'static str,
    dataset: &'static str,
    input_format: &'static str,
    output_format: &'static str,
    input_rows: fn(Tier) -> Option<usize>,
    required_path: Option<&'static str>,
    expected: Expected,
    extra_args: &'static [&'static str],
}

struct RunMetadata {
    timestamp_utc: String,
    git_ref: String,
    label: String,
}

#[derive(Debug, Clone, Default)]
struct TimingPhases {
    parse_ms: String,
    prepare_ms: String,
    render_ms: String,
    total_ms: String,
}

#[derive(Debug)]
struct SfoRow {
    date_micros: i64,
    event: String,
    airline: String,
    journey: String,
    longitude: f64,
    latitude: f64,
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let root = repo_root();
    match cli.command {
        Commands::Generate { tier, rows } => generate_all(&root, tier, rows)?,
        Commands::Download { dataset, force } => download_external(&root, dataset, force)?,
        Commands::Prepare { dataset } => prepare_external(&root, dataset)?,
        Commands::Run {
            suite,
            tier,
            run_label,
            profile,
            no_generate,
            no_prepare,
        } => run_suite(
            &root,
            suite,
            tier,
            run_label,
            profile,
            no_generate,
            no_prepare,
        )?,
        Commands::Compare { before, after } => compare_runs(&root, &before, &after)?,
        Commands::Snapshot {
            run_label,
            baseline,
        } => {
            snapshot_run(&root, &run_label, &baseline)?;
        }
    }
    Ok(())
}

#[derive(Debug)]
struct ReportRow {
    status: String,
    elapsed_ms: Option<f64>,
    marks: String,
    output_bytes: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("algraf-bench crate should live under crates/")
        .to_path_buf()
}

fn generate_all(
    root: &Path,
    tier: Tier,
    rows_override: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let rows = rows_override.unwrap_or_else(|| tier.rows());
    generate_million_row_csv(root, rows)?;
    generate_synthetic_fixtures(root, tier, rows)?;
    Ok(())
}

fn download_external(
    root: &Path,
    dataset: ExternalDataset,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if matches!(dataset, ExternalDataset::All | ExternalDataset::Tlc) {
        download_one(
            root,
            "NYC TLC January 2024 trips",
            &std::env::var("ALGRAF_TLC_URL").unwrap_or_else(|_| TLC_URL.to_string()),
            &root.join("bench/data/raw/tlc/yellow_tripdata_2024-01.parquet"),
            force,
        )?;
        download_one(
            root,
            "NYC TLC taxi zones",
            &std::env::var("ALGRAF_TLC_ZONES_URL").unwrap_or_else(|_| TLC_ZONES_URL.to_string()),
            &root.join("bench/data/raw/tlc/taxi_zone_lookup.csv"),
            force,
        )?;
    }
    if matches!(dataset, ExternalDataset::All | ExternalDataset::Sfo) {
        download_one(
            root,
            "SFO Museum March 2026 flights",
            &std::env::var("ALGRAF_SFO_URL").unwrap_or_else(|_| SFO_URL.to_string()),
            &root.join("bench/data/raw/sfo/sfomuseum-data-flights-2026-03.parquet"),
            force,
        )?;
    }
    Ok(())
}

fn download_one(
    root: &Path,
    label: &str,
    url: &str,
    out: &Path,
    force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if out.exists() && !force {
        println!("kept existing {} at {}", label, relative(root, out));
        return Ok(());
    }
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = out.with_extension("download");
    println!("downloading {} -> {}", label, relative(root, out));
    let status = Command::new("curl")
        .args(["-L", "--fail", "--show-error", "--output"])
        .arg(&tmp)
        .arg(url)
        .status()?;
    if !status.success() {
        let _ = fs::remove_file(&tmp);
        return Err(format!("download failed for {label}").into());
    }
    fs::rename(tmp, out)?;
    Ok(())
}

fn prepare_external(
    root: &Path,
    dataset: ExternalDataset,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut prepared = false;
    if matches!(dataset, ExternalDataset::All | ExternalDataset::Tlc) {
        let input = root.join("bench/data/raw/tlc/yellow_tripdata_2024-01.parquet");
        let output = root.join("bench/data/prepared/tlc/yellow_tripdata_2024-01_chart.parquet");
        if input.exists() {
            prepare_tlc(&input, &output)?;
            println!("prepared {}", relative(root, &output));
            prepared = true;
        } else if matches!(dataset, ExternalDataset::Tlc) {
            return Err(format!("missing TLC source: {}", relative(root, &input)).into());
        }
    }
    if matches!(dataset, ExternalDataset::All | ExternalDataset::Sfo) {
        let input = root.join("bench/data/raw/sfo/sfomuseum-data-flights-2026-03.parquet");
        let output =
            root.join("bench/data/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet");
        if input.exists() {
            prepare_sfo(&input, &output)?;
            println!("prepared {}", relative(root, &output));
            prepared = true;
        } else if matches!(dataset, ExternalDataset::Sfo) {
            return Err(format!("missing SFO source: {}", relative(root, &input)).into());
        }
    }
    if !prepared && matches!(dataset, ExternalDataset::All) {
        println!("no downloaded external sources found under bench/data/raw");
    }
    Ok(())
}

fn run_suite(
    root: &Path,
    suite: Suite,
    tier: Tier,
    run_label: Option<String>,
    profile: BuildProfile,
    no_generate: bool,
    no_prepare: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !matches!(suite, Suite::Large) {
        unreachable!("clap only exposes known suite values");
    }

    let million = root.join("bench/data/generated/million-row.csv");
    let synthetic = root.join("bench/data/generated/algraf-large-fixtures/current/manifest.json");
    if (!million.exists() || !synthetic.exists()) && !no_generate {
        generate_all(root, tier, None)?;
    }
    if !no_prepare {
        prepare_external(root, ExternalDataset::All)?;
    }
    build_cli(root, profile)?;

    let run_timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let run_label =
        run_label.unwrap_or_else(|| format!("run-{}", Utc::now().format("%Y%m%dT%H%M%SZ")));
    let run_label = sanitize_label(&run_label);
    let run_dir = root.join("bench/runs").join(&run_label);
    fs::create_dir_all(&run_dir)?;
    let report_path = run_dir.join("report.csv");
    let mut report = csv::Writer::from_path(&report_path)?;
    report.write_record(REPORT_HEADER)?;
    let run = RunMetadata {
        timestamp_utc: run_timestamp,
        git_ref: git_ref(root),
        label: run_label,
    };

    let mut failures = 0usize;
    for workload in large_workloads() {
        if let Some(required) = workload.required_path {
            let required = root.join(required);
            if !required.exists() {
                report.write_record(skip_record(
                    root,
                    workload,
                    tier,
                    &run,
                    &format!("missing {}", relative(root, &required)),
                ))?;
                continue;
            }
        }
        let row = run_workload(root, &run_dir, workload, tier, profile, &run);
        match row {
            Ok(record) => report.write_record(record)?,
            Err(err) => {
                failures += 1;
                report.write_record(failure_record(
                    root,
                    &run_dir,
                    workload,
                    tier,
                    &run,
                    &err.to_string(),
                ))?;
            }
        }
    }
    report.flush()?;
    println!("wrote {}", relative(root, &report_path));

    if failures > 0 {
        return Err(format!("{failures} workload(s) failed").into());
    }
    Ok(())
}

fn snapshot_run(
    root: &Path,
    run_label: &str,
    baseline: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let run_label = sanitize_label(run_label);
    let baseline = sanitize_label(baseline);
    let source_report = root.join("bench/runs").join(&run_label).join("report.csv");
    if !source_report.exists() {
        return Err(format!("missing run report: {}", relative(root, &source_report)).into());
    }
    let git_status = command_output(root, "git", &["status", "--short"]);

    let baseline_dir = root.join("bench/baselines").join(&baseline);
    fs::create_dir_all(&baseline_dir)?;
    let baseline_report = baseline_dir.join("report.csv");
    fs::copy(&source_report, &baseline_report)?;

    let environment = baseline_dir.join("environment.txt");
    write_environment(
        root,
        "algraf",
        &run_label,
        &baseline,
        &source_report,
        &environment,
        &git_status,
    )?;

    println!("wrote {}", relative(root, &baseline_report));
    println!("wrote {}", relative(root, &environment));
    Ok(())
}

fn compare_runs(root: &Path, before: &str, after: &str) -> Result<(), Box<dyn std::error::Error>> {
    let before = sanitize_label(before);
    let after = sanitize_label(after);
    let before_report = read_run_report(root, &before)?;
    let after_report = read_run_report(root, &after)?;
    let mut workloads: Vec<_> = before_report.keys().chain(after_report.keys()).collect();
    workloads.sort();
    workloads.dedup();

    let mut writer = csv::Writer::from_writer(std::io::stdout());
    writer.write_record([
        "workload",
        "before_ms",
        "after_ms",
        "delta_ms",
        "improvement_pct",
        "before_status",
        "after_status",
        "before_marks",
        "after_marks",
        "before_output_bytes",
        "after_output_bytes",
    ])?;
    for workload in workloads {
        let before_row = before_report.get(workload);
        let after_row = after_report.get(workload);
        let before_ms = before_row.and_then(|row| row.elapsed_ms);
        let after_ms = after_row.and_then(|row| row.elapsed_ms);
        let delta = before_ms
            .zip(after_ms)
            .map(|(before, after)| after - before);
        let improvement = before_ms.zip(after_ms).and_then(|(before, after)| {
            (before.abs() > f64::EPSILON).then_some((before - after) * 100.0 / before)
        });
        writer.write_record([
            workload.as_str(),
            &format_optional_ms(before_ms),
            &format_optional_ms(after_ms),
            &format_optional_ms(delta),
            &format_optional_percent(improvement),
            before_row.map_or("", |row| row.status.as_str()),
            after_row.map_or("", |row| row.status.as_str()),
            before_row.map_or("", |row| row.marks.as_str()),
            after_row.map_or("", |row| row.marks.as_str()),
            before_row.map_or("", |row| row.output_bytes.as_str()),
            after_row.map_or("", |row| row.output_bytes.as_str()),
        ])?;
    }
    writer.flush()?;
    Ok(())
}

fn read_run_report(
    root: &Path,
    label: &str,
) -> Result<HashMap<String, ReportRow>, Box<dyn std::error::Error>> {
    let path = root.join("bench/runs").join(label).join("report.csv");
    if !path.exists() {
        return Err(format!("missing run report: {}", relative(root, &path)).into());
    }
    let mut reader = csv::Reader::from_path(&path)?;
    let headers = reader.headers()?.clone();
    let workload_index = required_report_column(&headers, "workload")?;
    let status_index = required_report_column(&headers, "status")?;
    let elapsed_index = required_report_column(&headers, "elapsed_ms")?;
    let marks_index = required_report_column(&headers, "marks")?;
    let bytes_index = required_report_column(&headers, "output_bytes")?;
    let mut rows = HashMap::new();
    for record in reader.records() {
        let record = record?;
        let Some(workload) = record.get(workload_index).filter(|value| !value.is_empty()) else {
            continue;
        };
        rows.insert(
            workload.to_string(),
            ReportRow {
                status: record.get(status_index).unwrap_or_default().to_string(),
                elapsed_ms: record
                    .get(elapsed_index)
                    .filter(|value| !value.is_empty())
                    .and_then(|value| value.parse::<f64>().ok()),
                marks: record.get(marks_index).unwrap_or_default().to_string(),
                output_bytes: record.get(bytes_index).unwrap_or_default().to_string(),
            },
        );
    }
    Ok(rows)
}

fn required_report_column(
    headers: &csv::StringRecord,
    name: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    headers
        .iter()
        .position(|header| header == name)
        .ok_or_else(|| format!("missing `{name}` column").into())
}

fn format_optional_ms(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| format!("{value:.0}"))
}

fn format_optional_percent(value: Option<f64>) -> String {
    value.map_or_else(String::new, |value| format!("{value:.1}"))
}

fn write_environment(
    root: &Path,
    repo: &str,
    run_label: &str,
    baseline: &str,
    source_report: &Path,
    path: &Path,
    git_status: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufWriter::new(File::create(path)?);
    writeln!(file, "repo: {repo}")?;
    writeln!(file, "baseline: {baseline}")?;
    writeln!(file, "run_label: {run_label}")?;
    writeln!(
        file,
        "snapshot_timestamp_utc: {}",
        Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    )?;
    writeln!(file, "source_report: {}", relative(root, source_report))?;
    writeln!(file, "git_ref: {}", git_ref(root))?;
    writeln!(file, "git_status_short:")?;
    if git_status.trim().is_empty() {
        writeln!(file, "  clean")?;
    } else {
        for line in git_status.lines() {
            writeln!(file, "  {line}")?;
        }
    }
    writeln!(
        file,
        "system: {}",
        command_output(root, "uname", &["-a"]).trim()
    )?;
    writeln!(
        file,
        "rustc: {}",
        command_output(root, "rustc", &["-V"]).trim()
    )?;
    writeln!(
        file,
        "cargo: {}",
        command_output(root, "cargo", &["-V"]).trim()
    )?;
    writeln!(
        file,
        "run_command: cargo run -p algraf-bench -- run --suite large --run-label {run_label}"
    )?;
    Ok(())
}

fn large_workloads() -> &'static [Workload] {
    &[
        Workload {
            name: "million_row_summary_bin",
            chart: "bench/workloads/large/million_row_summary_bin.ag",
            dataset: "million-row",
            input_format: "csv",
            output_format: "svg",
            input_rows: million_rows,
            required_path: Some("bench/data/generated/million-row.csv"),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "synthetic_bin2d_density",
            chart: "bench/workloads/large/synthetic_bin2d_density.ag",
            dataset: "synthetic-dense-points",
            input_format: "parquet",
            output_format: "svg",
            input_rows: tier_rows,
            required_path: Some(
                "bench/data/generated/algraf-large-fixtures/current/dense_points.parquet",
            ),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "synthetic_nullable_histogram",
            chart: "bench/workloads/large/synthetic_nullable_histogram.ag",
            dataset: "synthetic-sparse-nullable",
            input_format: "parquet",
            output_format: "svg",
            input_rows: tier_rows,
            required_path: Some(
                "bench/data/generated/algraf-large-fixtures/current/sparse_nullable.parquet",
            ),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "synthetic_projection_smoke",
            chart: "bench/workloads/large/synthetic_projection_smoke.ag",
            dataset: "synthetic-wide-metrics",
            input_format: "parquet",
            output_format: "svg",
            input_rows: wide_rows,
            required_path: Some(
                "bench/data/generated/algraf-large-fixtures/current/wide_metrics.parquet",
            ),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "synthetic_raw_mark_budget",
            chart: "bench/workloads/large/synthetic_raw_mark_budget.ag",
            dataset: "synthetic-dense-points",
            input_format: "parquet",
            output_format: "svg",
            input_rows: tier_rows,
            required_path: Some(
                "bench/data/generated/algraf-large-fixtures/current/dense_points.parquet",
            ),
            expected: Expected::Diagnostic(codes::E2001),
            extra_args: &["--mark-budget", "500"],
        },
        Workload {
            name: "tlc_trip_distance_histogram",
            chart: "bench/workloads/large/tlc_trip_distance_histogram.ag",
            dataset: "tlc-yellow-tripdata-2024-01",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some("bench/data/prepared/tlc/yellow_tripdata_2024-01_chart.parquet"),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "tlc_fare_distance_density",
            chart: "bench/workloads/large/tlc_fare_distance_density.ag",
            dataset: "tlc-yellow-tripdata-2024-01",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some("bench/data/prepared/tlc/yellow_tripdata_2024-01_chart.parquet"),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "tlc_payment_type_counts",
            chart: "bench/workloads/large/tlc_payment_type_counts.ag",
            dataset: "tlc-yellow-tripdata-2024-01",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some("bench/data/prepared/tlc/yellow_tripdata_2024-01_chart.parquet"),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "tlc_pickup_time_bins",
            chart: "bench/workloads/large/tlc_pickup_time_bins.ag",
            dataset: "tlc-yellow-tripdata-2024-01",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some("bench/data/prepared/tlc/yellow_tripdata_2024-01_chart.parquet"),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "sfo_daily_flights",
            chart: "bench/workloads/large/sfo_daily_flights.ag",
            dataset: "sfo-museum-flights-2026-03",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some(
                "bench/data/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet",
            ),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "sfo_event_counts",
            chart: "bench/workloads/large/sfo_event_counts.ag",
            dataset: "sfo-museum-flights-2026-03",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some(
                "bench/data/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet",
            ),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "sfo_airline_counts",
            chart: "bench/workloads/large/sfo_airline_counts.ag",
            dataset: "sfo-museum-flights-2026-03",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some(
                "bench/data/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet",
            ),
            expected: Expected::Success,
            extra_args: &[],
        },
        Workload {
            name: "sfo_route_density",
            chart: "bench/workloads/large/sfo_route_density.ag",
            dataset: "sfo-museum-flights-2026-03",
            input_format: "parquet",
            output_format: "svg",
            input_rows: |_| None,
            required_path: Some(
                "bench/data/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet",
            ),
            expected: Expected::Success,
            extra_args: &[],
        },
    ]
}

fn run_workload(
    root: &Path,
    run_dir: &Path,
    workload: &Workload,
    tier: Tier,
    profile: BuildProfile,
    run: &RunMetadata,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output_path = run_dir.join(format!("{}.svg", workload.name));
    let log_path = run_dir.join(format!("{}.log", workload.name));
    let bin = root.join("target").join(profile.dir()).join("algraf");
    let mut command_text = format!(
        "{} render {} --output {}",
        relative(root, &bin),
        workload.chart,
        relative(root, &output_path)
    );
    for arg in workload.extra_args {
        command_text.push(' ');
        command_text.push_str(arg);
    }
    let stdout_log = File::create(&log_path)?;
    let stderr_log = stdout_log.try_clone()?;
    let start = Instant::now();
    let mut command = Command::new(&bin);
    command
        .current_dir(root)
        .arg("render")
        .arg(workload.chart)
        .arg("--output")
        .arg(&output_path)
        .stdout(Stdio::from(stdout_log))
        .stderr(Stdio::from(stderr_log));
    for arg in workload.extra_args {
        command.arg(arg);
    }
    let status = command.status()?;
    let elapsed_ms = start.elapsed().as_millis();

    let log = fs::read_to_string(&log_path).unwrap_or_default();
    let status_text = match workload.expected {
        Expected::Success if status.success() => "ok",
        Expected::Success => "failed",
        Expected::Diagnostic(code) if !status.success() && log.contains(code.as_str()) => {
            "expected-diagnostic"
        }
        Expected::Diagnostic(_) if status.success() => "unexpected-success",
        Expected::Diagnostic(_) => "failed",
    };
    let failed = matches!(status_text, "failed" | "unexpected-success");
    let output_bytes = byte_count(&output_path);
    let marks = if output_path.exists() {
        mark_count(&output_path).to_string()
    } else {
        String::new()
    };
    if failed {
        return Err(format!("{status_text}: {}", workload.name).into());
    }
    let (timing, notes) = if matches!(workload.expected, Expected::Success) {
        match run_timing(root, workload, profile) {
            Ok(timing) => (timing, String::new()),
            Err(err) => (TimingPhases::default(), format!("timing failed: {err}")),
        }
    } else {
        (TimingPhases::default(), String::new())
    };

    Ok(vec![
        "algraf".to_string(),
        "algraf-bench".to_string(),
        run.label.clone(),
        "large".to_string(),
        workload.name.to_string(),
        workload.dataset.to_string(),
        tier.as_str().to_string(),
        workload.input_format.to_string(),
        workload.output_format.to_string(),
        status_text.to_string(),
        command_text,
        relative(root, &output_path),
        relative(root, &log_path),
        input_rows_for(root, workload, tier),
        String::new(),
        marks,
        output_bytes.to_string(),
        elapsed_ms.to_string(),
        timing.parse_ms,
        timing.prepare_ms,
        timing.render_ms,
        timing.total_ms,
        run.timestamp_utc.clone(),
        run.git_ref.clone(),
        notes,
    ])
}

fn skip_record(
    root: &Path,
    workload: &Workload,
    tier: Tier,
    run: &RunMetadata,
    notes: &str,
) -> Vec<String> {
    vec![
        "algraf".to_string(),
        "algraf-bench".to_string(),
        run.label.clone(),
        "large".to_string(),
        workload.name.to_string(),
        workload.dataset.to_string(),
        tier.as_str().to_string(),
        workload.input_format.to_string(),
        workload.output_format.to_string(),
        "skipped".to_string(),
        String::new(),
        String::new(),
        String::new(),
        input_rows_for(root, workload, tier),
        String::new(),
        String::new(),
        "0".to_string(),
        "0".to_string(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        run.timestamp_utc.clone(),
        run.git_ref.clone(),
        notes.to_string(),
    ]
}

fn failure_record(
    root: &Path,
    run_dir: &Path,
    workload: &Workload,
    tier: Tier,
    run: &RunMetadata,
    notes: &str,
) -> Vec<String> {
    let log_path = run_dir.join(format!("{}.log", workload.name));
    vec![
        "algraf".to_string(),
        "algraf-bench".to_string(),
        run.label.clone(),
        "large".to_string(),
        workload.name.to_string(),
        workload.dataset.to_string(),
        tier.as_str().to_string(),
        workload.input_format.to_string(),
        workload.output_format.to_string(),
        "failed".to_string(),
        String::new(),
        String::new(),
        relative(root, &log_path),
        input_rows_for(root, workload, tier),
        String::new(),
        String::new(),
        "0".to_string(),
        "0".to_string(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        run.timestamp_utc.clone(),
        run.git_ref.clone(),
        notes.to_string(),
    ]
}

fn run_timing(
    root: &Path,
    workload: &Workload,
    profile: BuildProfile,
) -> Result<TimingPhases, Box<dyn std::error::Error>> {
    let bin = root
        .join("target")
        .join(profile.dir())
        .join("render-timing");
    let output = Command::new(&bin)
        .current_dir(root)
        .arg(workload.chart)
        .arg("--warmup")
        .arg("0")
        .arg("--iterations")
        .arg("1")
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(stderr.trim().to_string().into());
    }
    parse_timing_stdout(&String::from_utf8_lossy(&output.stdout))
}

fn parse_timing_stdout(stdout: &str) -> Result<TimingPhases, Box<dyn std::error::Error>> {
    let mut timing = TimingPhases::default();
    for line in stdout.lines() {
        let mut fields = line.split('\t');
        let Some(phase) = fields.next() else {
            continue;
        };
        let Some(_min) = fields.next() else {
            continue;
        };
        let Some(median) = fields.next() else {
            continue;
        };
        match phase {
            "parse" => timing.parse_ms = median.to_string(),
            "prepare" => timing.prepare_ms = median.to_string(),
            "render" => timing.render_ms = median.to_string(),
            "total" => timing.total_ms = median.to_string(),
            _ => {}
        }
    }
    if timing.total_ms.is_empty() {
        return Err("missing total phase in render-timing output".into());
    }
    Ok(timing)
}

fn input_rows_for(root: &Path, workload: &Workload, tier: Tier) -> String {
    if let Some(rows) = (workload.input_rows)(tier) {
        return rows.to_string();
    }
    workload
        .required_path
        .map(|path| root.join(path))
        .and_then(|path| match workload.input_format {
            "csv" => csv_data_rows(&path),
            "parquet" => parquet_rows(&path),
            _ => None,
        })
        .map(|rows| rows.to_string())
        .unwrap_or_default()
}

fn million_rows(_tier: Tier) -> Option<usize> {
    None
}

fn tier_rows(tier: Tier) -> Option<usize> {
    Some(tier.rows())
}

fn wide_rows(tier: Tier) -> Option<usize> {
    Some(tier.rows().min(80_000))
}

fn generate_million_row_csv(root: &Path, rows: usize) -> Result<(), Box<dyn std::error::Error>> {
    if rows == 0 {
        return Err("--rows must be greater than zero".into());
    }
    let out = root.join("bench/data/generated/million-row.csv");
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = out.with_extension("csv.tmp");
    let mut writer = BufWriter::new(File::create(&tmp)?);
    writeln!(writer, "row,segment,x,score,latency_ms")?;
    for row in 0..rows {
        let segment_index = row % 4;
        let segment = ["A", "B", "C", "D"][segment_index];
        let x = (row % 10_000) as f64 / 100.0;
        let cycle = (row * 37) % 1_000;
        let drift = row / 100_000;
        let score =
            20.0 + (segment_index as f64 * 5.0) + (x * 0.3) + (cycle as f64 / 25.0) + drift as f64;
        let latency = 40.0 + (segment_index as f64 * 12.0) + (((row * 17) % 900) as f64 / 3.0);
        writeln!(writer, "{row},{segment},{x:.2},{score:.3},{latency:.3}")?;
    }
    writer.flush()?;
    fs::rename(&tmp, &out)?;
    println!("generated {} rows at {}", rows, relative(root, &out));
    Ok(())
}

fn generate_synthetic_fixtures(
    root: &Path,
    tier: Tier,
    rows: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let tier_dir = root.join("bench/data/generated/algraf-large-fixtures/current");
    fs::create_dir_all(&tier_dir)?;
    let paths = SyntheticPaths {
        dense_points: tier_dir.join("dense_points.parquet"),
        tall_events: tier_dir.join("tall_events.parquet"),
        sparse_nullable: tier_dir.join("sparse_nullable.parquet"),
        high_cardinality: tier_dir.join("high_cardinality.parquet"),
        wide_metrics: tier_dir.join("wide_metrics.parquet"),
        tall_csv: tier_dir.join("tall_events.csv"),
        tall_ndjson: tier_dir.join("tall_events.ndjson"),
        manifest: tier_dir.join("manifest.json"),
    };
    write_dense_points(&paths.dense_points, rows)?;
    write_tall_events(&paths, rows)?;
    write_sparse_nullable(&paths.sparse_nullable, rows)?;
    write_high_cardinality(&paths.high_cardinality, rows)?;
    write_wide_metrics(&paths.wide_metrics, rows.min(80_000), tier.wide_columns())?;
    write_manifest(root, &paths, tier, rows)?;
    println!(
        "generated synthetic fixtures at {}",
        relative(root, &paths.manifest)
    );
    Ok(())
}

#[derive(Debug, Clone)]
struct SyntheticPaths {
    dense_points: PathBuf,
    tall_events: PathBuf,
    sparse_nullable: PathBuf,
    high_cardinality: PathBuf,
    wide_metrics: PathBuf,
    tall_csv: PathBuf,
    tall_ndjson: PathBuf,
    manifest: PathBuf,
}

fn write_dense_points(path: &Path, rows: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = Lcg::new(0x8c5d_7a01_d011_01f5);
    let mut x = Vec::with_capacity(rows);
    let mut y = Vec::with_capacity(rows);
    let mut value = Vec::with_capacity(rows);
    let mut group = Vec::with_capacity(rows);
    for row in 0..rows {
        let cluster = (row % 4) as f64;
        let cx = [-34.0, -8.0, 14.0, 38.0][row % 4];
        let cy = [18.0, -22.0, 27.0, -10.0][(row / 7) % 4];
        let px = cx + rng.normalish() * 9.0;
        let py = cy + rng.normalish() * 7.0 + px * 0.18;
        x.push(Some(px));
        y.push(Some(py));
        value.push(Some((px * 0.12 + py * 0.08 + cluster).sin() * 25.0 + 60.0));
        group.push(Some(format!("g{}", row % 6)));
    }
    write_parquet(
        path,
        Arc::new(Schema::new(vec![
            Field::new("x", ArrowDataType::Float64, true),
            Field::new("y", ArrowDataType::Float64, true),
            Field::new("value", ArrowDataType::Float64, true),
            Field::new("group", ArrowDataType::Utf8, true),
        ])),
        vec![
            Arc::new(Float64Array::from(x)) as ArrayRef,
            Arc::new(Float64Array::from(y)) as ArrayRef,
            Arc::new(Float64Array::from(value)) as ArrayRef,
            Arc::new(StringArray::from_iter(group)) as ArrayRef,
        ],
    )
}

fn write_tall_events(
    paths: &SyntheticPaths,
    rows: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = Lcg::new(0xd23a_5c8f_a07e_2026);
    let base = 1_704_067_200_000_000_i64;
    let mut event_time = Vec::with_capacity(rows);
    let mut value = Vec::with_capacity(rows);
    let mut group = Vec::with_capacity(rows);
    let mut nullable_score = Vec::with_capacity(rows);
    let mut csv = BufWriter::new(File::create(&paths.tall_csv)?);
    let mut ndjson = BufWriter::new(File::create(&paths.tall_ndjson)?);
    writeln!(csv, "event_time,value,group,nullable_score")?;
    for row in 0..rows {
        let ts = base + (row as i64) * 60_000_000;
        let v = 40.0 + ((row as f64) / 97.0).sin() * 11.0 + rng.normalish() * 3.5;
        let g = format!("segment_{}", row % 8);
        let score = (row % 11 != 0).then_some(55.0 + rng.normalish() * 12.0);
        event_time.push(Some(ts));
        value.push(Some(v));
        group.push(Some(g.clone()));
        nullable_score.push(score);
        writeln!(
            csv,
            "{ts},{v:.6},{g},{}",
            score.map_or(String::new(), |s| format!("{s:.6}"))
        )?;
        writeln!(
            ndjson,
            "{{\"event_time\":{ts},\"value\":{v:.6},\"group\":\"{g}\",\"nullable_score\":{}}}",
            score.map_or("null".to_string(), |s| format!("{s:.6}"))
        )?;
    }
    write_parquet(
        &paths.tall_events,
        Arc::new(Schema::new(vec![
            Field::new(
                "event_time",
                ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
                true,
            ),
            Field::new("value", ArrowDataType::Float64, true),
            Field::new("group", ArrowDataType::Utf8, true),
            Field::new("nullable_score", ArrowDataType::Float64, true),
        ])),
        vec![
            Arc::new(TimestampMicrosecondArray::from(event_time)) as ArrayRef,
            Arc::new(Float64Array::from(value)) as ArrayRef,
            Arc::new(StringArray::from_iter(group)) as ArrayRef,
            Arc::new(Float64Array::from(nullable_score)) as ArrayRef,
        ],
    )
}

fn write_sparse_nullable(path: &Path, rows: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut rng = Lcg::new(0x51a7_0019_ba55_2026);
    let base = 1_704_067_200_000_000_i64;
    let mut nullable_value = Vec::with_capacity(rows);
    let mut event_time = Vec::with_capacity(rows);
    let mut category = Vec::with_capacity(rows);
    for row in 0..rows {
        let center = if row % 3 == 0 { -14.0 } else { 18.0 };
        let wave = ((row as f64) / 180.0).sin() * 8.0;
        nullable_value.push((row % 5 != 0).then_some(center + wave + rng.normalish() * 6.5));
        event_time.push((row % 13 != 0).then_some(base + (row as i64) * 86_400_000_000));
        category.push((row % 9 != 0).then(|| format!("bucket_{}", row % 7)));
    }
    write_parquet(
        path,
        Arc::new(Schema::new(vec![
            Field::new("nullable_value", ArrowDataType::Float64, true),
            Field::new(
                "event_time",
                ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
                true,
            ),
            Field::new("category", ArrowDataType::Utf8, true),
        ])),
        vec![
            Arc::new(Float64Array::from(nullable_value)) as ArrayRef,
            Arc::new(TimestampMicrosecondArray::from(event_time)) as ArrayRef,
            Arc::new(StringArray::from_iter(category)) as ArrayRef,
        ],
    )
}

fn write_high_cardinality(path: &Path, rows: usize) -> Result<(), Box<dyn std::error::Error>> {
    let categories = rows.clamp(200, 2_000);
    let mut category = Vec::with_capacity(rows);
    let mut value = Vec::with_capacity(rows);
    for row in 0..rows {
        category.push(Some(format!("cat_{:04}", row % categories)));
        value.push(Some(((row * 17 % 10_000) as f64) / 100.0));
    }
    write_parquet(
        path,
        Arc::new(Schema::new(vec![
            Field::new("category", ArrowDataType::Utf8, true),
            Field::new("value", ArrowDataType::Float64, true),
        ])),
        vec![
            Arc::new(StringArray::from_iter(category)) as ArrayRef,
            Arc::new(Float64Array::from(value)) as ArrayRef,
        ],
    )
}

fn write_wide_metrics(
    path: &Path,
    rows: usize,
    wide_columns: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut fields = vec![
        Field::new("id", ArrowDataType::Int64, false),
        Field::new("metric_a", ArrowDataType::Float64, true),
        Field::new("metric_b", ArrowDataType::Float64, true),
        Field::new(
            "event_time",
            ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
    ];
    for idx in 0..wide_columns {
        fields.push(Field::new(
            format!("unused_{idx:02}"),
            ArrowDataType::Float64,
            true,
        ));
    }
    let mut columns: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from_iter_values((0..rows).map(|v| v as i64))),
        Arc::new(Float64Array::from_iter_values(
            (0..rows).map(|row| ((row as f64) / 31.0).sin() * 100.0),
        )),
        Arc::new(Float64Array::from_iter_values(
            (0..rows).map(|row| 20.0 + ((row as f64) / 19.0).cos() * 18.0),
        )),
        Arc::new(TimestampMicrosecondArray::from_iter_values((0..rows).map(
            |row| 1_704_067_200_000_000_i64 + (row as i64) * 3_600_000_000,
        ))),
    ];
    for idx in 0..wide_columns {
        columns.push(Arc::new(Float64Array::from_iter_values(
            (0..rows).map(|row| ((row + idx * 13) % 10_000) as f64 / 10.0),
        )));
    }
    write_parquet(path, Arc::new(Schema::new(fields)), columns)
}

fn write_manifest(
    root: &Path,
    paths: &SyntheticPaths,
    tier: Tier,
    rows: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufWriter::new(File::create(&paths.manifest)?);
    writeln!(file, "{{")?;
    writeln!(file, "  \"tier\": \"{}\",", tier.as_str())?;
    writeln!(file, "  \"rows\": {},", rows)?;
    writeln!(file, "  \"wide_columns\": {},", tier.wide_columns())?;
    writeln!(file, "  \"files\": {{")?;
    writeln!(
        file,
        "    \"dense_points\": \"{}\",",
        relative(root, &paths.dense_points)
    )?;
    writeln!(
        file,
        "    \"tall_events\": \"{}\",",
        relative(root, &paths.tall_events)
    )?;
    writeln!(
        file,
        "    \"sparse_nullable\": \"{}\",",
        relative(root, &paths.sparse_nullable)
    )?;
    writeln!(
        file,
        "    \"high_cardinality\": \"{}\",",
        relative(root, &paths.high_cardinality)
    )?;
    writeln!(
        file,
        "    \"wide_metrics\": \"{}\",",
        relative(root, &paths.wide_metrics)
    )?;
    writeln!(
        file,
        "    \"tall_csv\": \"{}\",",
        relative(root, &paths.tall_csv)
    )?;
    writeln!(
        file,
        "    \"tall_ndjson\": \"{}\"",
        relative(root, &paths.tall_ndjson)
    )?;
    writeln!(file, "  }}")?;
    writeln!(file, "}}")?;
    Ok(())
}

fn write_parquet(
    path: &Path,
    schema: Arc<Schema>,
    columns: Vec<ArrayRef>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let batch = RecordBatch::try_new(schema.clone(), columns)?;
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}

fn prepare_tlc(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let loaded = read_parquet_path_projected(input, Some(&TLC_COLUMNS))?;
    let table = loaded.frame;
    let trip_distance = required_column(&table, "trip_distance")?;
    let total_amount = required_column(&table, "total_amount")?;
    let payment_type = required_column(&table, "payment_type")?;
    let pickup_time = required_column(&table, "tpep_pickup_datetime")?;

    let mut distances = Vec::new();
    let mut totals = Vec::new();
    let mut payments = Vec::new();
    let mut pickups = Vec::new();
    for row in 0..table.row_count() {
        let Some(distance) = trip_distance.f64_at(row) else {
            continue;
        };
        let Some(total) = total_amount.f64_at(row) else {
            continue;
        };
        let Some(payment) = int_at(payment_type.get(row)) else {
            continue;
        };
        let Some(pickup) = pickup_time.temporal_at(row) else {
            continue;
        };
        let pickup_micros = pickup.instant.and_utc().timestamp_micros();
        if !(0.0..=40.0).contains(&distance) || distance == 0.0 {
            continue;
        }
        if !(0.0..=200.0).contains(&total) {
            continue;
        }
        if !(1..=6).contains(&payment) {
            continue;
        }
        if !(JAN_2024_START_MICROS..FEB_2024_START_MICROS).contains(&pickup_micros) {
            continue;
        }
        distances.push(Some(distance));
        totals.push(Some(total));
        payments.push(Some(payment_label(payment).to_string()));
        pickups.push(Some(pickup_micros));
    }
    if distances.is_empty() {
        return Err(format!(
            "no TLC rows survived chart preparation from {}",
            input.display()
        )
        .into());
    }
    write_tlc_parquet(output, distances, totals, payments, pickups)
}

fn prepare_sfo(input: &Path, output: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let loaded = read_parquet_path_projected(input, Some(&SFO_COLUMNS))?;
    let table = loaded.frame;
    let date = required_column(&table, "flysfo:date")?;
    let event = required_column(&table, "flysfo:event")?;
    let airline = required_column(&table, "flysfo:airline")?;
    let journey = required_column(&table, "flysfo:journey")?;
    let longitude = required_column(&table, "geom:longitude")?;
    let latitude = required_column(&table, "geom:latitude")?;

    let mut rows = Vec::new();
    for row in 0..table.row_count() {
        let Some(date) = date.temporal_at(row) else {
            continue;
        };
        let Some(event) = event.category_at(row).and_then(|value| event_label(&value)) else {
            continue;
        };
        let Some(airline) = airline.category_at(row) else {
            continue;
        };
        let Some(journey) = journey.category_at(row) else {
            continue;
        };
        let Some(lon) = longitude.f64_at(row) else {
            continue;
        };
        let Some(lat) = latitude.f64_at(row) else {
            continue;
        };
        if !(-180.0..=180.0).contains(&lon) || !(-90.0..=90.0).contains(&lat) {
            continue;
        }
        rows.push(SfoRow {
            date_micros: date.instant.and_utc().timestamp_micros(),
            event: event.to_string(),
            airline,
            journey,
            longitude: lon,
            latitude: lat,
        });
    }
    if rows.is_empty() {
        return Err(format!(
            "no SFO rows survived chart preparation from {}",
            input.display()
        )
        .into());
    }
    let top_airlines = top_categories(rows.iter().map(|row| row.airline.as_str()), 10);
    let top_journeys = top_categories(rows.iter().map(|row| row.journey.as_str()), 12);
    write_sfo_parquet(output, rows, &top_airlines, &top_journeys)
}

fn required_column<'a>(
    table: &'a dyn Table,
    name: &str,
) -> Result<algraf_data::ColumnView<'a>, Box<dyn std::error::Error>> {
    table
        .column(name)
        .ok_or_else(|| format!("missing required column `{name}`").into())
}

fn int_at(value: Option<DataValueRef<'_>>) -> Option<i64> {
    match value? {
        DataValueRef::Int(value) => Some(value),
        DataValueRef::Float(value) if value.is_finite() => Some(value.round() as i64),
        DataValueRef::String(value) => value.parse().ok(),
        _ => None,
    }
}

fn payment_label(payment: i64) -> &'static str {
    match payment {
        1 => "Credit card",
        2 => "Cash",
        3 => "No charge",
        4 => "Dispute",
        5 => "Unknown",
        6 => "Voided trip",
        _ => "Other",
    }
}

fn event_label(value: &str) -> Option<&'static str> {
    match value {
        "A" => Some("Arrival"),
        "D" => Some("Departure"),
        _ => None,
    }
}

fn top_categories<'a>(values: impl Iterator<Item = &'a str>, limit: usize) -> Vec<String> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for value in values {
        *counts.entry(value.to_string()).or_default() += 1;
    }
    let mut ranked: Vec<_> = counts.into_iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    ranked
        .into_iter()
        .take(limit)
        .map(|(value, _)| value)
        .collect()
}

fn grouped_value(value: &str, top_values: &[String]) -> String {
    if top_values.iter().any(|top| top == value) {
        value.to_string()
    } else {
        "Other".to_string()
    }
}

fn write_tlc_parquet(
    path: &Path,
    distances: Vec<Option<f64>>,
    totals: Vec<Option<f64>>,
    payments: Vec<Option<String>>,
    pickups: Vec<Option<i64>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("trip_distance", ArrowDataType::Float64, true),
        Field::new("total_amount", ArrowDataType::Float64, true),
        Field::new("payment_type", ArrowDataType::Utf8, true),
        Field::new(
            "tpep_pickup_datetime",
            ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
    ]));
    write_parquet(
        path,
        schema,
        vec![
            Arc::new(Float64Array::from(distances)) as ArrayRef,
            Arc::new(Float64Array::from(totals)) as ArrayRef,
            Arc::new(StringArray::from_iter(payments)) as ArrayRef,
            Arc::new(TimestampMicrosecondArray::from(pickups)) as ArrayRef,
        ],
    )
}

fn write_sfo_parquet(
    path: &Path,
    rows: Vec<SfoRow>,
    top_airlines: &[String],
    top_journeys: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(Schema::new(vec![
        Field::new(
            "date",
            ArrowDataType::Timestamp(TimeUnit::Microsecond, None),
            true,
        ),
        Field::new("event", ArrowDataType::Utf8, true),
        Field::new("airline", ArrowDataType::Utf8, true),
        Field::new("airline_group", ArrowDataType::Utf8, true),
        Field::new("journey", ArrowDataType::Utf8, true),
        Field::new("journey_group", ArrowDataType::Utf8, true),
        Field::new("longitude", ArrowDataType::Float64, true),
        Field::new("latitude", ArrowDataType::Float64, true),
    ]));
    write_parquet(
        path,
        schema,
        vec![
            Arc::new(TimestampMicrosecondArray::from_iter(
                rows.iter().map(|row| Some(row.date_micros)),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|row| Some(row.event.as_str())),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|row| Some(row.airline.as_str())),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter()
                    .map(|row| Some(grouped_value(&row.airline, top_airlines))),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter().map(|row| Some(row.journey.as_str())),
            )) as ArrayRef,
            Arc::new(StringArray::from_iter(
                rows.iter()
                    .map(|row| Some(grouped_value(&row.journey, top_journeys))),
            )) as ArrayRef,
            Arc::new(Float64Array::from_iter(
                rows.iter().map(|row| Some(row.longitude)),
            )) as ArrayRef,
            Arc::new(Float64Array::from_iter(
                rows.iter().map(|row| Some(row.latitude)),
            )) as ArrayRef,
        ],
    )
}

fn build_cli(root: &Path, profile: BuildProfile) -> Result<(), Box<dyn std::error::Error>> {
    for args in [
        ["-p", "algraf-cli", "", ""],
        ["-p", "algraf-render", "--bin", "render-timing"],
    ] {
        let mut command = Command::new("cargo");
        command.current_dir(root).arg("build");
        for arg in args.into_iter().filter(|arg| !arg.is_empty()) {
            command.arg(arg);
        }
        if matches!(profile, BuildProfile::Release) {
            command.arg("--release");
        }
        let status = command.status()?;
        if !status.success() {
            return Err(format!("cargo build {} failed", args.join(" ")).into());
        }
    }
    Ok(())
}

fn parquet_rows(path: &Path) -> Option<usize> {
    let loaded = algraf_data::read_parquet_path_projected(path, Some(&[])).ok()?;
    Some(loaded.frame.row_count())
}

fn csv_data_rows(path: &Path) -> Option<usize> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);
    let lines = reader.lines().map_while(Result::ok).count();
    Some(lines.saturating_sub(1))
}

fn mark_count(path: &Path) -> usize {
    let svg = fs::read_to_string(path).unwrap_or_default();
    [
        "<circle",
        "<rect",
        "<path",
        "<line",
        "<polyline",
        "<polygon",
        "<text",
    ]
    .iter()
    .map(|needle| svg.matches(needle).count())
    .sum()
}

fn byte_count(path: &Path) -> u64 {
    fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn git_ref(root: &Path) -> String {
    Command::new("git")
        .current_dir(root)
        .args(["describe", "--tags", "--always", "--dirty"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn command_output(root: &Path, program: &str, args: &[&str]) -> String {
    Command::new(program)
        .current_dir(root)
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string())
}

fn sanitize_label(label: &str) -> String {
    let sanitized: String = label
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "run".to_string()
    } else {
        sanitized
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug, Clone, Copy)]
struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Lcg { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        self.state
    }

    fn unit(&mut self) -> f64 {
        ((self.next_u64() >> 11) as f64) / ((1u64 << 53) as f64)
    }

    fn normalish(&mut self) -> f64 {
        (0..6).map(|_| self.unit()).sum::<f64>() - 3.0
    }
}
