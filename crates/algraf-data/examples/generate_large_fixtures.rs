use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{
    ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray, TimestampMicrosecondArray,
};
use arrow_schema::{DataType as ArrowDataType, Field, Schema, TimeUnit};
use parquet::arrow::ArrowWriter;

#[derive(Debug, Clone, Copy)]
struct Tier {
    name: &'static str,
    rows: usize,
    wide_columns: usize,
}

impl Tier {
    fn parse(value: &str) -> Result<Tier, String> {
        match value {
            "smoke" => Ok(Tier {
                name: "smoke",
                rows: 8_000,
                wide_columns: 32,
            }),
            "local" => Ok(Tier {
                name: "local",
                rows: 250_000,
                wide_columns: 64,
            }),
            "stress" => Ok(Tier {
                name: "stress",
                rows: 1_000_000,
                wide_columns: 96,
            }),
            _ => Err(format!(
                "unsupported tier {value:?}; expected smoke, local, or stress"
            )),
        }
    }
}

#[derive(Debug)]
struct Config {
    tier: Tier,
    out_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct Paths {
    dense_points: PathBuf,
    tall_events: PathBuf,
    sparse_nullable: PathBuf,
    high_cardinality: PathBuf,
    wide_metrics: PathBuf,
    tall_csv: PathBuf,
    tall_ndjson: PathBuf,
    manifest: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = parse_args()?;
    let tier_dir = config.out_dir.join(config.tier.name);
    fs::create_dir_all(&tier_dir)?;
    let paths = Paths {
        dense_points: tier_dir.join("dense_points.parquet"),
        tall_events: tier_dir.join("tall_events.parquet"),
        sparse_nullable: tier_dir.join("sparse_nullable.parquet"),
        high_cardinality: tier_dir.join("high_cardinality.parquet"),
        wide_metrics: tier_dir.join("wide_metrics.parquet"),
        tall_csv: tier_dir.join("tall_events.csv"),
        tall_ndjson: tier_dir.join("tall_events.ndjson"),
        manifest: tier_dir.join("manifest.json"),
    };

    write_dense_points(&paths.dense_points, config.tier.rows)?;
    write_tall_events(&paths, config.tier.rows)?;
    write_sparse_nullable(&paths.sparse_nullable, config.tier.rows)?;
    write_high_cardinality(&paths.high_cardinality, config.tier.rows)?;
    write_wide_metrics(
        &paths.wide_metrics,
        config.tier.rows.min(80_000),
        config.tier.wide_columns,
    )?;
    write_manifest(&paths, config.tier)?;

    println!("wrote {}", paths.manifest.display());
    Ok(())
}

fn parse_args() -> Result<Config, String> {
    let mut tier = Tier::parse("smoke")?;
    let mut out_dir = PathBuf::from("target/algraf-large-fixtures");
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--tier" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--tier requires smoke, local, or stress".to_string())?;
                tier = Tier::parse(&value)?;
            }
            "--out" => {
                out_dir = PathBuf::from(
                    args.next()
                        .ok_or_else(|| "--out requires a directory".to_string())?,
                );
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unexpected argument {other:?}")),
        }
    }
    Ok(Config { tier, out_dir })
}

fn print_help() {
    println!("Usage: generate_large_fixtures [--tier smoke|local|stress] [--out DIR]");
    println!("Writes deterministic Parquet fixtures plus CSV/NDJSON mirrors for tall_events.");
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

fn write_tall_events(paths: &Paths, rows: usize) -> Result<(), Box<dyn std::error::Error>> {
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

fn write_manifest(paths: &Paths, tier: Tier) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = BufWriter::new(File::create(&paths.manifest)?);
    writeln!(file, "{{")?;
    writeln!(file, "  \"tier\": \"{}\",", tier.name)?;
    writeln!(file, "  \"rows\": {},", tier.rows)?;
    writeln!(file, "  \"wide_columns\": {},", tier.wide_columns)?;
    writeln!(file, "  \"files\": {{")?;
    writeln!(
        file,
        "    \"dense_points\": \"{}\",",
        paths.dense_points.display()
    )?;
    writeln!(
        file,
        "    \"tall_events\": \"{}\",",
        paths.tall_events.display()
    )?;
    writeln!(
        file,
        "    \"sparse_nullable\": \"{}\",",
        paths.sparse_nullable.display()
    )?;
    writeln!(
        file,
        "    \"high_cardinality\": \"{}\",",
        paths.high_cardinality.display()
    )?;
    writeln!(
        file,
        "    \"wide_metrics\": \"{}\",",
        paths.wide_metrics.display()
    )?;
    writeln!(file, "    \"tall_csv\": \"{}\",", paths.tall_csv.display())?;
    writeln!(
        file,
        "    \"tall_ndjson\": \"{}\"",
        paths.tall_ndjson.display()
    )?;
    writeln!(file, "  }}")?;
    writeln!(file, "}}")?;
    Ok(())
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
