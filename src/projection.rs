//! Euclidean R^12, proper rotations, and orthographic coordinate-plane views.
//! Source cards are screen-facing annotations, not embedded rigid surfaces.
use serde::Serialize;
pub const DIM: usize = 12;
pub const PLANES: usize = 66;
pub const PAIRS: [[usize; 2]; PLANES] = {
    let mut result = [[0; 2]; PLANES];
    let mut p = 0;
    let mut i = 0;
    while i < DIM {
        let mut j = i + 1;
        while j < DIM {
            result[p] = [i, j];
            p += 1;
            j += 1;
        }
        i += 1;
    }
    result
};
pub type Vector = [f64; DIM];
pub type Matrix = [Vector; DIM];
pub fn identity() -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| if i == j { 1. } else { 0. }))
}
pub fn planes() -> Vec<[usize; 2]> {
    PAIRS.to_vec()
}
pub fn dot(a: Vector, b: Vector) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
pub fn project(r: &Matrix, p: Vector) -> [f32; 2] {
    [dot(r[0], p) as f32, dot(r[1], p) as f32]
}
fn turn(r: &mut Matrix, a: usize, b: usize, theta: f64) {
    let (s, c) = theta.sin_cos();
    let x = r[a];
    let y = r[b];
    for k in 0..DIM {
        r[a][k] = c * x[k] + s * y[k];
        r[b][k] = -s * x[k] + c * y[k];
    }
}
pub fn frame(plane: usize) -> Matrix {
    let [a, b] = PAIRS[plane];
    let mut r = identity();
    // Construct a signed permutation with determinant +1, never a reflection.
    for (row, axis) in [(0, a), (1, b)] {
        let other = (row..DIM).find(|&k| r[k][axis].abs() > 0.5).unwrap();
        if other != row {
            let angle = if r[other][axis] > 0. {
                std::f64::consts::FRAC_PI_2
            } else {
                -std::f64::consts::FRAC_PI_2
            };
            turn(&mut r, row, other, angle);
        } else if r[row][axis] < 0. {
            turn(&mut r, row, 11, std::f64::consts::PI);
        }
    }
    r
}
#[derive(Clone, Debug, Serialize)]
pub struct Rotation {
    pub start: Matrix,
    pub target: Matrix,
    /// A product of Givens rotations. Each interpolated factor is in SO(12).
    pub factors: Vec<(usize, usize, f64)>,
}
impl Rotation {
    pub fn new(start: Matrix, target: Matrix) -> Self {
        // Q=target*start^T. Reduce Q to identity by positive-diagonal QR.
        let mut q: Matrix =
            std::array::from_fn(|i| std::array::from_fn(|j| dot(target[i], start[j])));
        let mut factors = Vec::new();
        for col in 0..DIM - 1 {
            for row in (col + 1..DIM).rev() {
                let angle = q[row][col].atan2(q[col][col]);
                if angle.abs() > 1e-13 {
                    turn(&mut q, col, row, angle);
                    factors.push((col, row, -angle));
                }
            }
        }
        factors.reverse();
        Self {
            start,
            target,
            factors,
        }
    }
    pub fn at(&self, t: f64) -> Matrix {
        if t >= 1. {
            return self.target;
        }
        if t <= 0. {
            return self.start;
        }
        // Quintic time parameter has zero velocity and acceleration at both ends.
        let s = t * t * t * (10. + t * (-15. + 6. * t));
        self.at_parameter(s)
    }
    fn at_parameter(&self, s: f64) -> Matrix {
        let mut r = self.start;
        for &(a, b, angle) in &self.factors {
            turn(&mut r, a, b, angle * s);
        }
        r
    }

    /// Conservative bounds for the entire continuous rotation, including annotations.
    /// For R(s) = product G_i(s), ||dR/ds|| <= sum |angle_i|. Thus a point
    /// between samples moves at most ||p|| * sum |angle_i| / (2 * STEPS).
    /// Sampling alone would miss extrema; this analytic margin covers them.
    pub fn swept_bounds(&self, files: &[(Vector, [f32; 2])]) -> crate::geometry::Rect {
        use crate::geometry::Rect;
        use rayon::prelude::*;
        const STEPS: usize = 64;
        let frames: Vec<_> = (0..=STEPS)
            .map(|i| self.at_parameter(i as f64 / STEPS as f64))
            .collect();
        let speed: f64 = self.factors.iter().map(|f| f.2.abs()).sum();
        let bounds = files
            .par_iter()
            .map(|&(p, size)| {
                let norm = dot(p, p).sqrt();
                let pad = norm * (speed / (2. * STEPS as f64) + 1e-5) + 1.;
                let mut b = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for r in &frames {
                    for k in 0..2 {
                        let v = dot(r[k], p);
                        let half = size[k] as f64 * 0.5 + pad;
                        b[k] = b[k].min(v - half);
                        b[k + 2] = b[k + 2].max(v + half);
                    }
                }
                b
            })
            .reduce(
                || {
                    [
                        f64::INFINITY,
                        f64::INFINITY,
                        f64::NEG_INFINITY,
                        f64::NEG_INFINITY,
                    ]
                },
                |a, b| {
                    [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[2].max(b[2]),
                        a[3].max(b[3]),
                    ]
                },
            );
        if files.is_empty() {
            return Rect::new(-500., -500., 1000., 1000.);
        }
        Rect::new(
            bounds[0] as f32,
            bounds[1] as f32,
            (bounds[2] - bounds[0]) as f32,
            (bounds[3] - bounds[1]) as f32,
        )
    }
}
/// Squared bivector components. Nonnegative and sum to 1 for any orthonormal frame.
/// At a coordinate plane exactly one weight is 1; all other plane layers are absent.
pub fn weights(r: &Matrix) -> [f32; PLANES] {
    std::array::from_fn(|p| {
        let [i, j] = PAIRS[p];
        let x = r[0][i] * r[1][j] - r[0][j] * r[1][i];
        (x * x) as f32
    })
}
pub fn orthogonality_error(r: &Matrix) -> f64 {
    (0..DIM)
        .flat_map(|i| (0..DIM).map(move |j| (dot(r[i], r[j]) - if i == j { 1. } else { 0. }).abs()))
        .fold(0., f64::max)
}
pub fn hash(s: &str) -> u64 {
    s.bytes().fold(14695981039346656037u64, |h, b| {
        (h ^ b as u64).wrapping_mul(1099511628211)
    })
}
pub fn coordinate(path: &str, axis: usize) -> f64 {
    let h = hash(&format!("{axis}:{path}"));
    (h >> 11) as f64 / ((1u64 << 53) as f64) * 2. - 1.
}
