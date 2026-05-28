//! CSV loading, schema inference, and type inference tests (spec §10, §27.1).

use algraf_data::{
    parse_temporal, read_csv_schema_str, read_csv_str, read_csv_str_with_temporal_policy,
    DataError, DataType, DataValueRef, EpochUnit, Table, TemporalColumnParse, TemporalParsePolicy,
    TemporalParseType, TemporalPrecision, TemporalTimezone, DEFAULT_SCHEMA_SAMPLE,
};

fn load(input: &str) -> algraf_data::LoadResult {
    read_csv_str(input).expect("csv should load")
}

fn dtype(input: &str, column: &str) -> DataType {
    let frame = load(input).frame;
    frame.column_def(column).expect("column exists").dtype
}

#[test]
fn test_headers_define_columns() {
    let frame = load("a,b,c\n1,2,3\n").frame;
    let names: Vec<&str> = frame.column_names().collect();
    assert_eq!(names, vec!["a", "b", "c"]);
    assert_eq!(frame.row_count(), 1);
}

#[test]
fn test_duplicate_headers_rejected() {
    let err = read_csv_str("a,a\n1,2\n").unwrap_err();
    assert!(matches!(err, DataError::DuplicateHeader(_)));
}

#[test]
fn test_integer_inference() {
    assert_eq!(dtype("n\n1\n2\n1000\n", "n"), DataType::Integer);
}

#[test]
fn test_float_inference_when_mixed_int_and_decimal() {
    // A numeric column with a decimal becomes Float; integers widen to float.
    assert_eq!(dtype("x\n1\n2.5\n3\n", "x"), DataType::Float);
}

#[test]
fn test_boolean_inference() {
    assert_eq!(
        dtype("flag\ntrue\nfalse\nTrue\n", "flag"),
        DataType::Boolean
    );
}

#[test]
fn test_string_inference() {
    assert_eq!(dtype("name\nadelie\ngentoo\n", "name"), DataType::String);
}

#[test]
fn test_mixed_numeric_and_string_is_mixed() {
    // Mixed numeric/string columns are categorical (spec §10.3).
    assert_eq!(dtype("v\n1\n2\nabc\n", "v"), DataType::Mixed);
}

#[test]
fn test_missing_tokens_do_not_force_mixed() {
    // NA / empty in an otherwise-integer column stay missing, not Mixed.
    let input = "n,k\n1,a\n,b\n2,c\nNA,d\nNaN,e\nnull,f\n";
    let result = load(input);
    let frame = &result.frame;
    let def = frame.column_def("n").unwrap();
    assert_eq!(def.dtype, DataType::Integer);
    assert!(def.nullable);
    // The blank second row is a present-but-null cell.
    assert!(matches!(frame.value("n", 1), Some(DataValueRef::Null)));
    assert!(matches!(frame.value("n", 0), Some(DataValueRef::Int(1))));
}

#[test]
fn test_all_missing_column_is_unknown() {
    assert_eq!(dtype("e\n\n\nNA\n", "e"), DataType::Unknown);
}

#[test]
fn test_temporal_date_inference() {
    let frame = load("d\n2020-01-01\n2020-06-15\n").frame;
    assert_eq!(frame.column_def("d").unwrap().dtype, DataType::Temporal);
    let DataValueRef::Temporal(value) = frame.value("d", 0).unwrap() else {
        panic!("expected temporal");
    };
    assert_eq!(value.precision, TemporalPrecision::Date);
}

#[test]
fn test_temporal_datetime_inference() {
    let frame = load("t\n2020-01-01T12:30:00\n2020-01-02T08:00:00\n").frame;
    let DataValueRef::Temporal(value) = frame.value("t", 0).unwrap() else {
        panic!("expected temporal");
    };
    assert_eq!(value.precision, TemporalPrecision::DateTime);
}

#[test]
fn test_mixed_date_and_datetime_is_datetime_column() {
    // A column mixing dates and datetimes is temporal; the date lifts to
    // midnight (spec §10.3).
    let frame = load("t\n2020-01-01\n2020-01-02T06:00:00\n").frame;
    assert_eq!(frame.column_def("t").unwrap().dtype, DataType::Temporal);
}

#[test]
fn test_rfc3339_offset_converted_to_utc() {
    // 12:00:00+02:00 is 10:00:00Z.
    let parsed = parse_temporal("2020-01-01T12:00:00+02:00").unwrap();
    assert!(parsed.offset_aware);
    assert_eq!(parsed.value.instant.format("%H:%M").to_string(), "10:00");
}

#[test]
fn test_naive_datetime_not_offset_aware() {
    let parsed = parse_temporal("2020-01-01T12:00:00").unwrap();
    assert!(!parsed.offset_aware);
    assert_eq!(parsed.value.instant.format("%H:%M").to_string(), "12:00");
}

#[test]
fn test_space_separated_datetime() {
    assert!(parse_temporal("2020-01-01 09:15:00").is_some());
}

