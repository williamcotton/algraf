//! Axis, grid, and legend guides (spec §19).
//!
//! Split along the render execution boundary (spec §24.6):
//!
//! - [`plan`] measures tick labels and reserves axis margins from trained
//!   scales. It makes layout decisions and writes no output.
//! - [`emit`] takes those decisions and the trained scales and writes the grid,
//!   axes, facet strips, and legends to SVG.
//!
//! Planning runs before final layout (so the left margin can grow to fit y tick
//! labels); emission runs during document assembly.

mod emit;
mod plan;

pub(crate) use emit::{
    render_axes, render_facet_label, render_grid, render_legends, AxisRenderOptions,
};
pub(crate) use plan::{max_y_tick_label_width, y_axis_left_margin};
