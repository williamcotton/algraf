use algraf_data::{Column, ColumnView, DataFrame, DataType, Table};

use crate::scale::cell_f64;

use super::util::{col_def, deterministic_frame};

/// The smoothing method for [`smooth_points`] (spec §15.x).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmoothMethod {
    /// Ordinary least-squares linear fit.
    Lm,
    /// Locally weighted (tricube) degree-1 regression.
    Loess,
}

/// Options for [`smooth_points`].
#[derive(Debug, Clone, Copy)]
pub struct SmoothOptions {
    pub method: SmoothMethod,
    /// Loess neighborhood fraction in `(0, 1]`.
    pub span: f64,
    /// Emit confidence-band half-widths in [`SmoothPoint::se`].
    pub se: bool,
    /// Number of evaluation points across the x-range when a curve is sampled
    /// (loess, or `lm` with `se`). A plain `lm` line uses two endpoints.
    pub eval_points: usize,
    /// Multiplier applied to the standard error to form the band half-width
    /// (1.96 ≈ 95% under a normal approximation, spec §15.x).
    pub z: f64,
}

impl Default for SmoothOptions {
    fn default() -> Self {
        SmoothOptions {
            method: SmoothMethod::Lm,
            span: 0.75,
            se: false,
            eval_points: 80,
            z: 1.96,
        }
    }
}

/// One sampled point on a fitted smooth, with the standard error of the fit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothPoint {
    pub x: f64,
    pub y: f64,
    /// Standard error of the fitted `y` (0 when `se` is disabled).
    pub se: f64,
}

/// Materialize a `Smooth` derived table over two numeric columns (spec §15.x).
/// Columns are always `x`, `y`; when `options.se` is set, `ymin`, `ymax`, and
/// `se` confidence-band columns follow.
pub fn smooth(table: &dyn Table, x_col: &str, y_col: &str, options: SmoothOptions) -> DataFrame {
    let x_view = table.column(x_col);
    let y_view = table.column(y_col);
    let mut points: Vec<(f64, f64)> = (0..table.row_count())
        .filter_map(|row| {
            Some((
                f64_cell(x_view, table, x_col, row)?,
                f64_cell(y_view, table, y_col, row)?,
            ))
        })
        .filter(|(x, y)| x.is_finite() && y.is_finite())
        .collect();
    let fitted = smooth_points(&mut points, options);

    if options.se {
        let schema = vec![
            col_def("x", DataType::Float),
            col_def("y", DataType::Float),
            col_def("ymin", DataType::Float),
            col_def("ymax", DataType::Float),
            col_def("se", DataType::Float),
        ];
        return deterministic_frame(
            schema,
            vec![
                Column::from_float_options(fitted.iter().map(|p| Some(p.x)).collect()),
                Column::from_float_options(fitted.iter().map(|p| Some(p.y)).collect()),
                Column::from_float_options(
                    fitted
                        .iter()
                        .map(|p| Some(p.y - options.z * p.se))
                        .collect(),
                ),
                Column::from_float_options(
                    fitted
                        .iter()
                        .map(|p| Some(p.y + options.z * p.se))
                        .collect(),
                ),
                Column::from_float_options(fitted.iter().map(|p| Some(p.se)).collect()),
            ],
        );
    }

    let schema = vec![col_def("x", DataType::Float), col_def("y", DataType::Float)];
    deterministic_frame(
        schema,
        vec![
            Column::from_float_options(fitted.iter().map(|p| Some(p.x)).collect()),
            Column::from_float_options(fitted.iter().map(|p| Some(p.y)).collect()),
        ],
    )
}

fn f64_cell(
    view: Option<ColumnView<'_>>,
    table: &dyn Table,
    column: &str,
    row: usize,
) -> Option<f64> {
    match view {
        Some(view) => view.f64_at(row),
        None => cell_f64(table, column, row),
    }
}

/// Fit a smooth over `(x, y)` points and sample it deterministically.
///
/// `points` is sorted in place by x. Returns an empty vector when there are too
/// few distinct x values to fit a line. For `lm` without `se`, the result is the
/// two line endpoints; otherwise the curve is sampled at `eval_points` evenly
/// spaced x values across the observed range.
pub fn smooth_points(points: &mut [(f64, f64)], options: SmoothOptions) -> Vec<SmoothPoint> {
    points.sort_by(|a, b| a.0.total_cmp(&b.0));
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let x_min = points[0].0;
    let x_max = points[n - 1].0;
    if (x_max - x_min).abs() <= f64::EPSILON {
        return Vec::new();
    }

    match options.method {
        SmoothMethod::Lm => lm_points(points, options, x_min, x_max),
        SmoothMethod::Loess => loess_points(points, options, x_min, x_max),
    }
}

