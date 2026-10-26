//! Campionamento numerico per il grafico: curve spezzate agli asintoti,
//! ricerca delle radici, curve implicite (marching squares) e mappe di colore.

use egui::Color32;

use crate::analysis::Func;

#[derive(Clone, Copy)]
pub struct View {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

impl View {
    pub fn w(&self) -> f64 {
        self.x1 - self.x0
    }

    pub fn h(&self) -> f64 {
        self.y1 - self.y0
    }
}

/// Spezza una polilinea dove la funzione diverge o salta attraverso la vista.
fn push_point(segs: &mut Vec<Vec<[f64; 2]>>, p: [f64; 2], prev: &mut Option<[f64; 2]>, lim: (f64, f64)) {
    let (lo, hi) = lim;
    if !p[0].is_finite() || !p[1].is_finite() {
        *prev = None;
        return;
    }
    let span = hi - lo;
    if let Some(q) = *prev {
        let jump = (q[1] > hi + span && p[1] < lo - span) || (q[1] < lo - span && p[1] > hi + span);
        if jump {
            segs.push(Vec::new());
        }
    } else {
        segs.push(Vec::new());
    }
    // Limita i valori enormi per non degradare la tassellazione.
    let clamped = [p[0], p[1].clamp(lo - 10.0 * span, hi + 10.0 * span)];
    segs.last_mut().unwrap().push(clamped);
    *prev = Some(p);
}

fn finish(segs: Vec<Vec<[f64; 2]>>) -> Vec<Vec<[f64; 2]>> {
    segs.into_iter().filter(|s| s.len() > 1).collect()
}

/// Campiona y = f(a) su [a0, a1].
pub fn sample_function(f: &mut Func, a0: f64, a1: f64, n: usize, ylim: (f64, f64)) -> Vec<Vec<[f64; 2]>> {
    let mut segs = Vec::new();
    let mut prev = None;
    for i in 0..=n {
        let a = a0 + (a1 - a0) * i as f64 / n as f64;
        push_point(&mut segs, [a, f.eval1(a)], &mut prev, ylim);
    }
    finish(segs)
}

/// Campiona x = g(y): stessa logica con gli assi scambiati.
pub fn sample_function_of_y(g: &mut Func, v: View, n: usize) -> Vec<Vec<[f64; 2]>> {
    let ext = v.h() * 0.5;
    sample_function(g, v.y0 - ext, v.y1 + ext, n, (v.x0, v.x1))
        .into_iter()
        .map(|s| s.into_iter().map(|[y, x]| [x, y]).collect())
        .collect()
}

pub fn sample_parametric(fx: &mut Func, fy: &mut Func, t0: f64, t1: f64, n: usize, v: View) -> Vec<Vec<[f64; 2]>> {
    let max_jump = 3.0 * (v.w().hypot(v.h()));
    let mut segs: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut prev: Option<[f64; 2]> = None;
    for i in 0..=n {
        let t = t0 + (t1 - t0) * i as f64 / n as f64;
        let p = [fx.eval1(t), fy.eval1(t)];
        if !p[0].is_finite() || !p[1].is_finite() {
            prev = None;
            continue;
        }
        match prev {
            Some(q) if (p[0] - q[0]).hypot(p[1] - q[1]) < max_jump => {}
            _ => segs.push(Vec::new()),
        }
        segs.last_mut().unwrap().push(p);
        prev = Some(p);
    }
    finish(segs)
}

pub fn sample_polar(r: &mut Func, th0: f64, th1: f64, degrees: bool, n: usize, v: View) -> Vec<Vec<[f64; 2]>> {
    let mut x = Vec::with_capacity(n + 1);
    let k = if degrees { 180.0 / std::f64::consts::PI } else { 1.0 };
    for i in 0..=n {
        let th = th0 + (th1 - th0) * i as f64 / n as f64;
        let rv = r.eval1(th * k);
        x.push([rv * th.cos(), rv * th.sin()]);
    }
    let max_jump = 3.0 * (v.w().hypot(v.h()));
    let mut segs: Vec<Vec<[f64; 2]>> = vec![Vec::new()];
    for (i, p) in x.iter().enumerate() {
        if !p[0].is_finite() || !p[1].is_finite() {
            segs.push(Vec::new());
            continue;
        }
        if i > 0 {
            let q = x[i - 1];
            if (p[0] - q[0]).hypot(p[1] - q[1]) > max_jump {
                segs.push(Vec::new());
            }
        }
        segs.last_mut().unwrap().push(*p);
    }
    finish(segs)
}

/// Radici di f su [a0, a1]: cambi di segno raffinati per bisezione, più i
/// minimi locali di |f| che toccano lo zero (radici doppie).
pub fn find_roots(f: &mut Func, a0: f64, a1: f64, n: usize) -> Vec<f64> {
    let xs: Vec<f64> = (0..=n).map(|i| a0 + (a1 - a0) * i as f64 / n as f64).collect();
    let ys: Vec<f64> = xs.iter().map(|&x| f.eval1(x)).collect();
    let scale = ys.iter().filter(|y| y.is_finite()).fold(0.0_f64, |m, y| m.max(y.abs())).max(1e-300);
    let mut roots: Vec<f64> = Vec::new();
    let tol = (a1 - a0) / n as f64 * 1e-3;
    let push = |r: f64, roots: &mut Vec<f64>| {
        if !roots.iter().any(|q| (q - r).abs() < tol.max(1e-12)) {
            roots.push(r);
        }
    };
    for i in 0..n {
        let (ya, yb) = (ys[i], ys[i + 1]);
        if !ya.is_finite() || !yb.is_finite() {
            continue;
        }
        if ya == 0.0 {
            push(xs[i], &mut roots);
            continue;
        }
        if ya.signum() != yb.signum() && yb != 0.0 {
            let (mut lo, mut hi, mut flo) = (xs[i], xs[i + 1], ya);
            for _ in 0..80 {
                let mid = 0.5 * (lo + hi);
                let fm = f.eval1(mid);
                if fm.signum() == flo.signum() {
                    lo = mid;
                    flo = fm;
                } else {
                    hi = mid;
                }
            }
            let r = 0.5 * (lo + hi);
            // Scarta i poli (es. tan): lì |f| resta grande vicino al "cambio di segno".
            let fr = f.eval1(r);
            if fr.is_finite() && fr.abs() < 1e-6 * scale.max(1.0) {
                push(r, &mut roots);
            }
        }
    }
    // Radici di molteplicità pari: minimi locali di |f| molto vicini a zero.
    for i in 1..n {
        let (a, b, c) = (ys[i - 1].abs(), ys[i].abs(), ys[i + 1].abs());
        if !(b <= a && b <= c) || ys[i - 1].signum() != ys[i + 1].signum() {
            continue;
        }
        let (mut lo, mut hi) = (xs[i - 1], xs[i + 1]);
        for _ in 0..100 {
            let m1 = lo + (hi - lo) / 3.0;
            let m2 = hi - (hi - lo) / 3.0;
            if f.eval1(m1).abs() < f.eval1(m2).abs() {
                hi = m2;
            } else {
                lo = m1;
            }
        }
        let r = 0.5 * (lo + hi);
        if f.eval1(r).abs() < 1e-10 * scale.max(1.0) {
            push(r, &mut roots);
        }
    }
    roots.sort_by(f64::total_cmp);
    roots
}

/// Marching squares su una griglia cols × rows: segmenti della curva f = 0.
pub fn implicit_segments(f: &mut Func, v: View, cols: usize, rows: usize) -> Vec<[[f64; 2]; 2]> {
    let dx = v.w() / cols as f64;
    let dy = v.h() / rows as f64;
    let mut grid = vec![0.0; (cols + 1) * (rows + 1)];
    for j in 0..=rows {
        let y = v.y0 + j as f64 * dy;
        for i in 0..=cols {
            grid[j * (cols + 1) + i] = f.eval2(v.x0 + i as f64 * dx, y);
        }
    }
    let at = |i: usize, j: usize| grid[j * (cols + 1) + i];
    let mut segs = Vec::new();
    for j in 0..rows {
        for i in 0..cols {
            // Vertici in senso antiorario a partire da (i, j)
            let corners = [
                (v.x0 + i as f64 * dx, v.y0 + j as f64 * dy, at(i, j)),
                (v.x0 + (i + 1) as f64 * dx, v.y0 + j as f64 * dy, at(i + 1, j)),
                (v.x0 + (i + 1) as f64 * dx, v.y0 + (j + 1) as f64 * dy, at(i + 1, j + 1)),
                (v.x0 + i as f64 * dx, v.y0 + (j + 1) as f64 * dy, at(i, j + 1)),
            ];
            if corners.iter().any(|c| !c.2.is_finite()) {
                continue;
            }
            let mut pts: Vec<[f64; 2]> = Vec::with_capacity(4);
            for k in 0..4 {
                let (ax, ay, fa) = corners[k];
                let (bx, by, fb) = corners[(k + 1) % 4];
                if (fa < 0.0) != (fb < 0.0) {
                    let t = fa / (fa - fb);
                    let p = [ax + t * (bx - ax), ay + t * (by - ay)];
                    // Un vero zero ha |f| piccolo nel punto interpolato; un polo no.
                    let fp = f.eval2(p[0], p[1]);
                    if fp.is_finite() && fp.abs() <= 0.75 * fa.abs().max(fb.abs()) {
                        pts.push(p);
                    }
                }
            }
            match pts.len() {
                2 => segs.push([pts[0], pts[1]]),
                4 => {
                    // Punto di sella: disambigua con il valore al centro
                    let center = f.eval2(v.x0 + (i as f64 + 0.5) * dx, v.y0 + (j as f64 + 0.5) * dy);
                    if (center < 0.0) == (corners[0].2 < 0.0) {
                        segs.push([pts[0], pts[3]]);
                        segs.push([pts[1], pts[2]]);
                    } else {
                        segs.push([pts[0], pts[1]]);
                        segs.push([pts[2], pts[3]]);
                    }
                }
                _ => {}
            }
        }
    }
    segs
}

/// Valori di f(x, y) su griglia (righe dall'alto verso il basso) e intervallo robusto.
pub fn field_values(f: &mut Func, v: View, cols: usize, rows: usize) -> (Vec<f64>, f64, f64) {
    let mut vals = Vec::with_capacity(cols * rows);
    for j in 0..rows {
        let y = v.y1 - (j as f64 + 0.5) * v.h() / rows as f64;
        for i in 0..cols {
            let x = v.x0 + (i as f64 + 0.5) * v.w() / cols as f64;
            vals.push(f.eval2(x, y));
        }
    }
    let mut finite: Vec<f64> = vals.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return (vals, 0.0, 1.0);
    }
    finite.sort_by(f64::total_cmp);
    // Percentili 2–98 per non far dominare le singolarità.
    let lo = finite[finite.len() * 2 / 100];
    let hi = finite[(finite.len() * 98 / 100).min(finite.len() - 1)];
    (vals, lo, hi)
}

