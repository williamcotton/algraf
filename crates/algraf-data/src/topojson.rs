//! TopoJSON loading (spec §10.11).
//!
//! TopoJSON encodes topology: shared boundaries are stored once as **arcs** and
//! geometries reference them by index. A TopoJSON `Topology` decodes to the same
//! column-oriented [`DataFrame`](crate::DataFrame) shape as GeoJSON — one row per
//! geometry in the named object, each `properties` key a scalar column through
//! the shared inference pipeline (spec §10.3), and the geometry itself the
//! [`GEOMETRY_COLUMN`](crate::geojson::GEOMETRY_COLUMN). Arcs are stitched and
//! (optionally) de-quantized into `geo_types`, so a TopoJSON object and an
//! equivalent GeoJSON `FeatureCollection` produce the same dataframe and share
//! the spatial scale and `Geo` render path.

use std::io::Read;

use geo_types::{
    Coord, Geometry, LineString, MultiLineString, MultiPoint, MultiPolygon, Point, Polygon,
};
use indexmap::IndexMap;
use serde_json::Value;

use crate::csv::LoadResult;
use crate::error::DataError;
use crate::geojson::build_with_geometry;
use crate::json::json_cell;

/// Fully load a TopoJSON object from a reader (spec §10.11).
pub fn read_topojson<R: Read>(
    mut reader: R,
    object: Option<&str>,
) -> Result<LoadResult, DataError> {
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    read_topojson_str(&text, object)
}

/// The optional quantization transform applied to delta-encoded arc and point
/// coordinates (`q = position * scale + translate`).
struct Transform {
    scale: [f64; 2],
    translate: [f64; 2],
}

/// Fully load a TopoJSON object from a string. `object` names the entry in the
/// topology's `objects` map; `None` selects the sole object, and is ambiguous
/// when the topology defines more than one (spec §10.11).
pub fn read_topojson_str(input: &str, object: Option<&str>) -> Result<LoadResult, DataError> {
    let root: Value = serde_json::from_str(input).map_err(|e| DataError::Geo(e.to_string()))?;

    if root.get("type").and_then(Value::as_str) != Some("Topology") {
        return Err(DataError::Geo(
            "expected a TopoJSON document with \"type\": \"Topology\"".to_string(),
        ));
    }

    let transform = parse_transform(root.get("transform"))?;
    let raw_arcs = root
        .get("arcs")
        .and_then(Value::as_array)
        .ok_or_else(|| DataError::Geo("TopoJSON topology has no `arcs` array".to_string()))?;
    let arcs = decode_arcs(raw_arcs, transform.as_ref())?;

    let objects = root
        .get("objects")
        .and_then(Value::as_object)
        .ok_or_else(|| DataError::Geo("TopoJSON topology has no `objects` map".to_string()))?;

    let selected = match object {
        Some(name) => objects.get(name).ok_or_else(|| {
            let available: Vec<&str> = objects.keys().map(String::as_str).collect();
            DataError::Geo(format!(
                "TopoJSON object `{name}` not found; available objects: {}",
                available.join(", ")
            ))
        })?,
        None => {
            let mut entries = objects.iter();
            match (entries.next(), entries.next()) {
                (Some((_, value)), None) => value,
                (Some(_), Some(_)) => {
                    let available: Vec<&str> = objects.keys().map(String::as_str).collect();
                    return Err(DataError::Geo(format!(
                        "TopoJSON topology defines multiple objects ({}); \
                         name one with `object:`",
                        available.join(", ")
                    )));
                }
                _ => {
                    return Err(DataError::Geo(
                        "TopoJSON topology has no objects".to_string(),
                    ))
                }
            }
        }
    };

    // A GeometryCollection object yields one row per member geometry (like a
    // GeoJSON FeatureCollection); any other object is a single feature.
    let members: Vec<&Value> = match selected.get("type").and_then(Value::as_str) {
        Some("GeometryCollection") => selected
            .get("geometries")
            .and_then(Value::as_array)
            .map(|g| g.iter().collect())
            .unwrap_or_default(),
        _ => vec![selected],
    };

    let mut prop_names: Vec<String> = Vec::new();
    let mut prop_index: IndexMap<String, usize> = IndexMap::new();
    let mut prop_cols: Vec<Vec<String>> = Vec::new();
    let mut geoms: Vec<Option<Geometry<f64>>> = Vec::with_capacity(members.len());

    for (row, member) in members.iter().enumerate() {
        for column in &mut prop_cols {
            column.push(String::new());
        }
        if let Some(properties) = member.get("properties").and_then(Value::as_object) {
            for (key, value) in properties {
                let index = *prop_index.entry(key.clone()).or_insert_with(|| {
                    prop_names.push(key.clone());
                    prop_cols.push(vec![String::new(); row + 1]);
                    prop_names.len() - 1
                });
                prop_cols[index][row] = json_cell(value);
            }
        }
        geoms.push(decode_geometry(member, &arcs, transform.as_ref())?);
    }

    Ok(build_with_geometry(prop_names, prop_cols, geoms))
}

