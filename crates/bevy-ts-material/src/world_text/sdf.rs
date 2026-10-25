//! Coverage to signed distance (openspec change `world-text`, design 5): Felzenszwalb and
//! Huttenlocher's squared distance transform, inside and out, seeded at each edge pixel with a
//! sub-pixel distance from its coverage, as Mapbox's TinySDF does. No crate.

const INF: f64 = 1e20;

/// How far, in raster pixels, a distance field reaches past a glyph's edge; farther clamps. An
/// outline is at most this wide.
pub const SPREAD: u32 = 6;

/// `alpha` (`w` × `h` coverage, row by row) padded by `spread` on every side, as `R8Unorm` bytes of
/// `(w + 2 spread)` × `(h + 2 spread)`: 0.5 on the edge, 1 at `spread` pixels inside or deeper, 0 at
/// `spread` pixels outside or farther.
pub fn sdf(alpha: &[u8], w: usize, h: usize, spread: usize) -> Vec<u8> {
    let (pw, ph) = (w + 2 * spread, h + 2 * spread);
    // Squared distance to the glyph (zero inside it), and to the outside (zero outside it).
    let mut outer = vec![INF; pw * ph];
    let mut inner = vec![0.0; pw * ph];
    for y in 0..h {
        for x in 0..w {
            let a = alpha[y * w + x] as f64 / 255.0;
            let i = (y + spread) * pw + x + spread;
            if a >= 1.0 {
                outer[i] = 0.0;
                inner[i] = INF;
            } else if a > 0.0 {
                let d = 0.5 - a;
                outer[i] = if d > 0.0 { d * d } else { 0.0 };
                inner[i] = if d < 0.0 { d * d } else { 0.0 };
            }
        }
    }
    edt(&mut outer, pw, ph);
    edt(&mut inner, pw, ph);
    outer
        .iter()
        .zip(&inner)
        .map(|(o, i)| {
            let d = o.sqrt() - i.sqrt();
            ((0.5 - d / (2.0 * spread as f64)).clamp(0.0, 1.0) * 255.0).round() as u8
        })
        .collect()
}

/// The squared Euclidean distance transform of `grid` in place: columns, then rows.
fn edt(grid: &mut [f64], w: usize, h: usize) {
    let n = w.max(h);
    let (mut f, mut d, mut v, mut z) = (vec![0.0; n], vec![0.0; n], vec![0usize; n], vec![0.0; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = grid[y * w + x];
        }
        edt1d(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            grid[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&grid[y * w..(y + 1) * w]);
        edt1d(&f[..w], &mut d[..w], &mut v, &mut z);
        grid[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
}

/// One line: the lower envelope of the parabolas rooted at each sample.
fn edt1d(f: &[f64], d: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let s = |q: usize, r: usize| ((f[q] + (q * q) as f64) - (f[r] + (r * r) as f64)) / (2.0 * (q as f64 - r as f64));
    let mut k = 0;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;
    for q in 1..n {
        let mut x = s(q, v[k]);
        while x <= z[k] {
            k -= 1;
            x = s(q, v[k]);
        }
        k += 1;
        v[k] = q;
        z[k] = x;
        z[k + 1] = INF;
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let r = v[k];
        *out = (q as f64 - r as f64).powi(2) + f[r];
    }
}
