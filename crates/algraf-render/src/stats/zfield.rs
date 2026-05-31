use std::collections::BTreeMap;

use algraf_data::geo_types::{Coord, Geometry, LineString, Polygon};
use algraf_data::{Column, DataFrame, DataType, Table};

use crate::scale::{cell_f64, numeric_domain};

use super::bin::{bin_index, bin_layout, hex_lattice_index};
use super::density::percentile;
use super::summary::SummaryReducer;
use super::util::{col_def, deterministic_frame};
use super::{BinClosed, BinOptions};

#[derive(Debug, Clone, Copy)]
pub struct GridSize {
    pub x: usize,
    pub y: usize,
}

impl GridSize {
    pub fn square(n: usize) -> GridSize {
        GridSize { x: n, y: n }
    }
}

impl Default for GridSize {
    fn default() -> Self {
        GridSize::square(30)
    }
}

#[derive(Debug, Clone)]
pub enum LevelSpec {
    Count(Option<usize>),
    Values(Vec<f64>),
}

impl Default for LevelSpec {
    fn default() -> Self {
        LevelSpec::Count(None)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContourOptions {
    pub levels: LevelSpec,
}

#[derive(Debug, Clone, Copy)]
pub struct Density2DOptions {
    pub bandwidth: Option<f64>,
    pub grid: GridSize,
}

impl Default for Density2DOptions {
    fn default() -> Self {
        Density2DOptions {
            bandwidth: None,
            grid: GridSize::square(64),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Summary2DOptions {
    pub bins: GridSize,
    pub reducer: SummaryReducer,
}

impl Default for Summary2DOptions {
    fn default() -> Self {
        Summary2DOptions {
            bins: GridSize::default(),
            reducer: SummaryReducer::Mean,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OrderedF64(f64);

impl Eq for OrderedF64 {}

impl PartialOrd for OrderedF64 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedF64 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

#[derive(Debug, Clone, Copy)]
struct GridPoint {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Debug, Clone)]
struct Grid {
    xs: Vec<f64>,
    ys: Vec<f64>,
    z: Vec<Option<f64>>,
}

impl Grid {
    fn new(xs: Vec<f64>, ys: Vec<f64>, z: Vec<Option<f64>>) -> Grid {
        Grid { xs, ys, z }
    }

    fn get(&self, x: usize, y: usize) -> Option<f64> {
        self.z.get(y * self.xs.len() + x).copied().flatten()
    }

    fn point(&self, x: usize, y: usize) -> Option<GridPoint> {
        Some(GridPoint {
            x: *self.xs.get(x)?,
            y: *self.ys.get(y)?,
            z: self.get(x, y)?,
        })
    }

    fn z_domain(&self) -> Option<(f64, f64)> {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for z in self.z.iter().flatten().copied() {
            min = min.min(z);
            max = max.max(z);
        }
        (min <= max).then_some((min, max))
    }
}

pub fn contour_lines(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    z_col: &str,
    options: ContourOptions,
) -> DataFrame {
    let grid = regular_grid(table, x_col, y_col, z_col);
    contour_lines_from_grid(&grid, options)
}

pub fn contour_bands(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    z_col: &str,
    options: ContourOptions,
) -> DataFrame {
    let grid = regular_grid(table, x_col, y_col, z_col);
    contour_bands_from_grid(&grid, options)
}

pub fn density2d(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    options: Density2DOptions,
) -> DataFrame {
    let grid = density2d_grid(table, x_col, y_col, options);
    density_grid_frame(&grid)
}

pub fn density2d_contours(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    density_options: Density2DOptions,
    contour_options: ContourOptions,
) -> DataFrame {
    let grid = density2d_grid(table, x_col, y_col, density_options);
    contour_lines_from_grid(&grid, contour_options)
}

pub fn density2d_bands(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    density_options: Density2DOptions,
    contour_options: ContourOptions,
) -> DataFrame {
    let grid = density2d_grid(table, x_col, y_col, density_options);
    contour_bands_from_grid(&grid, contour_options)
}

pub fn summary2d(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    z_col: &str,
    options: Summary2DOptions,
) -> DataFrame {
    let bins = normalize_grid_size(options.bins, 30, 1, 512);
    let (x_min, x_max) = numeric_domain(table, x_col).unwrap_or((0.0, 1.0));
    let (y_min, y_max) = numeric_domain(table, y_col).unwrap_or((0.0, 1.0));
    let (x_start, x_width, x_bins) = bin_layout(x_min, x_max, bins.x, unit_bin_options(bins.x));
    let (y_start, y_width, y_bins) = bin_layout(y_min, y_max, bins.y, unit_bin_options(bins.y));
    let mut cells = vec![Vec::new(); x_bins * y_bins];
    let mut total = 0i64;
    for row in 0..table.row_count() {
        let (Some(x), Some(y), Some(z)) = (
            cell_f64(table, x_col, row),
            cell_f64(table, y_col, row),
            cell_f64(table, z_col, row),
        ) else {
            continue;
        };
        let xi = bin_index(x, x_start, x_width, x_bins, BinClosed::Left);
        let yi = bin_index(y, y_start, y_width, y_bins, BinClosed::Left);
        cells[yi * x_bins + xi].push(z);
        total += 1;
    }

    let area = (x_width * y_width).abs();
    let denom = total as f64 * area;
    let mut xs0 = Vec::new();
    let mut xs1 = Vec::new();
    let mut xcs = Vec::new();
    let mut ys0 = Vec::new();
    let mut ys1 = Vec::new();
    let mut ycs = Vec::new();
    let mut counts = Vec::new();
    let mut densities = Vec::new();
    let mut values = Vec::new();
    for yi in 0..y_bins {
        for xi in 0..x_bins {
            let cell = &mut cells[yi * x_bins + xi];
            if cell.is_empty() {
                continue;
            }
            let x0 = x_start + xi as f64 * x_width;
            let x1 = x0 + x_width;
            let y0 = y_start + yi as f64 * y_width;
            let y1 = y0 + y_width;
            let count = cell.len() as i64;
            xs0.push(Some(x0));
            xs1.push(Some(x1));
            xcs.push(Some((x0 + x1) / 2.0));
            ys0.push(Some(y0));
            ys1.push(Some(y1));
            ycs.push(Some((y0 + y1) / 2.0));
            counts.push(Some(count));
            densities.push(Some(if denom > f64::EPSILON {
                count as f64 / denom
            } else {
                0.0
            }));
            values.push(Some(reduce(cell, options.reducer)));
        }
    }

    deterministic_frame(
        summary2d_schema(),
        vec![
            Column::Float(xs0),
            Column::Float(xs1),
            Column::Float(xcs),
            Column::Float(ys0),
            Column::Float(ys1),
            Column::Float(ycs),
            Column::Int(counts),
            Column::Float(densities),
            Column::Float(values),
        ],
    )
}

pub fn summaryhex(
    table: &dyn Table,
    x_col: &str,
    y_col: &str,
    z_col: &str,
    options: Summary2DOptions,
) -> DataFrame {
    let bins = options.bins.x.max(options.bins.y).max(1);
    let bins = bins.clamp(1, 512);
    let (x_min, x_max) = numeric_domain(table, x_col).unwrap_or((0.0, 1.0));
    let (y_min, y_max) = numeric_domain(table, y_col).unwrap_or((0.0, 1.0));
    let x_span = nonzero_span(x_min, x_max);
    let y_span = nonzero_span(y_min, y_max);
    let r = 1.0 / (bins as f64 * 3.0_f64.sqrt());
    let dx = r * 3.0_f64.sqrt();
    let dy = r * 1.5;
    let mut cells: BTreeMap<(i64, i64), Vec<f64>> = BTreeMap::new();
    let mut total = 0i64;
    for row in 0..table.row_count() {
        let (Some(x), Some(y), Some(z)) = (
            cell_f64(table, x_col, row),
            cell_f64(table, y_col, row),
            cell_f64(table, z_col, row),
        ) else {
            continue;
        };
        let u = (x - x_min) / x_span;
        let v = (y - y_min) / y_span;
        let (pi, pj) = hex_lattice_index(u, v, dx, dy);
        cells
            .entry(((pi * 2.0).round() as i64, pj as i64))
            .or_default()
            .push(z);
        total += 1;
    }

    let rx = r * x_span.abs();
    let ry = r * y_span.abs();
    let area = 3.0 * 3.0_f64.sqrt() * rx * ry / 2.0;
    let denom = total as f64 * area;
    let mut geoms = Vec::new();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut radii = Vec::new();
    let mut y_radii = Vec::new();
    let mut counts = Vec::new();
    let mut densities = Vec::new();
    let mut values = Vec::new();
    for ((pi2, pj), mut zs) in cells {
        let pi = pi2 as f64 / 2.0;
        let pj = pj as f64;
        let x = x_min + pi * dx * x_span;
        let y = y_min + pj * dy * y_span;
        let count = zs.len() as i64;
        geoms.push(Some(Geometry::Polygon(hex_polygon(x, y, rx, ry))));
        xs.push(Some(x));
        ys.push(Some(y));
        radii.push(Some(rx));
        y_radii.push(Some(ry));
        counts.push(Some(count));
        densities.push(Some(if denom > f64::EPSILON {
            count as f64 / denom
        } else {
            0.0
        }));
        values.push(Some(reduce(&mut zs, options.reducer)));
    }

    deterministic_frame(
        summaryhex_schema(),
        vec![
            Column::Geometry(geoms),
            Column::Float(xs),
            Column::Float(ys),
            Column::Float(radii),
            Column::Float(y_radii),
            Column::Int(counts),
            Column::Float(densities),
            Column::Float(values),
        ],
    )
}

fn contour_lines_from_grid(grid: &Grid, options: ContourOptions) -> DataFrame {
    let schema = contour_lines_schema();
    if grid.xs.len() < 2 || grid.ys.len() < 2 {
        return deterministic_frame(schema, empty_contour_line_columns());
    }
    let Some((min, max)) = grid.z_domain() else {
        return deterministic_frame(schema, empty_contour_line_columns());
    };
    let levels = line_levels(&options.levels, min, max);
    if levels.is_empty() {
        return deterministic_frame(schema, empty_contour_line_columns());
    }

    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut level_values = Vec::new();
    let mut level_indices = Vec::new();
    let mut contour_ids = Vec::new();
    let mut contour_id = 0i64;
    for (li, level) in levels.iter().copied().enumerate() {
        for yi in 0..grid.ys.len() - 1 {
            for xi in 0..grid.xs.len() - 1 {
                let Some(p00) = grid.point(xi, yi) else {
                    continue;
                };
                let Some(p10) = grid.point(xi + 1, yi) else {
                    continue;
                };
                let Some(p11) = grid.point(xi + 1, yi + 1) else {
                    continue;
                };
                let Some(p01) = grid.point(xi, yi + 1) else {
                    continue;
                };
                for tri in [[p00, p10, p11], [p00, p11, p01]] {
                    if let Some((a, b)) = contour_segment(tri, level) {
                        xs.push(Some(a.x));
                        ys.push(Some(a.y));
                        level_values.push(Some(level));
                        level_indices.push(Some(li as i64));
                        contour_ids.push(Some(contour_id));
                        xs.push(Some(b.x));
                        ys.push(Some(b.y));
                        level_values.push(Some(level));
                        level_indices.push(Some(li as i64));
                        contour_ids.push(Some(contour_id));
                        contour_id += 1;
                    }
                }
            }
        }
    }
    deterministic_frame(
        schema,
        vec![
            Column::Float(xs),
            Column::Float(ys),
            Column::Float(level_values),
            Column::Int(level_indices),
            Column::Int(contour_ids),
        ],
    )
}

fn contour_bands_from_grid(grid: &Grid, options: ContourOptions) -> DataFrame {
    let schema = contour_bands_schema();
    if grid.xs.len() < 2 || grid.ys.len() < 2 {
        return deterministic_frame(schema, empty_contour_band_columns());
    }
    let Some((min, max)) = grid.z_domain() else {
        return deterministic_frame(schema, empty_contour_band_columns());
    };
    let breaks = band_breaks(&options.levels, min, max);
    if breaks.len() < 2 {
        return deterministic_frame(schema, empty_contour_band_columns());
    }

    let mut geoms = Vec::new();
    let mut lows = Vec::new();
    let mut highs = Vec::new();
    let mut mids = Vec::new();
    let mut band_indices = Vec::new();
    for (bi, pair) in breaks.windows(2).enumerate() {
        let lo = pair[0];
        let hi = pair[1];
        if hi <= lo {
            continue;
        }
        for yi in 0..grid.ys.len() - 1 {
            for xi in 0..grid.xs.len() - 1 {
                let Some(p00) = grid.point(xi, yi) else {
                    continue;
                };
                let Some(p10) = grid.point(xi + 1, yi) else {
                    continue;
                };
                let Some(p11) = grid.point(xi + 1, yi + 1) else {
                    continue;
                };
                let Some(p01) = grid.point(xi, yi + 1) else {
                    continue;
                };
                for tri in [[p00, p10, p11], [p00, p11, p01]] {
                    let clipped = clip_band(&tri, lo, hi);
                    if clipped.len() < 3 || polygon_area(&clipped) <= f64::EPSILON {
                        continue;
                    }
                    geoms.push(Some(Geometry::Polygon(points_polygon(&clipped))));
                    lows.push(Some(lo));
                    highs.push(Some(hi));
                    mids.push(Some((lo + hi) / 2.0));
                    band_indices.push(Some(bi as i64));
                }
            }
        }
    }

    deterministic_frame(
        schema,
        vec![
            Column::Geometry(geoms),
            Column::Float(lows),
            Column::Float(highs),
            Column::Float(mids),
            Column::Int(band_indices),
        ],
    )
}

fn regular_grid(table: &dyn Table, x_col: &str, y_col: &str, z_col: &str) -> Grid {
    let mut accum: BTreeMap<(OrderedF64, OrderedF64), (f64, usize)> = BTreeMap::new();
    for row in 0..table.row_count() {
        let (Some(x), Some(y), Some(z)) = (
            cell_f64(table, x_col, row),
            cell_f64(table, y_col, row),
            cell_f64(table, z_col, row),
        ) else {
            continue;
        };
        let entry = accum
            .entry((OrderedF64(x), OrderedF64(y)))
            .or_insert((0.0, 0));
        entry.0 += z;
        entry.1 += 1;
    }
    let mut xs: Vec<f64> = accum.keys().map(|(x, _)| x.0).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| a.total_cmp(b).is_eq());
    let mut ys: Vec<f64> = accum.keys().map(|(_, y)| y.0).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| a.total_cmp(b).is_eq());
    let x_index: BTreeMap<OrderedF64, usize> = xs
        .iter()
        .copied()
        .enumerate()
        .map(|(i, x)| (OrderedF64(x), i))
        .collect();
    let y_index: BTreeMap<OrderedF64, usize> = ys
        .iter()
        .copied()
        .enumerate()
        .map(|(i, y)| (OrderedF64(y), i))
        .collect();
    let mut z = vec![None; xs.len() * ys.len()];
    for ((x, y), (sum, count)) in accum {
        if count == 0 {
            continue;
        }
        let Some(&xi) = x_index.get(&x) else {
            continue;
        };
        let Some(&yi) = y_index.get(&y) else {
            continue;
        };
        z[yi * xs.len() + xi] = Some(sum / count as f64);
    }
    Grid::new(xs, ys, z)
}

fn density2d_grid(table: &dyn Table, x_col: &str, y_col: &str, options: Density2DOptions) -> Grid {
    let mut values: Vec<(f64, f64)> = (0..table.row_count())
        .filter_map(|row| Some((cell_f64(table, x_col, row)?, cell_f64(table, y_col, row)?)))
        .collect();
    values.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.total_cmp(&b.1)));
    if values.len() < 2 {
        return Grid::new(Vec::new(), Vec::new(), Vec::new());
    }
    let n = values.len() as f64;
    let xs_values: Vec<f64> = values.iter().map(|(x, _)| *x).collect();
    let ys_values: Vec<f64> = values.iter().map(|(_, y)| *y).collect();
    let hx = options
        .bandwidth
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or_else(|| silverman_axis(&xs_values));
    let hy = options
        .bandwidth
        .filter(|h| h.is_finite() && *h > 0.0)
        .unwrap_or_else(|| silverman_axis(&ys_values));
    let hx = if hx > f64::EPSILON { hx } else { 1.0 };
    let hy = if hy > f64::EPSILON { hy } else { 1.0 };
    let grid = normalize_grid_size(options.grid, 64, 2, 256);
    let x_min = xs_values.first().copied().unwrap_or(0.0) - 3.0 * hx;
    let x_max = xs_values.last().copied().unwrap_or(1.0) + 3.0 * hx;
    let y_min = ys_values
        .iter()
        .copied()
        .min_by(f64::total_cmp)
        .unwrap_or(0.0)
        - 3.0 * hy;
    let y_max = ys_values
        .iter()
        .copied()
        .max_by(f64::total_cmp)
        .unwrap_or(1.0)
        + 3.0 * hy;
    let xs = linspace(x_min, x_max, grid.x);
    let ys = linspace(y_min, y_max, grid.y);
    let inv = 1.0 / (n * hx * hy);
    let mut z = Vec::with_capacity(xs.len() * ys.len());
    for y in &ys {
        for x in &xs {
            let sum: f64 = values
                .iter()
                .map(|(vx, vy)| gaussian_kernel((x - vx) / hx) * gaussian_kernel((y - vy) / hy))
                .sum();
            z.push(Some(sum * inv));
        }
    }
    Grid::new(xs, ys, z)
}

fn density_grid_frame(grid: &Grid) -> DataFrame {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    let mut ds = Vec::new();
    for yi in 0..grid.ys.len() {
        for xi in 0..grid.xs.len() {
            let Some(density) = grid.get(xi, yi) else {
                continue;
            };
            xs.push(Some(grid.xs[xi]));
            ys.push(Some(grid.ys[yi]));
            ds.push(Some(density));
        }
    }
    deterministic_frame(
        density2d_schema(),
        vec![Column::Float(xs), Column::Float(ys), Column::Float(ds)],
    )
}

fn line_levels(spec: &LevelSpec, min: f64, max: f64) -> Vec<f64> {
    if max <= min {
        return Vec::new();
    }
    match spec {
        LevelSpec::Values(values) => values
            .iter()
            .copied()
            .filter(|level| level.is_finite() && *level >= min && *level <= max)
            .collect(),
        LevelSpec::Count(count) => {
            let count = count.unwrap_or(10).max(1);
            let step = (max - min) / (count + 1) as f64;
            (1..=count).map(|i| min + step * i as f64).collect()
        }
    }
}

fn band_breaks(spec: &LevelSpec, min: f64, max: f64) -> Vec<f64> {
    if max <= min {
        return Vec::new();
    }
    match spec {
        LevelSpec::Values(values) => values
            .iter()
            .copied()
            .filter(|level| level.is_finite())
            .collect(),
        LevelSpec::Count(count) => {
            let bands = count.unwrap_or(10).max(1);
            let step = (max - min) / bands as f64;
            (0..=bands).map(|i| min + step * i as f64).collect()
        }
    }
}

fn contour_segment(triangle: [GridPoint; 3], level: f64) -> Option<(GridPoint, GridPoint)> {
    let mut points = Vec::new();
    for (a, b) in [
        (triangle[0], triangle[1]),
        (triangle[1], triangle[2]),
        (triangle[2], triangle[0]),
    ] {
        if let Some(point) = interpolate_level(a, b, level) {
            if !points.iter().any(|p: &GridPoint| same_xy(*p, point)) {
                points.push(point);
            }
        }
    }
    if points.len() == 2 && !same_xy(points[0], points[1]) {
        Some((points[0], points[1]))
    } else {
        None
    }
}

fn interpolate_level(a: GridPoint, b: GridPoint, level: f64) -> Option<GridPoint> {
    let da = a.z - level;
    let db = b.z - level;
    if da.abs() <= f64::EPSILON && db.abs() <= f64::EPSILON {
        return None;
    }
    if da.abs() <= f64::EPSILON {
        return Some(a);
    }
    if db.abs() <= f64::EPSILON {
        return Some(b);
    }
    if (da < 0.0 && db > 0.0) || (da > 0.0 && db < 0.0) {
        let t = (level - a.z) / (b.z - a.z);
        return Some(GridPoint {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
            z: level,
        });
    }
    None
}

fn clip_band(triangle: &[GridPoint; 3], lo: f64, hi: f64) -> Vec<GridPoint> {
    let above = clip_polygon(triangle.as_slice(), lo, true);
    clip_polygon(&above, hi, false)
}

fn clip_polygon(points: &[GridPoint], threshold: f64, keep_above: bool) -> Vec<GridPoint> {
    if points.is_empty() {
        return Vec::new();
    }
    let inside = |p: GridPoint| {
        if keep_above {
            p.z >= threshold
        } else {
            p.z <= threshold
        }
    };
    let mut out = Vec::new();
    let mut prev = *points.last().unwrap();
    let mut prev_inside = inside(prev);
    for &cur in points {
        let cur_inside = inside(cur);
        if cur_inside {
            if !prev_inside {
                out.push(interpolate_threshold(prev, cur, threshold));
            }
            out.push(cur);
        } else if prev_inside {
            out.push(interpolate_threshold(prev, cur, threshold));
        }
        prev = cur;
        prev_inside = cur_inside;
    }
    dedup_ring(out)
}

fn interpolate_threshold(a: GridPoint, b: GridPoint, z: f64) -> GridPoint {
    if (b.z - a.z).abs() <= f64::EPSILON {
        return a;
    }
    let t = (z - a.z) / (b.z - a.z);
    GridPoint {
        x: a.x + (b.x - a.x) * t,
        y: a.y + (b.y - a.y) * t,
        z,
    }
}

fn dedup_ring(points: Vec<GridPoint>) -> Vec<GridPoint> {
    let mut out = Vec::new();
    for point in points {
        if !out.last().is_some_and(|prev| same_xy(*prev, point)) {
            out.push(point);
        }
    }
    if out.len() > 1 && same_xy(out[0], *out.last().unwrap()) {
        out.pop();
    }
    out
}

fn same_xy(a: GridPoint, b: GridPoint) -> bool {
    (a.x - b.x).abs() <= 1e-12 && (a.y - b.y).abs() <= 1e-12
}

fn polygon_area(points: &[GridPoint]) -> f64 {
    if points.len() < 3 {
        return 0.0;
    }
    let mut sum = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        sum += a.x * b.y - b.x * a.y;
    }
    sum.abs() / 2.0
}

fn points_polygon(points: &[GridPoint]) -> Polygon<f64> {
    let mut coords: Vec<Coord<f64>> = points.iter().map(|p| Coord { x: p.x, y: p.y }).collect();
    if let Some(first) = coords.first().copied() {
        coords.push(first);
    }
    Polygon::new(LineString::from(coords), Vec::new())
}

fn hex_polygon(x: f64, y: f64, rx: f64, ry: f64) -> Polygon<f64> {
    let mut coords = Vec::new();
    for i in 0..6 {
        let angle = std::f64::consts::TAU * (i as f64) / 6.0 + std::f64::consts::FRAC_PI_2;
        coords.push(Coord {
            x: x + rx * angle.cos(),
            y: y + ry * angle.sin(),
        });
    }
    coords.push(coords[0]);
    Polygon::new(LineString::from(coords), Vec::new())
}

fn reduce(values: &mut [f64], reducer: SummaryReducer) -> f64 {
    match reducer {
        SummaryReducer::Count => values.len() as f64,
        SummaryReducer::Mean | SummaryReducer::MeanSe => {
            values.iter().sum::<f64>() / values.len() as f64
        }
        SummaryReducer::Min => values.iter().copied().fold(f64::INFINITY, f64::min),
        SummaryReducer::Max => values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        SummaryReducer::Sum => values.iter().sum(),
        SummaryReducer::Median => {
            values.sort_by(f64::total_cmp);
            let mid = values.len() / 2;
            if values.len() % 2 == 1 {
                values[mid]
            } else {
                (values[mid - 1] + values[mid]) / 2.0
            }
        }
    }
}

fn normalize_grid_size(size: GridSize, _default: usize, min: usize, max: usize) -> GridSize {
    GridSize {
        x: size.x.clamp(min, max),
        y: size.y.clamp(min, max),
    }
}

fn unit_bin_options(bins: usize) -> BinOptions {
    BinOptions {
        bins,
        bin_width: None,
        boundary: None,
        closed: BinClosed::Left,
        interval: None,
    }
}

fn nonzero_span(min: f64, max: f64) -> f64 {
    if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    }
}

fn linspace(min: f64, max: f64, n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![min];
    }
    let step = (max - min) / (n - 1) as f64;
    (0..n).map(|i| min + step * i as f64).collect()
}