fn parse_transform(value: Option<&Value>) -> Result<Option<Transform>, DataError> {
    let Some(transform) = value else {
        return Ok(None);
    };
    let pair = |key: &str| -> Result<[f64; 2], DataError> {
        let arr = transform
            .get(key)
            .and_then(Value::as_array)
            .filter(|a| a.len() == 2)
            .ok_or_else(|| DataError::Geo(format!("TopoJSON transform `{key}` must be [x, y]")))?;
        Ok([num(&arr[0]), num(&arr[1])])
    };
    Ok(Some(Transform {
        scale: pair("scale")?,
        translate: pair("translate")?,
    }))
}

/// Decode every arc into absolute `(x, y)` coordinates. Delta-encoded arcs (when
/// a transform is present) accumulate per arc; otherwise positions are absolute.
fn decode_arcs(
    raw: &[Value],
    transform: Option<&Transform>,
) -> Result<Vec<Vec<Coord<f64>>>, DataError> {
    raw.iter()
        .map(|arc| {
            let positions = arc
                .as_array()
                .ok_or_else(|| DataError::Geo("TopoJSON arc must be an array".to_string()))?;
            let mut x = 0.0;
            let mut y = 0.0;
            let mut out = Vec::with_capacity(positions.len());
            for pos in positions {
                let p = pos.as_array().filter(|a| a.len() >= 2).ok_or_else(|| {
                    DataError::Geo("TopoJSON arc position must be [x, y]".to_string())
                })?;
                match transform {
                    Some(t) => {
                        x += num(&p[0]);
                        y += num(&p[1]);
                        out.push(Coord {
                            x: x * t.scale[0] + t.translate[0],
                            y: y * t.scale[1] + t.translate[1],
                        });
                    }
                    None => out.push(Coord {
                        x: num(&p[0]),
                        y: num(&p[1]),
                    }),
                }
            }
            Ok(out)
        })
        .collect()
}

/// Stitch a line from a list of arc indices. A negative index `i` references arc
/// `-i - 1` reversed; consecutive arcs share an endpoint, so all but the first
/// drop their leading coordinate (the standard TopoJSON stitching rule).
fn stitch(arc_list: &[Value], arcs: &[Vec<Coord<f64>>]) -> Result<Vec<Coord<f64>>, DataError> {
    let mut coords: Vec<Coord<f64>> = Vec::new();
    for (k, index) in arc_list.iter().enumerate() {
        let i = index
            .as_i64()
            .ok_or_else(|| DataError::Geo("TopoJSON arc index must be an integer".to_string()))?;
        let (resolved, reversed) = if i < 0 {
            ((-i - 1) as usize, true)
        } else {
            (i as usize, false)
        };
        let arc = arcs
            .get(resolved)
            .ok_or_else(|| DataError::Geo(format!("TopoJSON arc index {i} out of range")))?;
        let mut segment: Vec<Coord<f64>> = arc.clone();
        if reversed {
            segment.reverse();
        }
        if k > 0 && !segment.is_empty() {
            segment.remove(0);
        }
        coords.extend(segment);
    }
    Ok(coords)
}

