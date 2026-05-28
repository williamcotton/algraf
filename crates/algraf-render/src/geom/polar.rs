//! Shared SVG path emission for polar geometries (spec §16.16, §18).
//!
//! Polar area-filling geometries (`Bar`, `Rect`, `Tile`, `Ribbon`) draw wedges
//! and annular segments rather than rectangles; polar `Line`/`Area` draw closed
//! polygons. These helpers build the SVG `d` attributes (using the arc `A`
//! command) so geometry code stays free of trigonometry.

use std::f64::consts::PI;

use crate::space::Polar;
use crate::svg::num;

/// Build an SVG path `d` for an annular segment (a wedge when `r_in` is ~0):
/// the region between angles `[theta0, theta1]` and radii `[r_in, r_out]`
/// (spec §18). Angles are in radians in the polar frame's clockwise convention.
pub(super) fn annular_segment_path(
    polar: &Polar,
    theta0: f64,
    theta1: f64,
    r_in: f64,
    r_out: f64,
) -> String {
    let (a0, a1) = if theta0 <= theta1 {
        (theta0, theta1)
    } else {
        (theta1, theta0)
    };
    // The arc sweeps `[a0, a1]`; the large-arc flag is set past a half turn.
    let large = if (a1 - a0).abs() > PI { 1 } else { 0 };
    let (ox0, oy0) = polar.point(a0, r_out);
    let (ox1, oy1) = polar.point(a1, r_out);

    if r_in <= f64::EPSILON {
        // A solid wedge to the center.
        format!(
            "M {} {} A {} {} 0 {} 1 {} {} L {} {} Z",
            num(ox0),
            num(oy0),
            num(r_out),
            num(r_out),
            large,
            num(ox1),
            num(oy1),
            num(polar.cx),
            num(polar.cy),
        )
    } else {
        // An annular segment (donut wedge): outer arc forward, inner arc back.
        let (ix1, iy1) = polar.point(a1, r_in);
        let (ix0, iy0) = polar.point(a0, r_in);
        format!(
            "M {} {} A {} {} 0 {} 1 {} {} L {} {} A {} {} 0 {} 0 {} {} Z",
            num(ox0),
            num(oy0),
            num(r_out),
            num(r_out),
            large,
            num(ox1),
            num(oy1),
            num(ix1),
            num(iy1),
            num(r_in),
            num(r_in),
            large,
            num(ix0),
            num(iy0),
        )
    }
}