fn silverman_axis(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    let std_dev = variance.sqrt();
    let iqr = percentile(values, 0.75) - percentile(values, 0.25);
    let spread = if iqr > 0.0 {
        std_dev.min(iqr / 1.349)
    } else {
        std_dev
    };
    0.9 * spread * n.powf(-0.2)
}

fn gaussian_kernel(u: f64) -> f64 {
    const INV_SQRT_2PI: f64 = 0.398_942_280_401_432_7;
    INV_SQRT_2PI * (-0.5 * u * u).exp()
}

fn summary2d_schema() -> Vec<algraf_data::ColumnDef> {
    let mut schema = vec![
        col_def("x_start", DataType::Float),
        col_def("x_end", DataType::Float),
        col_def("x_center", DataType::Float),
        col_def("y_start", DataType::Float),
        col_def("y_end", DataType::Float),
        col_def("y_center", DataType::Float),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
    ];
    schema.push(col_def("value", DataType::Float));
    schema
}

fn summaryhex_schema() -> Vec<algraf_data::ColumnDef> {
    vec![
        col_def("geom", DataType::Geometry),
        col_def("x", DataType::Float),
        col_def("y", DataType::Float),
        col_def("radius", DataType::Float),
        col_def("y_radius", DataType::Float),
        col_def("count", DataType::Integer),
        col_def("density", DataType::Float),
        col_def("value", DataType::Float),
    ]
}