fn decode_geometry(
    value: &Value,
    arcs: &[Vec<Coord<f64>>],
    transform: Option<&Transform>,
) -> Result<Option<Geometry<f64>>, DataError> {
    let Some(kind) = value.get("type").and_then(Value::as_str) else {
        // A null geometry (the TopoJSON `{"type": null}` member) is a missing cell.
        return Ok(None);
    };
    let geometry = match kind {
        "Point" => Geometry::Point(Point(point_coord(value.get("coordinates"), transform)?)),
        "MultiPoint" => {
            let positions = coord_array(value.get("coordinates"))?;
            let points = positions
                .iter()
                .map(|p| Ok(Point(transform_point(p, transform)?)))
                .collect::<Result<Vec<_>, DataError>>()?;
            Geometry::MultiPoint(MultiPoint(points))
        }
        "LineString" => Geometry::LineString(LineString(stitch(arc_indices(value)?, arcs)?)),
        "MultiLineString" => {
            let lines = arc_index_lists(value)?
                .iter()
                .map(|line| {
                    let line = line.as_array().ok_or_else(|| {
                        DataError::Geo("TopoJSON line arcs must be an array".to_string())
                    })?;
                    Ok(LineString(stitch(line, arcs)?))
                })
                .collect::<Result<Vec<_>, DataError>>()?;
            Geometry::MultiLineString(MultiLineString(lines))
        }
        "Polygon" => Geometry::Polygon(decode_polygon(arc_index_lists(value)?, arcs)?),
        "MultiPolygon" => {
            let polys = value
                .get("arcs")
                .and_then(Value::as_array)
                .ok_or_else(|| DataError::Geo("TopoJSON MultiPolygon has no `arcs`".to_string()))?
                .iter()
                .map(|rings| {
                    let rings = rings.as_array().ok_or_else(|| {
                        DataError::Geo("TopoJSON polygon rings must be arrays".to_string())
                    })?;
                    decode_polygon(rings, arcs)
                })
                .collect::<Result<Vec<_>, DataError>>()?;
            Geometry::MultiPolygon(MultiPolygon(polys))
        }
        other => {
            return Err(DataError::Geo(format!(
                "unsupported TopoJSON geometry type `{other}`"
            )))
        }
    };
    Ok(Some(geometry))
}

fn decode_polygon(rings: &[Value], arcs: &[Vec<Coord<f64>>]) -> Result<Polygon<f64>, DataError> {
    let mut decoded: Vec<LineString<f64>> = rings
        .iter()
        .map(|ring| {
            let ring = ring.as_array().ok_or_else(|| {
                DataError::Geo("TopoJSON polygon ring must be an array".to_string())
            })?;
            Ok(LineString(stitch(ring, arcs)?))
        })
        .collect::<Result<Vec<_>, DataError>>()?;
    if decoded.is_empty() {
        return Ok(Polygon::new(LineString(Vec::new()), Vec::new()));
    }
    let exterior = decoded.remove(0);
    Ok(Polygon::new(exterior, decoded))
}

fn arc_indices(value: &Value) -> Result<&Vec<Value>, DataError> {
    value
        .get("arcs")
        .and_then(Value::as_array)
        .ok_or_else(|| DataError::Geo("TopoJSON geometry has no `arcs`".to_string()))
}

fn arc_index_lists(value: &Value) -> Result<&Vec<Value>, DataError> {
    arc_indices(value)
}

fn point_coord(
    value: Option<&Value>,
    transform: Option<&Transform>,
) -> Result<Coord<f64>, DataError> {
    let position = value
        .and_then(Value::as_array)
        .filter(|a| a.len() >= 2)
        .ok_or_else(|| DataError::Geo("TopoJSON point coordinates must be [x, y]".to_string()))?;
    transform_point(position, transform)
}

fn coord_array(value: Option<&Value>) -> Result<Vec<Vec<Value>>, DataError> {
    value
        .and_then(Value::as_array)
        .map(|positions| {
            positions
                .iter()
                .filter_map(|p| p.as_array().cloned())
                .collect()
        })
        .ok_or_else(|| DataError::Geo("TopoJSON coordinates must be an array".to_string()))
}

