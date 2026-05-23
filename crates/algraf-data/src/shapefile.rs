//! Shapefile loading (spec §10.1, §10.11).
//!
//! An ESRI shapefile is a bundle: the `.shp` binary supplies geometry and the
//! sidecar `.dbf` supplies attributes. The constructor names the `.shp`; the
//! `shapefile` crate resolves the `.dbf`/`.shx` sidecars next to it. Records
//! decode in file order to the same column-oriented
//! [`DataFrame`](crate::DataFrame) shape as GeoJSON — attribute columns through
//! the shared type-inference pipeline (spec §10.3) plus one
//! [`DataType::Geometry`](crate::DataType) column named
//! [`GEOMETRY_COLUMN`](crate::geojson::GEOMETRY_COLUMN) — and the geometry
//! decodes to the same `geo_types`, so both formats share the spatial scale,
//! projection, and `Geo` render path.

use std::path::Path;

use geo_types::Geometry;
use indexmap::IndexMap;
use shapefile::dbase::FieldValue;
use shapefile::Shape;

use crate::csv::LoadResult;
use crate::error::DataError;
use crate::geojson::build_with_geometry;

/// Fully load a shapefile bundle from the path to its `.shp` file (spec §10.11).
/// The `.dbf` and `.shx` sidecars are resolved next to it by the reader.
pub fn read_shapefile_path(path: &Path) -> Result<LoadResult, DataError> {
    let mut reader =
        shapefile::Reader::from_path(path).map_err(|e| DataError::Geo(e.to_string()))?;

    let mut field_names: Vec<String> = Vec::new();
    let mut field_index: IndexMap<String, usize> = IndexMap::new();
    let mut field_cols: Vec<Vec<String>> = Vec::new();
    let mut geoms: Vec<Option<Geometry<f64>>> = Vec::new();

    for (row, shape_record) in reader.iter_shapes_and_records().enumerate() {
        let (shape, record) = shape_record.map_err(|e| DataError::Geo(e.to_string()))?;

        for column in &mut field_cols {
            column.push(String::new());
        }
        for (name, value) in record {
            let index = *field_index.entry(name.clone()).or_insert_with(|| {
                field_names.push(name.clone());
                field_cols.push(vec![String::new(); row + 1]);
                field_names.len() - 1
            });
            field_cols[index][row] = field_cell(&value);
        }

        geoms.push(shape_to_geometry(shape)?);
    }

    Ok(build_with_geometry(field_names, field_cols, geoms))
}

/// Convert a shapefile shape to a `geo_types` geometry. A null shape is a
/// missing cell; anything the converter rejects is a parse error (`E1805`).
fn shape_to_geometry(shape: Shape) -> Result<Option<Geometry<f64>>, DataError> {
    if matches!(shape, Shape::NullShape) {
        return Ok(None);
    }
    Geometry::<f64>::try_from(shape)
        .map(Some)
        .map_err(|e| DataError::Geo(e.to_string()))
}

/// Render one dBASE field value to the canonical text the inference pipeline
/// expects. A null/absent value becomes the empty (missing) string; an
/// integral number prints without a trailing decimal so it infers as an
/// integer, matching CSV and GeoJSON (spec §10.3, §10.11).
fn field_cell(value: &FieldValue) -> String {
    match value {
        FieldValue::Character(Some(s)) => s.clone(),
        FieldValue::Numeric(Some(n)) | FieldValue::Double(n) => num_cell(*n),
        FieldValue::Float(Some(f)) => num_cell(*f as f64),
        FieldValue::Currency(c) => num_cell(*c),
        FieldValue::Integer(i) => i.to_string(),
        FieldValue::Logical(Some(b)) => b.to_string(),
        FieldValue::Date(Some(d)) => {
            format!("{:04}-{:02}-{:02}", d.year(), d.month(), d.day())
        }
        FieldValue::Memo(s) => s.clone(),
        // Null/absent values, and datetime variants we do not surface, are
        // treated as missing cells.
        _ => String::new(),
    }
}

/// Format a float as an integer when it is integral, else with its natural
/// shortest representation.
fn num_cell(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}