fn lerp_color(stops: &[(f32, [u8; 3])], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t <= t1 {
            let k = (t - t0) / (t1 - t0);
            return std::array::from_fn(|i| (c0[i] as f32 + (c1[i] as f32 - c0[i] as f32) * k) as u8);
        }
    }
    stops.last().unwrap().1
}

/// Colori della mappa: divergente (blu–neutro–arancio) se i valori attraversano lo zero,
/// altrimenti sequenziale.
pub fn field_image(vals: &[f64], cols: usize, rows: usize, lo: f64, hi: f64, dark: bool) -> egui::ColorImage {
    let neutral = if dark { [44, 44, 46] } else { [250, 250, 252] };
    let diverging = [(0.0, [10, 96, 220]), (0.5, neutral), (1.0, [255, 120, 20])];
    let sequential = [
        (0.0, [68, 1, 84]),
        (0.25, [59, 82, 139]),
        (0.5, [33, 145, 140]),
        (0.75, [94, 201, 98]),
        (1.0, [253, 231, 37]),
    ];
    let div = lo < 0.0 && hi > 0.0;
    let m = lo.abs().max(hi.abs()).max(1e-300);
    let span = (hi - lo).max(1e-300);
    let pixels = vals
        .iter()
        .map(|&v| {
            if !v.is_finite() {
                return Color32::TRANSPARENT;
            }
            let c = if div {
                lerp_color(&diverging, (0.5 + 0.5 * v / m) as f32)
            } else {
                lerp_color(&sequential, ((v - lo) / span) as f32)
            };
            Color32::from_rgba_unmultiplied(c[0], c[1], c[2], 215)
        })
        .collect();
    egui::ColorImage { size: [cols, rows], pixels, source_size: egui::vec2(cols as f32, rows as f32) }
}
