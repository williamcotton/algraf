//! Statistical transforms for derived tables (spec §15).
//!
//! Public stat functions preserve the pre-v0.35 API while the implementation is
//! split by stat family. Each public stat exits through the deterministic-frame
//! helper in `util`, making stable output ordering an explicit module-boundary
//! contract (spec §18.12).

mod bin;
mod density;
mod primitive;
mod smooth;
mod summary;
pub(crate) mod util;

pub use bin::{
    bin2d, bin_blended, bin_grouped, bin_with_options, hexbin, hexbin_frame, Bin2DOptions,
    BinClosed, BinInterval, BinOptions,
};
pub use density::{density, density_blended, density_values, DensityOptions, DensityPoint};
pub use primitive::{
    curve_sample, interval_middles, interval_rects, interval_segments, step_vertices,
    vector_endpoints, CurveSampleOptions, IntervalOrientation, IntervalSegmentsOptions,
    IntervalWidthOptions, StepDirection, StepVerticesOptions, VectorEndpointsOptions,
};
pub use smooth::{smooth, smooth_points, SmoothMethod, SmoothOptions};
pub use summary::count_by;