/// Apply the quantization transform to a single (non-delta) point position.
fn transform_point(
    position: &[Value],
    transform: Option<&Transform>,
) -> Result<Coord<f64>, DataError> {
    if position.len() < 2 {
        return Err(DataError::Geo(
            "TopoJSON point must have x and y".to_string(),
        ));
    }
    let (x, y) = (num(&position[0]), num(&position[1]));
    Ok(match transform {
        Some(t) => Coord {
            x: x * t.scale[0] + t.translate[0],
            y: y * t.scale[1] + t.translate[1],
        },
        None => Coord { x, y },
    })
}

fn num(value: &Value) -> f64 {
    value.as_f64().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DataType, Table};

    const SAMPLE: &str = r#"{
      "type": "Topology",
      "objects": {
        "regions": {
          "type": "GeometryCollection",
          "geometries": [
            {"type": "Polygon", "arcs": [[0, 1]], "properties": {"name": "A", "pop": 10}},
            {"type": "Polygon", "arcs": [[-2, 2]], "properties": {"name": "B", "pop": 20}}
          ]
        }
      },
      "arcs": [
        [[0, 0], [10, 0], [0, 10]],
        [[10, 10], [-10, 0], [0, -10]],
        [[10, 10], [10, 0], [0, 10], [-10, 0]]
      ]
    }"#;

    fn dtype(frame: &crate::DataFrame, name: &str) -> DataType {
        frame
            .schema()
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("column {name}"))
            .dtype
    }

    #[test]
    fn decodes_object_into_geometry_and_property_columns() {
        let result = read_topojson_str(SAMPLE, Some("regions")).expect("decode");
        let frame = result.frame;
        assert_eq!(frame.row_count(), 2);
        assert_eq!(dtype(&frame, "geom"), DataType::Geometry);
        // Two property columns inferred: a string name and an integer pop.
        assert!(frame.column("name").is_some());
        assert_eq!(dtype(&frame, "pop"), DataType::Integer);
    }

    #[test]
    fn shared_arc_is_reused_with_reversal() {
        // The two polygons share arc 2 (forward in B, its reverse closes A via -2),
        // proving negative indices reverse and stitching drops shared endpoints.
        let result = read_topojson_str(SAMPLE, Some("regions")).unwrap();
        assert_eq!(result.frame.row_count(), 2);
    }

    #[test]
    fn applies_quantization_transform() {
        let quantized = r#"{
          "type": "Topology",
          "transform": {"scale": [2.0, 2.0], "translate": [100.0, 200.0]},
          "objects": {"pts": {"type": "Point", "coordinates": [5, 10]}},
          "arcs": []
        }"#;
        let result = read_topojson_str(quantized, None).unwrap();
        match result.frame.value("geom", 0) {
            Some(crate::DataValueRef::Geometry(Geometry::Point(p))) => {
                assert_eq!((p.x(), p.y()), (5.0 * 2.0 + 100.0, 10.0 * 2.0 + 200.0));
            }
            other => panic!("expected a transformed point, got {other:?}"),
        }
    }

    #[test]
    fn missing_object_name_is_reported() {
        let err = read_topojson_str(SAMPLE, Some("nope")).unwrap_err();
        assert!(matches!(err, DataError::Geo(msg) if msg.contains("not found")));
    }

    #[test]
    fn ambiguous_object_selection_is_reported() {
        let two = r#"{"type":"Topology","arcs":[],
          "objects":{"a":{"type":"Point","coordinates":[0,0]},
                     "b":{"type":"Point","coordinates":[1,1]}}}"#;
        let err = read_topojson_str(two, None).unwrap_err();
        assert!(matches!(err, DataError::Geo(msg) if msg.contains("multiple objects")));
    }

    #[test]
    fn non_topology_document_is_reported() {
        let err =
            read_topojson_str(r#"{"type":"FeatureCollection","features":[]}"#, None).unwrap_err();
        assert!(matches!(err, DataError::Geo(_)));
    }
}