fn eval_grid(x_min: f64, x_max: f64, count: usize) -> Vec<f64> {
    let count = count.max(2);
    let step = (x_max - x_min) / (count - 1) as f64;
    (0..count).map(|i| x_min + step * i as f64).collect()
}

fn lm_points(
    points: &[(f64, f64)],
    options: SmoothOptions,
    x_min: f64,
    x_max: f64,
) -> Vec<SmoothPoint> {
    let n = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| *x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| *y).sum::<f64>() / n;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (x, y) in points {
        sxx += (x - mean_x).powi(2);
        sxy += (x - mean_x) * (y - mean_y);
    }
    if sxx.abs() <= f64::EPSILON {
        return Vec::new();
    }
    let slope = sxy / sxx;
    let intercept = mean_y - slope * mean_x;
    let predict = |x: f64| intercept + slope * x;

    if !options.se {
        return vec![
            SmoothPoint {
                x: x_min,
                y: predict(x_min),
                se: 0.0,
            },
            SmoothPoint {
                x: x_max,
                y: predict(x_max),
                se: 0.0,
            },
        ];
    }

    // Residual standard deviation of the fit (RSS / (n - 2)); the standard error
    // of the fitted mean at x is s * sqrt(1/n + (x - x̄)² / Sxx).
    let rss: f64 = points.iter().map(|(x, y)| (y - predict(*x)).powi(2)).sum();
    let dof = (points.len() as f64 - 2.0).max(1.0);
    let s = (rss / dof).sqrt();
    eval_grid(x_min, x_max, options.eval_points)
        .into_iter()
        .map(|x| {
            let se = s * (1.0 / n + (x - mean_x).powi(2) / sxx).sqrt();
            SmoothPoint {
                x,
                y: predict(x),
                se,
            }
        })
        .collect()
}

fn loess_points(
    points: &[(f64, f64)],
    options: SmoothOptions,
    x_min: f64,
    x_max: f64,
) -> Vec<SmoothPoint> {
    let n = points.len();
    let span = options.span.clamp(f64::MIN_POSITIVE, 1.0);
    // Neighborhood size: at least 2 points, capped at all of them.
    let q = ((span * n as f64).ceil() as usize).clamp(2, n);

    // Estimate the residual noise from the loess fit at the observed x values so
    // the band reflects the smoother's own residuals (spec §15.x).
    let sigma = if options.se {
        let mut rss = 0.0;
        for (x, y) in points {
            if let Some((yhat, _)) = local_linear(points, *x, q) {
                rss += (y - yhat).powi(2);
            }
        }
        let dof = (n as f64 - 2.0).max(1.0);
        (rss / dof).sqrt()
    } else {
        0.0
    };

    eval_grid(x_min, x_max, options.eval_points)
        .into_iter()
        .filter_map(|x| {
            let (yhat, l2) = local_linear(points, x, q)?;
            Some(SmoothPoint {
                x,
                y: yhat,
                se: sigma * l2.sqrt(),
            })
        })
        .collect()
}

/// Local degree-1 weighted regression at `x0` over the `q` nearest points, using
/// tricube weights. Returns the fitted value and `Σ lᵢ²` (the variance factor of
/// the equivalent kernel), or `None` if the local system is singular.
fn local_linear(points: &[(f64, f64)], x0: f64, q: usize) -> Option<(f64, f64)> {
    let n = points.len();
    if n == 0 {
        return None;
    }
    // The q-th nearest distance sets the tricube bandwidth.
    let mut dists: Vec<f64> = points.iter().map(|(x, _)| (x - x0).abs()).collect();
    dists.sort_by(f64::total_cmp);
    let d_max = dists[q.min(n) - 1].max(f64::MIN_POSITIVE);

    // Weighted normal equations for [intercept, slope].
    let (mut s0, mut s1, mut s2) = (0.0, 0.0, 0.0);
    let (mut t0, mut t1) = (0.0, 0.0);
    let mut weights = vec![0.0; n];
    for (i, (x, y)) in points.iter().enumerate() {
        let u = (x - x0).abs() / d_max;
        let w = if u < 1.0 {
            let t = 1.0 - u * u * u;
            t * t * t
        } else {
            0.0
        };
        weights[i] = w;
        let dx = x - x0;
        s0 += w;
        s1 += w * dx;
        s2 += w * dx * dx;
        t0 += w * y;
        t1 += w * dx * y;
    }
    // Solve the 2x2 system centered at x0, so the prediction is the intercept.
    let det = s0 * s2 - s1 * s1;
    if det.abs() <= f64::EPSILON {
        // Degenerate (e.g. a single distinct neighbor): fall back to the
        // weighted mean.
        if s0 <= f64::EPSILON {
            return None;
        }
        let yhat = t0 / s0;
        let l2: f64 = weights.iter().map(|w| (w / s0).powi(2)).sum();
        return Some((yhat, l2));
    }
    let a = (t0 * s2 - t1 * s1) / det; // intercept = prediction at x0
                                       // l_i = w_i * (s2 - s1 * dx_i) / det; ŷ(x0) = Σ l_i y_i.
    let mut l2 = 0.0;
    for (i, (x, _)) in points.iter().enumerate() {
        let dx = x - x0;
        let li = weights[i] * (s2 - s1 * dx) / det;
        l2 += li * li;
    }
    Some((a, l2))
}

