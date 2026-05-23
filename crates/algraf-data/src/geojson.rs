//! GeoJSON loading (spec §10.1, §10.11).
//!
//! A GeoJSON `FeatureCollection` decodes to the same column-oriented
//! [`DataFrame`](crate::DataFrame) shape as every other source: one row per
//! feature, in file order. Each `properties` key becomes a scalar column run
//! through the shared type-inference pipeline (spec §10.3) — so a numeric
//! property infers exactly as it would from CSV — and each feature's
//! `geometry` becomes a single [`DataType::Geometry`](crate::DataType) column
//! named [`GEOMETRY_COLUMN`]. The geometry decodes to `geo_types`, identical to
//! the shapefile loader, so both share the spatial scale and `Geo` render path.

use std::io::Read;

use geo_types::Geometry;
use geojson::GeoJson;
use indexmap::IndexMap;

use crate::csv::LoadResult;
use crate::error::DataError;
use crate::frame::{Column, DataFrame};
use crate::infer::infer_column;
use crate::json::json_cell;
use crate::schema::{ColumnDef, DataType};

/// The column name assigned to each feature's geometry (spec §10.11).
pub const GEOMETRY_COLUMN: &str = "geom";

/// Fully load a GeoJSON `FeatureCollection` from a reader (spec §10.11).
pub fn read_geojson<R: Read>(mut reader: R) -> Result<LoadResult, DataError> {
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    read_geojson_str(&text)
}

/// Load GeoJSON data from a string.
pub fn read_geojson_str(input: &str) -> Result<LoadResult, DataError> {
    let geojson: GeoJson = input
        .parse()
        .map_err(|e: geojson::Error| DataError::Geo(e.to_string()))?;

    let features = match geojson {
        GeoJson::FeatureCollection(fc) => fc.features,
        GeoJson::Feature(feature) => vec![feature],
        GeoJson::Geometry(_) => {
            return Err(DataError::Geo(
                "expected a GeoJSON FeatureCollection or Feature, found a bare geometry"
                    .to_string(),
            ))
        }
    };

    // Property columns, discovered in first-seen key order across features; a
    // key absent from a feature is a missing cell. Mirrors the JSON loader.
    let mut prop_names: Vec<String> = Vec::new();
    let mut prop_index: IndexMap<String, usize> = IndexMap::new();
    let mut prop_cols: Vec<Vec<String>> = Vec::new();
    let mut geoms: Vec<Option<Geometry<f64>>> = Vec::with_capacity(features.len());

    for (row, feature) in features.iter().enumerate() {
        for column in &mut prop_cols {
            column.push(String::new());
        }
        if let Some(properties) = &feature.properties {
            for (key, value) in properties {
                let index = *prop_index.entry(key.clone()).or_insert_with(|| {
                    prop_names.push(key.clone());
                    prop_cols.push(vec![String::new(); row + 1]);
                    prop_names.len() - 1
                });
                prop_cols[index][row] = json_cell(value);
            }
        }

        match &feature.geometry {
            Some(geometry) => {
                let geom = Geometry::<f64>::try_from(geometry.clone())
                    .map_err(|e: geojson::Error| DataError::Geo(e.to_string()))?;
                geoms.push(Some(geom));
            }
            None => geoms.push(None),
        }
    }

    Ok(build_with_geometry(prop_names, prop_cols, geoms))
}

/// Assemble property columns (through inference) plus the geometry column into
/// a [`LoadResult`]. The `geom` column is appended last (spec §10.11).
pub(crate) fn build_with_geometry(
    prop_names: Vec<String>,
    prop_cols: Vec<Vec<String>>,
    geoms: Vec<Option<Geometry<f64>>>,
) -> LoadResult {
    let row_count = geoms.len();
    let mut schema = Vec::with_capacity(prop_names.len() + 1);
    let mut columns = Vec::with_capacity(prop_names.len() + 1);
    let mut warnings = Vec::new();

    for (name, raw) in prop_names.iter().zip(prop_cols) {
        // Pad short columns so every column has one cell per feature.
        let mut raw = raw;
        raw.resize(row_count, String::new());
        let inferred = infer_column(name, &raw);
        schema.push(inferred.def);
        columns.push(inferred.column);
        warnings.extend(inferred.warnings);
    }

    let nullable = geoms.iter().any(Option::is_none);
    schema.push(ColumnDef {
        name: GEOMETRY_COLUMN.to_string(),
        dtype: DataType::Geometry,
        nullable,
        examples: Vec::new(),
    });
    columns.push(Column::Geometry(geoms));

    LoadResult {
        frame: DataFrame::new(schema, columns),
        warnings,
    }
}
