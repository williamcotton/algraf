use std::collections::HashMap;
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use algraf_data::{read_parquet_path_projected, DataValueRef, Table};
use arrow_array::{ArrayRef, Float64Array, RecordBatch, StringArray, TimestampMicrosecondArray};
use arrow_schema::{DataType as ArrowDataType, Field, Schema, TimeUnit};
use parquet::arrow::ArrowWriter;

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

#[derive(Debug)]
struct Config {
    tlc_in: PathBuf,
    tlc_out: PathBuf,
    sfo_in: PathBuf,
    sfo_out: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_args()?;
    let mut prepared = false;
    if config.tlc_in.exists() {
        prepare_tlc(&config.tlc_in, &config.tlc_out)?;
        prepared = true;
    } else {
        eprintln!("missing TLC source: {}", config.tlc_in.display());
    }
    if config.sfo_in.exists() {
        prepare_sfo(&config.sfo_in, &config.sfo_out)?;
        prepared = true;
    } else {
        eprintln!("missing SFO source: {}", config.sfo_in.display());
    }
    if !prepared {
        return Err("no large fixture sources were available to prepare".into());
    }
    Ok(())
}

fn parse_args() -> Result<Config, String> {
    let mut tlc_in = PathBuf::from("benchdata/raw/tlc/yellow_tripdata_2024-01.parquet");
    let mut tlc_out = PathBuf::from("benchdata/prepared/tlc/yellow_tripdata_2024-01_chart.parquet");
    let mut sfo_in = PathBuf::from("benchdata/raw/sfo/sfomuseum-data-flights-2026-03.parquet");
    let mut sfo_out =
        PathBuf::from("benchdata/prepared/sfo/sfomuseum-data-flights-2026-03_chart.parquet");
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tlc-in" => {
                tlc_in = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--tlc-in requires a Parquet path".to_string())?,
                );
            }
            "--tlc-out" => {
                tlc_out = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--tlc-out requires a Parquet path".to_string())?,
                );
            }
            "--sfo-in" => {
                sfo_in = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--sfo-in requires a Parquet path".to_string())?,
                );
            }
            "--sfo-out" => {
                sfo_out = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--sfo-out requires a Parquet path".to_string())?,
                );
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    Ok(Config {
        tlc_in,
        tlc_out,
        sfo_in,
        sfo_out,
    })
}

fn print_help() {
    println!(
        "Usage: prepare_large_fixtures [--tlc-in PATH] [--tlc-out PATH] [--sfo-in PATH] [--sfo-out PATH]"
    );
    println!("Writes chart-ready prepared Parquet fixtures from downloaded external data.");
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

    let prepared_rows = distances.len();
    write_tlc_parquet(output, distances, totals, payments, pickups)?;
    println!(
        "wrote {} prepared TLC rows to {}",
        prepared_rows,
        output.display()
    );
    Ok(())
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
    let prepared_rows = rows.len();
    write_sfo_parquet(output, rows, &top_airlines, &top_journeys)?;
    println!(
        "wrote {} prepared SFO rows to {}",
        prepared_rows,
        output.display()
    );
    Ok(())
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

fn write_tlc_parquet(
    path: &Path,
    distances: Vec<Option<f64>>,
    totals: Vec<Option<f64>>,
    payments: Vec<Option<String>>,
    pickups: Vec<Option<i64>>,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
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
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Float64Array::from(distances)) as ArrayRef,
            Arc::new(Float64Array::from(totals)) as ArrayRef,
            Arc::new(StringArray::from_iter(payments)) as ArrayRef,
            Arc::new(TimestampMicrosecondArray::from(pickups)) as ArrayRef,
        ],
    )?;
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
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

fn write_sfo_parquet(
    path: &Path,
    rows: Vec<SfoRow>,
    top_airlines: &[String],
    top_journeys: &[String],
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
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
    let batch = RecordBatch::try_new(
        schema.clone(),
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
    )?;
    let file = File::create(path)?;
    let mut writer = ArrowWriter::try_new(file, schema, None)?;
    writer.write(&batch)?;
    writer.close()?;
    Ok(())
}