#[cfg(test)]
mod smooth_tests {
    use super::*;

    #[test]
    fn lm_recovers_an_exact_line() {
        // y = 3x + 1; an OLS fit returns the two endpoints exactly.
        let mut points: Vec<(f64, f64)> =
            (0..10).map(|i| (i as f64, 3.0 * i as f64 + 1.0)).collect();
        let out = smooth_points(&mut points, SmoothOptions::default());
        assert_eq!(out.len(), 2);
        assert!((out[0].x - 0.0).abs() < 1e-9 && (out[0].y - 1.0).abs() < 1e-9);
        assert!((out[1].x - 9.0).abs() < 1e-9 && (out[1].y - 28.0).abs() < 1e-9);
    }

    #[test]
    fn lm_se_band_is_widest_at_the_extremes() {
        // The standard error of an OLS fit is minimized at the mean of x and
        // grows toward the ends of the range.
        let mut points: Vec<(f64, f64)> = (0..20)
            .map(|i| {
                let x = i as f64;
                // A little deterministic wiggle so the residual variance is > 0.
                (x, 2.0 * x + if i % 2 == 0 { 1.0 } else { -1.0 })
            })
            .collect();
        let out = smooth_points(
            &mut points,
            SmoothOptions {
                se: true,
                ..SmoothOptions::default()
            },
        );
        assert!(out.len() >= 3);
        let mid = out.len() / 2;
        assert!(out[0].se > out[mid].se);
        assert!(out[out.len() - 1].se > out[mid].se);
        assert!(out.iter().all(|p| p.se >= 0.0));
    }

    #[test]
    fn loess_tracks_a_curve_better_than_a_line() {
        // A parabola: loess should sit much closer to the true curve at its
        // vertex than a single straight OLS fit does.
        let f = |x: f64| (x - 5.0).powi(2);
        let mut data: Vec<(f64, f64)> = (0..=10).map(|i| (i as f64, f(i as f64))).collect();
        let loess = smooth_points(
            &mut data.clone(),
            SmoothOptions {
                method: SmoothMethod::Loess,
                span: 0.5,
                eval_points: 11,
                ..SmoothOptions::default()
            },
        );
        let lm = smooth_points(&mut data, SmoothOptions::default());
        // At x = 5 the true value is 0; loess should be far below the OLS line.
        let loess_at_vertex = loess
            .iter()
            .find(|p| (p.x - 5.0).abs() < 1e-6)
            .map(|p| p.y)
            .unwrap();
        let lm_at_vertex = lm[0].y + (lm[1].y - lm[0].y) * 0.5;
        assert!(loess_at_vertex < lm_at_vertex);
        assert!(loess_at_vertex.abs() < 5.0);
    }

    #[test]
    fn loess_is_deterministic() {
        let f = |x: f64| (x * 0.7).sin();
        let mut a: Vec<(f64, f64)> = (0..30)
            .map(|i| (i as f64 * 0.3, f(i as f64 * 0.3)))
            .collect();
        let mut b = a.clone();
        let opts = SmoothOptions {
            method: SmoothMethod::Loess,
            ..SmoothOptions::default()
        };
        assert_eq!(smooth_points(&mut a, opts), smooth_points(&mut b, opts));
    }

    #[test]
    fn too_few_distinct_x_values_yields_no_points() {
        let mut points = vec![(1.0, 2.0), (1.0, 3.0)];
        assert!(smooth_points(&mut points, SmoothOptions::default()).is_empty());
    }
}