#[test]
fn test_space_separated_minute_datetime() {
    let parsed = parse_temporal("2026-05-25 14:30").expect("minute datetime");
    assert_eq!(parsed.value.precision, TemporalPrecision::DateTime);
    assert_eq!(
        parsed.value.instant.format("%Y-%m-%d %H:%M").to_string(),
        "2026-05-25 14:30"
    );
}

#[test]
fn test_non_temporal_strings_stay_strings() {
    assert!(parse_temporal("not a date").is_none());
    assert!(parse_temporal("2020").is_none());
    assert!(parse_temporal("12:30:00").is_none());
    assert!(parse_temporal("01/02/2026").is_none());
}

#[test]
fn test_broader_unambiguous_temporal_inference_matrix() {
    for accepted in [
        "2026-05-27T14:30",
        "2026-05-27T14:30:15.250",
        "2026-05-27 14:30:15.250",
        "2026/05/27",
        "20260527",
        "Wed, 27 May 2026 14:30:00 -0500",
        "May 27, 2026",
        "27 May 2026 14:30",
    ] {
        assert!(parse_temporal(accepted).is_some(), "{accepted}");
    }
    for rejected in ["02-01-2026", "26", "2026-05", "14:30"] {
        assert!(parse_temporal(rejected).is_none(), "{rejected}");
    }
}

#[test]
fn test_explicit_temporal_policy_parses_ambiguous_dates() {
    let policy = TemporalParsePolicy {
        columns: vec![TemporalColumnParse {
            column: "started".to_string(),
            as_type: TemporalParseType::DateTime,
            formats: vec!["%m/%d/%Y %I:%M %p".to_string()],
            unit: None,
            timezone: TemporalTimezone::Utc,
        }],
    };
    let result = read_csv_str_with_temporal_policy(
        "started,latency\n05/27/2026 2:30 PM,82\nbad,91\n",
        Some(&policy),
    )
    .unwrap();
    assert_eq!(
        result.frame.column_def("started").unwrap().dtype,
        DataType::Temporal
    );
    let DataValueRef::Temporal(value) = result.frame.value("started", 0).unwrap() else {
        panic!("expected temporal");
    };
    assert_eq!(
        value.instant.format("%Y-%m-%d %H:%M").to_string(),
        "2026-05-27 14:30"
    );
    assert!(matches!(
        result.frame.value("started", 1),
        Some(DataValueRef::Null)
    ));
    assert!(result
        .warnings
        .iter()
        .any(|warning| warning.message.contains("failed explicit temporal parsing")));
}

#[test]
fn test_explicit_epoch_milliseconds_policy() {
    let policy = TemporalParsePolicy {
        columns: vec![TemporalColumnParse {
            column: "observed".to_string(),
            as_type: TemporalParseType::DateTime,
            formats: Vec::new(),
            unit: Some(EpochUnit::Milliseconds),
            timezone: TemporalTimezone::Utc,
        }],
    };
    let result =
        read_csv_str_with_temporal_policy("observed\n1780065000000\n", Some(&policy)).unwrap();
    let DataValueRef::Temporal(value) = result.frame.value("observed", 0).unwrap() else {
        panic!("expected temporal");
    };
    assert_eq!(value.precision, TemporalPrecision::DateTime);
}

#[test]
fn test_mixed_naive_and_offset_warns() {
    let result = load("t\n2020-01-01T12:00:00\n2020-01-02T08:00:00Z\n");
    assert_eq!(
        result.frame.column_def("t").unwrap().dtype,
        DataType::Temporal
    );
    assert!(result
        .warnings
        .iter()
        .any(|w| w.column.as_deref() == Some("t")));
}

#[test]
fn test_quoted_fields_with_commas() {
    let frame = load("label,n\n\"a, b\",1\n\"c\",2\n").frame;
    let DataValueRef::String(label) = frame.value("label", 0).unwrap() else {
        panic!("expected string");
    };
    assert_eq!(label, "a, b");
}

#[test]
fn test_examples_are_collected() {
    let def = load("n\n10\n20\n30\n40\n")
        .frame
        .column_def("n")
        .unwrap()
        .clone();
    assert_eq!(def.examples, vec!["10", "20", "30"]);
}

#[test]
fn test_row_view() {
    let frame = load("a,b\n1,x\n2,y\n").frame;
    let row = frame.row(1).unwrap();
    assert!(matches!(row.get("a"), Some(DataValueRef::Int(2))));
    assert!(matches!(row.get("b"), Some(DataValueRef::String("y"))));
    assert!(frame.row(99).is_none());
}

#[test]
fn test_schema_only_sampling_reads_headers() {
    let schema = read_csv_schema_str("a,b\n1,2\n", DEFAULT_SCHEMA_SAMPLE).unwrap();
    let names: Vec<&str> = schema.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["a", "b"]);
    assert_eq!(schema[0].dtype, DataType::Integer);
}