fn contour_lines_schema() -> Vec<algraf_data::ColumnDef> {
    vec![
        col_def("x", DataType::Float),
        col_def("y", DataType::Float),
        col_def("level", DataType::Float),
        col_def("level_index", DataType::Integer),
        col_def("contour_id", DataType::Integer),
    ]
}

fn contour_bands_schema() -> Vec<algraf_data::ColumnDef> {
    vec![
        col_def("geom", DataType::Geometry),
        col_def("level_low", DataType::Float),
        col_def("level_high", DataType::Float),
        col_def("level_mid", DataType::Float),
        col_def("band_index", DataType::Integer),
    ]
}

fn density2d_schema() -> Vec<algraf_data::ColumnDef> {
    vec![
        col_def("x", DataType::Float),
        col_def("y", DataType::Float),
        col_def("density", DataType::Float),
    ]
}

fn empty_contour_line_columns() -> Vec<Column> {
    vec![
        Column::Float(Vec::new()),
        Column::Float(Vec::new()),
        Column::Float(Vec::new()),
        Column::Int(Vec::new()),
        Column::Int(Vec::new()),
    ]
}

fn empty_contour_band_columns() -> Vec<Column> {
    vec![
        Column::Geometry(Vec::new()),
        Column::Float(Vec::new()),
        Column::Float(Vec::new()),
        Column::Float(Vec::new()),
        Column::Int(Vec::new()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(rows: &[(f64, f64, f64)]) -> DataFrame {
        DataFrame::new(
            vec![
                col_def("x", DataType::Float),
                col_def("y", DataType::Float),
                col_def("z", DataType::Float),
            ],
            vec![
                Column::Float(rows.iter().map(|r| Some(r.0)).collect()),
                Column::Float(rows.iter().map(|r| Some(r.1)).collect()),
                Column::Float(rows.iter().map(|r| Some(r.2)).collect()),
            ],
        )
    }

    #[test]
    fn contour_lines_emit_stable_segments() {
        let df = frame(&[
            (0.0, 0.0, 0.0),
            (1.0, 0.0, 1.0),
            (0.0, 1.0, 1.0),
            (1.0, 1.0, 2.0),
        ]);
        let out = contour_lines(
            &df,
            "x",
            "y",
            "z",
            ContourOptions {
                levels: LevelSpec::Values(vec![1.0]),
            },
        );
        assert_eq!(out.row_count(), 4);
    }

    #[test]
    fn summary2d_median_is_deterministic() {
        let df = frame(&[
            (0.0, 0.0, 4.0),
            (0.1, 0.1, 2.0),
            (0.2, 0.2, 8.0),
            (1.0, 1.0, 10.0),
        ]);
        let out = summary2d(
            &df,
            "x",
            "y",
            "z",
            Summary2DOptions {
                bins: GridSize::square(1),
                reducer: SummaryReducer::Median,
            },
        );
        assert_eq!(out.row_count(), 1);
        assert_eq!(cell_f64(&out, "value", 0), Some(6.0));
    }
}
