use std::collections::HashMap;

use algraf_core::Diagnostic;
use algraf_data::{DataFrame, DataType, DataValueRef, Table};
use algraf_semantics::{ChartIr, FrameIr, SpaceIr};

use crate::layout::Rect;
use crate::projection::{for_each_coord, Projection, SpatialScale};
use crate::scale::cell_f64;
use crate::space::ScaledSpace;

use super::derived::active_table;

/// Whether a column reference holds geometry (spec §10.11).
fn frame_is_geometry(frame: &FrameIr) -> bool {
    matches!(frame, FrameIr::Vector(col) if col.dtype == DataType::Geometry)
}

/// Whether a space renders as a spatial (projected map) space: it either frames
/// a geometry column or declares a `projection:` (spec §16.14, §16.15).
pub(super) fn is_spatial_space(space: &SpaceIr) -> bool {
    space.projection.is_some() || frame_is_geometry(&space.frame)
}

/// The `(longitude, latitude)` column names of a projected `long * lat` space,
/// for point/line overlays sharing a basemap's spatial scale.
fn lonlat_columns(frame: &FrameIr) -> Option<(String, String)> {
    if let FrameIr::Cartesian(axes) = frame {
        if let (Some(FrameIr::Vector(x)), Some(FrameIr::Vector(y))) = (axes.first(), axes.get(1)) {
            return Some((x.name.clone(), y.name.clone()));
        }
    }
    None
}

/// An accumulating projected bounding box.
#[derive(Default)]
struct Bbox {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
    seen: bool,
}

impl Bbox {
    fn add(&mut self, x: f64, y: f64) {
        if self.seen {
            self.min_x = self.min_x.min(x);
            self.min_y = self.min_y.min(y);
            self.max_x = self.max_x.max(x);
            self.max_y = self.max_y.max(y);
        } else {
            (self.min_x, self.min_y, self.max_x, self.max_y) = (x, y, x, y);
            self.seen = true;
        }
    }

    fn finish(self) -> Option<(f64, f64, f64, f64)> {
        self.seen
            .then_some((self.min_x, self.min_y, self.max_x, self.max_y))
    }
}

/// The shared projection and projected bounding box across all spatial spaces
/// (spec §16.15, §17.5).
pub(super) struct SpatialPlan {
    proj_name: Option<String>,
    bbox: (f64, f64, f64, f64),
}

impl SpatialPlan {
    /// Build a spatial scale for one space against the shared fit, tagging the
    /// longitude/latitude columns for a projected `long * lat` overlay.
    pub(super) fn scaled_space(&self, space: &SpaceIr, plot: Rect) -> Option<ScaledSpace> {
        let projection = Projection::resolve(self.proj_name.as_deref()).ok()?;
        let mut spatial = SpatialScale::fit(projection, self.bbox, plot);
        if let FrameIr::Vector(col) = &space.frame {
            if col.dtype == DataType::Geometry {
                spatial.geom_col = Some(col.name.clone());
            }
        }
        if let Some((lon, lat)) = lonlat_columns(&space.frame) {
            spatial.lon_col = Some(lon);
            spatial.lat_col = Some(lat);
        }
        Some(ScaledSpace::spatial(spatial))
    }
}

/// Resolve the shared spatial plan: one projection for all overlaid spatial
/// spaces (conflict is `E1803`, an invalid projection is `E1802`) and the union
/// of their projected bounding boxes so a basemap and overlay align.
pub(super) fn build_spatial_plan(
    ir: &ChartIr,
    primary: &dyn Table,
    derived: &HashMap<String, DataFrame>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<SpatialPlan> {
    let spatial: Vec<&SpaceIr> = ir.spaces.iter().filter(|s| is_spatial_space(s)).collect();
    let first = *spatial.first()?;

    // All spatial spaces must agree on projection; `None` means the default
    // equirectangular projection.
    let effective = |s: &SpaceIr| {
        s.projection
            .clone()
            .unwrap_or_else(|| "equirectangular".into())
    };
    let agreed = effective(first);
    if spatial.iter().any(|s| effective(s) != agreed) {
        diagnostics.push(Diagnostic::error(
            "E1803",
            "overlaid spaces declare conflicting projections; \
             all spatial spaces must use the same projection",
            first.span,
        ));
    }

    let proj_name = first.projection.clone();
    let projection = match Projection::resolve(proj_name.as_deref()) {
        Ok(projection) => projection,
        Err(message) => {
            diagnostics.push(Diagnostic::error(
                "E1802",
                format!("invalid or unknown projection: {message}"),
                first.span,
            ));
            return None;
        }
    };

    let mut bbox = Bbox::default();
    for space in &spatial {
        let table = active_table(&space.data, primary, derived);
        accumulate_space_bbox(space, table, &projection, &mut bbox);
    }
    Some(SpatialPlan {
        proj_name,
        bbox: bbox.finish()?,
    })
}

/// Project every coordinate a space contributes (geometry vertices, or
/// `long * lat` points) into the accumulating projected bounding box.
fn accumulate_space_bbox(
    space: &SpaceIr,
    table: &dyn Table,
    projection: &Projection,
    bbox: &mut Bbox,
) {
    if let FrameIr::Vector(col) = &space.frame {
        if col.dtype == DataType::Geometry {
            for row in 0..table.row_count() {
                if let Some(DataValueRef::Geometry(geometry)) = table.value(&col.name, row) {
                    for_each_coord(geometry, &mut |lon, lat| {
                        if let Some((x, y)) = projection.project(lon, lat) {
                            bbox.add(x, y);
                        }
                    });
                }
            }
            return;
        }
    }
    if let Some((lon_col, lat_col)) = lonlat_columns(&space.frame) {
        for row in 0..table.row_count() {
            if let (Some(lon), Some(lat)) = (
                cell_f64(table, &lon_col, row),
                cell_f64(table, &lat_col, row),
            ) {
                if let Some((x, y)) = projection.project(lon, lat) {
                    bbox.add(x, y);
                }
            }
        }
    }
}
