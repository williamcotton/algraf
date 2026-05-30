use std::f64::consts::PI;

use algraf_semantics::{PolarDirectionIr, PolarThetaIr};

/// The default polar angular origin: the 12-o'clock position. `θ = -π/2` is the
/// top; increasing `θ` moves clockwise in screen coordinates (where +y points
/// down). A space MAY rotate this origin (`startAngle`) and reverse the sweep
/// (`direction`) — see [`polar_angular_range`] (spec §16.16).
pub(crate) const THETA_ORIGIN: f64 = -PI / 2.0;

/// Compute the `(start, end)` angular range a polar theta axis maps into, from a
/// `start_angle` (degrees, clockwise from 12 o'clock) and a sweep direction
/// (spec §16.16). The defaults (`0`, clockwise) yield `(-π/2, 3π/2)`,
/// reproducing the fixed behavior of earlier versions.
pub(crate) fn polar_angular_range(start_angle: f64, direction: PolarDirectionIr) -> (f64, f64) {
    let start = THETA_ORIGIN + start_angle.to_radians();
    let full = 2.0 * PI;
    match direction {
        PolarDirectionIr::Clockwise => (start, start + full),
        PolarDirectionIr::CounterClockwise => (start, start - full),
    }
}

/// Radial gap (px) between the outer radius and the baseline of a perimeter
/// category label (spec §19). The polar plot reserves this plus the widest
/// label so the labels stay within the plot rect; `render_polar_grid` places
/// labels at the same offset.
pub(crate) const POLAR_LABEL_GAP: f64 = 12.0;

/// A trained polar coordinate transform for a space (spec §16.16). The `theta`
/// axis maps its domain to `[THETA_START, THETA_END]` and the radius axis maps
/// its domain to `[r_inner, r_outer]`; final pixel positions come from
/// [`Polar::point`].
#[derive(Debug, Clone, Copy)]
pub struct Polar {
    pub cx: f64,
    pub cy: f64,
    pub r_inner: f64,
    pub r_outer: f64,
    pub theta: PolarThetaIr,
    /// The angle (radians) the theta-domain minimum maps to.
    pub theta_start: f64,
    /// The angle (radians) the theta-domain maximum maps to. May be less than
    /// `theta_start` for a counterclockwise sweep.
    pub theta_end: f64,
}

impl Polar {
    /// Convert a `(θ, r)` polar coordinate to a Cartesian pixel position:
    /// `x = cx + r·cos(θ)`, `y = cy + r·sin(θ)` (spec §16.16).
    pub fn point(&self, theta: f64, r: f64) -> (f64, f64) {
        (self.cx + r * theta.cos(), self.cy + r * theta.sin())
    }

    /// Clamp a radius to the drawable annulus `[r_inner, r_outer]`.
    pub fn clamp_radius(&self, r: f64) -> f64 {
        r.clamp(self.r_inner, self.r_outer)
    }
}
