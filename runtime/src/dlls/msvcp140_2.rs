//! msvcp140_2.dll maison (#2187) : les fonctions mathématiques spéciales de
//! C++17 (`<cmath>`, norme C++ [sf.cmath]) de la bibliothèque C++ de Visual
//! Studio, pour les machines sans redistribuable. Qt 6 (Qt6Gui, OBS) en
//! importe `__std_smf_hypot3f`.
//!
//! Les 44 exports `extern "C"` `__std_smf_*` (double et float) sont déclarés
//! dans la bibliothèque standard publique de Microsoft (MS STL, licence
//! Apache 2.0, `stl/inc/cmath` et `stl/src/special_math.cpp`), qui en fixe
//! aussi le comportement d'erreur : un argument NaN est rendu tel quel, une
//! erreur de domaine rend NaN avec `errno = EDOM`.
//!
//! Les calculs sont écrits ici d'après les définitions mathématiques
//! publiques (norme C++ et NIST DLMF) :
//! - polynômes (Legendre, Laguerre, Hermite) : relations de récurrence ;
//! - harmoniques sphériques : récurrence sur les fonctions normalisées ;
//! - bêta : fonction gamma de la libm ;
//! - zêta de Riemann : série alternée accélérée (Borwein) et réflexion ;
//! - exponentielle intégrale : série, développement asymptotique et
//!   fraction continue (DLMF 6.6, 6.9, 6.12) ;
//! - intégrales elliptiques : formes symétriques de Carlson RF, RD, RJ, RC
//!   par duplication (DLMF 19.25, 19.36) ;
//! - Bessel : `jn`/`yn` de la libm pour un ordre entier, sinon série
//!   (DLMF 10.2, 10.25), représentations intégrales (DLMF 10.9.6, 10.9.7,
//!   10.32.4, 10.32.9) et développement de Hankel (DLMF 10.17).
//!
//! Limite connue : `ellint_3` et `comp_ellint_3` avec `ν·sin²φ ≥ 1` (valeur
//! principale de Cauchy) rendent NaN et EDOM.
//!
//! Quand la vraie msvcp140_2.dll est présente, c'est elle qui est chargée :
//! voir `dlls::prefers_real_file`.

use super::Exports;
use std::f64::consts::{FRAC_PI_2, PI};
use std::sync::OnceLock;

extern "C" {
    fn jn(n: i32, x: f64) -> f64;
    fn yn(n: i32, x: f64) -> f64;
    fn lgamma_r(x: f64, sign: *mut i32) -> f64;
    fn tgamma(x: f64) -> f64;
}

/// errno de la CRT Windows pour une erreur de domaine.
const EDOM: i32 = 33;
use std::f64::consts::EULER_GAMMA;

fn domain_error() -> f64 {
    super::crt_runtime::set_errno_value(EDOM);
    f64::NAN
}

// ---------- outils ----------

/// Nœuds et poids de Gauss-Legendre à 16 points sur [-1, 1], calculés une
/// fois par la méthode de Newton sur le polynôme de Legendre.
fn gauss16() -> &'static [(f64, f64); 16] {
    static NODES: OnceLock<[(f64, f64); 16]> = OnceLock::new();
    NODES.get_or_init(|| {
        const N: usize = 16;
        let mut out = [(0.0, 0.0); N];
        for (i, slot) in out.iter_mut().enumerate() {
            let mut x = (PI * (i as f64 + 0.75) / (N as f64 + 0.5)).cos();
            let mut dp = 1.0;
            for _ in 0..100 {
                let (mut p0, mut p1) = (1.0, x);
                for k in 2..=N {
                    let k = k as f64;
                    let p2 = ((2.0 * k - 1.0) * x * p1 - (k - 1.0) * p0) / k;
                    p0 = p1;
                    p1 = p2;
                }
                dp = N as f64 * (x * p1 - p0) / (x * x - 1.0);
                let dx = p1 / dp;
                x -= dx;
                if dx.abs() < 1e-16 {
                    break;
                }
            }
            *slot = (x, 2.0 / ((1.0 - x * x) * dp * dp));
        }
        out
    })
}

/// ∫ f sur [a, b] découpé en `panels` intervalles de Gauss-Legendre.
fn integrate(f: impl Fn(f64) -> f64, a: f64, b: f64, panels: usize) -> f64 {
    let nodes = gauss16();
    let h = (b - a) / panels as f64;
    let mut sum = 0.0;
    for p in 0..panels {
        let mid = a + (p as f64 + 0.5) * h;
        let mut s = 0.0;
        for &(x, w) in nodes {
            s += w * f(mid + 0.5 * h * x);
        }
        sum += s * 0.5 * h;
    }
    sum
}

fn is_integer(v: f64) -> bool {
    v.fract() == 0.0 && v.abs() < 2.0e9
}

/// 1/Γ(v), nul aux pôles de Γ.
fn rgamma(v: f64) -> f64 {
    if v <= 0.0 && is_integer(v) {
        return 0.0;
    }
    if v < 170.0 {
        return 1.0 / unsafe { tgamma(v) };
    }
    let mut sign = 1;
    let l = unsafe { lgamma_r(v, &mut sign) };
    sign as f64 * (-l).exp()
}

// ---------- hypot à trois arguments ----------

macro_rules! hypot3 {
    ($name:ident, $t:ty) => {
        fn $name(x: $t, y: $t, z: $t) -> $t {
            let (mut x, mut y, mut z) = (x.abs(), y.abs(), z.abs());
            if x == <$t>::INFINITY || y == <$t>::INFINITY || z == <$t>::INFINITY {
                return <$t>::INFINITY;
            }
            if y > x {
                std::mem::swap(&mut x, &mut y);
            }
            if z > x {
                std::mem::swap(&mut x, &mut z);
            }
            if x * <$t>::EPSILON >= y && x * <$t>::EPSILON >= z {
                return x;
            }
            let (a, b) = (y / x, z / x);
            x * (1.0 + a * a + b * b).sqrt()
        }
    };
}
hypot3!(hypot3_f64, f64);
hypot3!(hypot3_f32, f32);

// ---------- polynômes ----------

fn legendre(l: u32, x: f64) -> f64 {
    assoc_legendre(l, 0, x)
}

/// P_l^m(x) sans le facteur de Condon-Shortley (définition de la norme).
fn assoc_legendre(l: u32, m: u32, x: f64) -> f64 {
    if x.abs() > 1.0 {
        return domain_error();
    }
    if m > l {
        return 0.0;
    }
    let s = ((1.0 - x) * (1.0 + x)).sqrt();
    let mut pmm = 1.0;
    let mut odd = 1.0;
    for _ in 0..m {
        pmm *= odd * s;
        odd += 2.0;
    }
    if l == m {
        return pmm;
    }
    let mut p0 = pmm;
    let mut p1 = x * (2 * m + 1) as f64 * pmm;
    for k in (m + 2)..=l {
        let p2 = (x * (2 * k - 1) as f64 * p1 - (k + m - 1) as f64 * p0) / (k - m) as f64;
        p0 = p1;
        p1 = p2;
    }
    p1
}

/// Y_l^m(θ, 0) : (-1)^m √((2l+1)/4π · (l-m)!/(l+m)!) P_l^m(cos θ), calculée
/// par la récurrence des fonctions normalisées (pas de factorielles).
fn sph_legendre(l: u32, m: u32, theta: f64) -> f64 {
    if m > l {
        return 0.0;
    }
    let x = theta.cos();
    let s = theta.sin().abs();
    let (lf, mf) = (l as f64, m as f64);
    // Y_m^m.
    let mut ymm = if m == 0 {
        (1.0 / (4.0 * PI)).sqrt()
    } else if s == 0.0 {
        0.0
    } else {
        let mut log = (2.0 * mf + 1.0).ln() - (4.0 * PI).ln();
        for i in 1..=m {
            log += ((2 * i - 1) as f64 / (2 * i) as f64).ln();
        }
        (0.5 * log + mf * s.ln()).exp()
    };
    if m % 2 == 1 {
        ymm = -ymm;
    }
    if l == m {
        return ymm;
    }
    let mut y0 = ymm;
    let mut y1 = x * (2.0 * mf + 3.0).sqrt() * ymm;
    let mut k = mf + 2.0;
    while k <= lf {
        let a = ((4.0 * k * k - 1.0) / (k * k - mf * mf)).sqrt();
        let b = ((2.0 * k + 1.0) * ((k - 1.0) * (k - 1.0) - mf * mf)
            / ((2.0 * k - 3.0) * (k * k - mf * mf)))
            .sqrt();
        let y2 = a * x * y1 - b * y0;
        y0 = y1;
        y1 = y2;
        k += 1.0;
    }
    y1
}

fn assoc_laguerre(n: u32, m: u32, x: f64) -> f64 {
    let m = m as f64;
    if n == 0 {
        return 1.0;
    }
    let mut l0 = 1.0;
    let mut l1 = 1.0 + m - x;
    for k in 1..n {
        let k = k as f64;
        let l2 = ((2.0 * k + 1.0 + m - x) * l1 - (k + m) * l0) / (k + 1.0);
        l0 = l1;
        l1 = l2;
    }
    l1
}

fn laguerre(n: u32, x: f64) -> f64 {
    assoc_laguerre(n, 0, x)
}

fn hermite(n: u32, x: f64) -> f64 {
    if n == 0 {
        return 1.0;
    }
    let mut h0 = 1.0;
    let mut h1 = 2.0 * x;
    for k in 1..n {
        let h2 = 2.0 * x * h1 - 2.0 * k as f64 * h0;
        h0 = h1;
        h1 = h2;
    }
    h1
}

// ---------- bêta, zêta, exponentielle intégrale ----------

fn beta(x: f64, y: f64) -> f64 {
    if x <= 0.0 || y <= 0.0 {
        return domain_error();
    }
    if x + y < 170.0 {
        return unsafe { tgamma(x) * tgamma(y) / tgamma(x + y) };
    }
    let mut s = 0;
    let l = unsafe { lgamma_r(x, &mut s) + lgamma_r(y, &mut s) - lgamma_r(x + y, &mut s) };
    l.exp()
}

/// η(s) = Σ (-1)^(k) / (k+1)^s pour s > 0, accélérée par l'algorithme 2
/// de Borwein (n = 40).
fn dirichlet_eta(s: f64) -> f64 {
    const N: usize = 40;
    let n = N as f64;
    let mut d = [0.0f64; N + 1];
    let mut term = 1.0;
    let mut sum = 1.0;
    d[0] = sum;
    for (i, slot) in d.iter_mut().enumerate().skip(1) {
        let fi = i as f64;
        term *= 4.0 * (n + fi - 1.0) * (n - fi + 1.0) / ((2.0 * fi) * (2.0 * fi - 1.0));
        sum += term;
        *slot = sum;
    }
    let dn = d[N];
    let mut acc = 0.0;
    for (k, dk) in d.iter().take(N).enumerate() {
        let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
        acc += sign * (dk - dn) / ((k + 1) as f64).powf(s);
    }
    -acc / dn
}

fn riemann_zeta(s: f64) -> f64 {
    if s == 1.0 {
        return domain_error();
    }
    if s == 0.0 {
        return -0.5;
    }
    if s > 0.0 {
        if s > 60.0 {
            return 1.0 + 2f64.powf(-s) + 3f64.powf(-s);
        }
        // ζ(s) = η(s) / (1 - 2^(1-s)), dénominateur précis près de s = 1.
        return dirichlet_eta(s) / -((1.0 - s) * std::f64::consts::LN_2).exp_m1();
    }
    // Réflexion : ζ(s) = 2^s π^(s-1) sin(πs/2) Γ(1-s) ζ(1-s).
    if is_integer(s) && (s as i64) % 2 == 0 {
        return 0.0;
    }
    let z = riemann_zeta(1.0 - s);
    let sin = (FRAC_PI_2 * s).sin();
    let mut sign = 1;
    let lg = unsafe { lgamma_r(1.0 - s, &mut sign) };
    let log = s * std::f64::consts::LN_2 + (s - 1.0) * PI.ln() + lg;
    sign as f64 * sin * z * log.exp()
}

/// E1(z) pour z > 0.
fn expint_e1(z: f64) -> f64 {
    if z <= 1.0 {
        let mut sum = 0.0;
        let mut term = 1.0;
        for k in 1..200 {
            term *= -z / k as f64;
            let t = term / k as f64;
            sum += t;
            if t.abs() < 1e-18 * sum.abs() {
                break;
            }
        }
        return -EULER_GAMMA - z.ln() - sum;
    }
    // Fraction continue (DLMF 6.9.1), évaluée par la méthode de Lentz :
    // E1(z) = e^-z / (z + 1 - 1²/(z + 3 - 2²/(z + 5 - …))).
    const TINY: f64 = 1e-300;
    let mut b = z + 1.0;
    let mut c = 1.0 / TINY;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..1000 {
        let a = -((i * i) as f64);
        b += 2.0;
        d = 1.0 / (a * d + b);
        c = b + a / c;
        let del = c * d;
        h *= del;
        if (del - 1.0).abs() < 1e-16 {
            break;
        }
    }
    h * (-z).exp()
}

/// Ei(x) (définition de la norme : -∫_{-x}^∞ e^-t / t dt).
fn expint(x: f64) -> f64 {
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if x < 0.0 {
        return -expint_e1(-x);
    }
    if x < 40.0 {
        let mut sum = 0.0;
        let mut term = 1.0;
        for k in 1..500 {
            term *= x / k as f64;
            let t = term / k as f64;
            sum += t;
            if t < 1e-18 * sum {
                break;
            }
        }
        return EULER_GAMMA + x.ln() + sum;
    }
    if x > 720.0 {
        return f64::INFINITY;
    }
    let mut sum = 1.0;
    let mut term = 1.0;
    for k in 1..100 {
        let next = term * k as f64 / x;
        if next > term {
            break;
        }
        term = next;
        sum += term;
        if term < 1e-18 {
            break;
        }
    }
    x.exp() / x * sum
}

// ---------- intégrales elliptiques (Carlson) ----------

/// RC(x, y), y ≠ 0.
fn carlson_rc(x: f64, y: f64) -> f64 {
    if y > x {
        let d = y - x;
        if d < 1e-8 * y {
            return (1.0 - (x - y) / (6.0 * y)) / y.sqrt();
        }
        (d / x).sqrt().atan() / d.sqrt()
    } else if y < x {
        let d = x - y;
        if d < 1e-8 * x {
            return (1.0 + d / (6.0 * x)) / x.sqrt();
        }
        if y > 0.0 {
            (d / x).sqrt().atanh() / d.sqrt()
        } else {
            // Valeur principale (DLMF 19.2.20).
            (x / (x - y)).sqrt() * carlson_rc(x - y, -y)
        }
    } else {
        1.0 / x.sqrt()
    }
}

/// RF(x, y, z) : duplication jusqu'à un écart relatif de 1e-9 ; le terme du
/// premier ordre est nul, l'erreur restante est d'ordre 1e-18.
fn carlson_rf(mut x: f64, mut y: f64, mut z: f64) -> f64 {
    for _ in 0..200 {
        let mu = (x + y + z) / 3.0;
        let dev = (x - mu).abs().max((y - mu).abs()).max((z - mu).abs()) / mu;
        if dev < 1e-9 {
            return 1.0 / mu.sqrt();
        }
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lambda = sx * sy + sx * sz + sy * sz;
        x = (x + lambda) / 4.0;
        y = (y + lambda) / 4.0;
        z = (z + lambda) / 4.0;
    }
    1.0 / ((x + y + z) / 3.0).sqrt()
}

/// RD(x, y, z).
fn carlson_rd(mut x: f64, mut y: f64, mut z: f64) -> f64 {
    let mut sum = 0.0;
    let mut scale = 1.0;
    for _ in 0..200 {
        let mu = (x + y + 3.0 * z) / 5.0;
        let dev = (x - mu).abs().max((y - mu).abs()).max((z - mu).abs()) / mu;
        if dev < 1e-9 {
            break;
        }
        let (sx, sy, sz) = (x.sqrt(), y.sqrt(), z.sqrt());
        let lambda = sx * sy + sx * sz + sy * sz;
        sum += scale / (sz * (z + lambda));
        scale /= 4.0;
        x = (x + lambda) / 4.0;
        y = (y + lambda) / 4.0;
        z = (z + lambda) / 4.0;
    }
    let mu = (x + y + 3.0 * z) / 5.0;
    3.0 * sum + scale * mu.powf(-1.5)
}

/// RJ(x, y, z, p), p > 0.
fn carlson_rj(mut x: f64, mut y: f64, mut z: f64, mut p: f64) -> f64 {
    let mut sum = 0.0;
    let mut scale = 1.0;
    for _ in 0..200 {
        let mu = (x + y + z + 2.0 * p) / 5.0;
        let dev = (x - mu)
            .abs()
            .max((y - mu).abs())
            .max((z - mu).abs())
            .max((p - mu).abs())
            / mu;
        if dev < 1e-9 {
            break;
        }
        let (sx, sy, sz, sp) = (x.sqrt(), y.sqrt(), z.sqrt(), p.sqrt());
        let lambda = sx * sy + sx * sz + sy * sz;
        let alpha = p * (sx + sy + sz) + sx * sy * sz;
        let beta = sp * (p + lambda);
        sum += scale * carlson_rc(alpha * alpha, beta * beta);
        scale /= 4.0;
        x = (x + lambda) / 4.0;
        y = (y + lambda) / 4.0;
        z = (z + lambda) / 4.0;
        p = (p + lambda) / 4.0;
    }
    let mu = (x + y + z + 2.0 * p) / 5.0;
    3.0 * sum + scale * mu.powf(-1.5)
}

fn comp_ellint_1(k: f64) -> f64 {
    if k.abs() > 1.0 {
        return domain_error();
    }
    if k.abs() == 1.0 {
        return f64::INFINITY;
    }
    carlson_rf(0.0, 1.0 - k * k, 1.0)
}

fn comp_ellint_2(k: f64) -> f64 {
    if k.abs() > 1.0 {
        return domain_error();
    }
    if k.abs() == 1.0 {
        return 1.0;
    }
    let y = 1.0 - k * k;
    carlson_rf(0.0, y, 1.0) - k * k / 3.0 * carlson_rd(0.0, y, 1.0)
}

fn comp_ellint_3(k: f64, nu: f64) -> f64 {
    if k.abs() > 1.0 || nu > 1.0 {
        return domain_error();
    }
    if nu == 1.0 || k.abs() == 1.0 {
        return f64::INFINITY;
    }
    let y = 1.0 - k * k;
    carlson_rf(0.0, y, 1.0) + nu / 3.0 * carlson_rj(0.0, y, 1.0, 1.0 - nu)
}

/// Réduit φ à [-π/2, π/2] : φ = nπ + r.
fn reduce(phi: f64) -> (f64, f64) {
    let n = (phi / PI).round();
    (n, phi - n * PI)
}

fn ellint_1(k: f64, phi: f64) -> f64 {
    if k.abs() > 1.0 {
        return domain_error();
    }
    let (n, r) = reduce(phi);
    let (s, c) = r.sin_cos();
    let delta = 1.0 - k * k * s * s;
    let part = if s == 0.0 {
        0.0
    } else {
        s * carlson_rf(c * c, delta, 1.0)
    };
    if n == 0.0 {
        part
    } else {
        2.0 * n * comp_ellint_1(k) + part
    }
}

fn ellint_2(k: f64, phi: f64) -> f64 {
    if k.abs() > 1.0 {
        return domain_error();
    }
    let (n, r) = reduce(phi);
    let (s, c) = r.sin_cos();
    let delta = 1.0 - k * k * s * s;
    let part = if s == 0.0 {
        0.0
    } else {
        s * carlson_rf(c * c, delta, 1.0) - k * k / 3.0 * s * s * s * carlson_rd(c * c, delta, 1.0)
    };
    if n == 0.0 {
        part
    } else {
        2.0 * n * comp_ellint_2(k) + part
    }
}

fn ellint_3(k: f64, nu: f64, phi: f64) -> f64 {
    if k.abs() > 1.0 {
        return domain_error();
    }
    let (n, r) = reduce(phi);
    let (s, c) = r.sin_cos();
    let p = 1.0 - nu * s * s;
    if p <= 0.0 || (n != 0.0 && nu >= 1.0) {
        return domain_error();
    }
    let delta = 1.0 - k * k * s * s;
    let part = if s == 0.0 {
        0.0
    } else {
        s * carlson_rf(c * c, delta, 1.0) + nu / 3.0 * s * s * s * carlson_rj(c * c, delta, 1.0, p)
    };
    if n == 0.0 {
        part
    } else {
        2.0 * n * comp_ellint_3(k, nu) + part
    }
}

// ---------- Bessel ----------

/// Série de J_ν (signe = -1) ou de I_ν (signe = +1), DLMF 10.2.2 et 10.25.2.
fn bessel_series(nu: f64, x: f64, sign: f64) -> f64 {
    let h = 0.5 * x;
    let mut term = if nu + 1.0 < 170.0 && nu > -169.0 {
        h.powf(nu) * rgamma(nu + 1.0)
    } else {
        let mut sg = 1;
        let lg = unsafe { lgamma_r(nu + 1.0, &mut sg) };
        if nu + 1.0 <= 0.0 && is_integer(nu + 1.0) {
            0.0
        } else {
            sg as f64 * (nu * h.ln() - lg).exp()
        }
    };
    let mut sum = term;
    let q = sign * h * h;
    let mut k = 0.0;
    // Γ(k+ν+1) passe par un pôle (ν entier négatif) : on recalcule le terme.
    for _ in 0..500 {
        k += 1.0;
        let den = k * (k + nu);
        if den == 0.0 {
            term = h.powf(2.0 * k + nu) * rgamma(k + nu + 1.0) * sign.powf(k)
                / unsafe { tgamma(k + 1.0) };
        } else {
            term *= q / den;
        }
        sum += term;
        if term.abs() <= 1e-17 * sum.abs() && k > nu.abs() {
            break;
        }
    }
    sum
}

/// Développement de Hankel (DLMF 10.17.3/4) : (J_ν, Y_ν) pour x grand.
fn hankel(nu: f64, x: f64) -> (f64, f64) {
    let mu = 4.0 * nu * nu;
    let (mut p, mut q) = (1.0, 0.0);
    let mut a = 1.0;
    let mut last = f64::INFINITY;
    for k in 1..60 {
        let kf = k as f64;
        a *= (mu - (2.0 * kf - 1.0) * (2.0 * kf - 1.0)) / (kf * 8.0 * x);
        if a.abs() > last {
            break;
        }
        last = a.abs();
        match k % 4 {
            1 => q += a,
            2 => p -= a,
            3 => q -= a,
            _ => p += a,
        }
        if a.abs() < 1e-17 {
            break;
        }
    }
    let w = x - (0.5 * nu + 0.25) * PI;
    let (sw, cw) = w.sin_cos();
    let f = (2.0 / (PI * x)).sqrt();
    (f * (p * cw - q * sw), f * (p * sw + q * cw))
}

fn oscillation_panels(nu: f64, x: f64) -> usize {
    (((x + nu.abs()) * 0.5) as usize + 8).min(1_000_000)
}

/// ∫_0^∞ e^{g(t)} dt avec g concave : borne là où g a chuté de 45.
fn tail_integral(g: impl Fn(f64) -> f64) -> f64 {
    // Maximum de g sur [0, ∞) par recherche du point où g décroît.
    let mut t_max = 0.0;
    let mut g_max = g(0.0);
    let mut t = 0.0;
    let mut step = 0.125;
    loop {
        let v = g(t + step);
        if v >= g_max {
            t += step;
            g_max = v;
            t_max = t;
            step *= 2.0;
        } else if step > 1e-6 {
            step *= 0.5;
        } else {
            break;
        }
        if t > 1e4 {
            break;
        }
    }
    let mut end = t_max + 1.0;
    while g(end) > g_max - 45.0 && end < 1e4 {
        end = t_max + 2.0 * (end - t_max);
    }
    let panels = ((end / 0.125) as usize + 8).min(200_000);
    g_max.exp() * integrate(|t| (g(t) - g_max).exp(), 0.0, end, panels)
}

fn cyl_bessel_j(nu: f64, x: f64) -> f64 {
    if x < 0.0 {
        return domain_error();
    }
    if x == 0.0 {
        return if nu == 0.0 {
            1.0
        } else if nu > 0.0 || is_integer(nu) {
            0.0
        } else {
            f64::INFINITY
        };
    }
    if is_integer(nu) {
        return unsafe { jn(nu as i32, x) };
    }
    if x <= 6.0 || x * x <= 4.0 * (nu + 1.0) {
        return bessel_series(nu, x, -1.0);
    }
    if x > 1.0e5 && x > nu * nu {
        return hankel(nu, x).0;
    }
    // DLMF 10.9.6.
    let a = integrate(
        |t| (x * t.sin() - nu * t).cos(),
        0.0,
        PI,
        oscillation_panels(nu, x),
    ) / PI;
    let b = (nu * PI).sin() / PI * tail_integral(|t| -x * t.sinh() - nu * t);
    a - b
}

fn cyl_neumann(nu: f64, x: f64) -> f64 {
    if x < 0.0 {
        return domain_error();
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    if is_integer(nu) {
        return unsafe { yn(nu as i32, x) };
    }
    if x <= 6.0 {
        let (s, c) = (nu * PI).sin_cos();
        return (bessel_series(nu, x, -1.0) * c - bessel_series(-nu, x, -1.0)) / s;
    }
    if x > 1.0e5 && x > nu * nu {
        return hankel(nu, x).1;
    }
    // DLMF 10.9.7.
    let a = integrate(
        |t| (x * t.sin() - nu * t).sin(),
        0.0,
        PI,
        oscillation_panels(nu, x),
    ) / PI;
    let c = (nu * PI).cos();
    let b =
        tail_integral(|t| nu * t - x * t.sinh()) + c * tail_integral(|t| -nu * t - x * t.sinh());
    a - b / PI
}

fn cyl_bessel_i(nu: f64, x: f64) -> f64 {
    if x < 0.0 {
        return domain_error();
    }
    if x == 0.0 {
        return if nu == 0.0 {
            1.0
        } else if nu > 0.0 || is_integer(nu) {
            0.0
        } else {
            f64::INFINITY
        };
    }
    if x <= 30.0 || x * x <= 4.0 * (nu.abs() + 1.0) {
        return bessel_series(nu, x, 1.0);
    }
    if x > 709.0 {
        return f64::INFINITY;
    }
    // DLMF 10.32.4, à l'échelle e^-x.
    let panels = ((PI * 2.0 * x.sqrt()) as usize + (nu.abs() * 0.5) as usize + 8).min(1_000_000);
    let a = integrate(
        |t| (x * (t.cos() - 1.0)).exp() * (nu * t).cos(),
        0.0,
        PI,
        panels,
    ) / PI;
    let b = if is_integer(nu) {
        0.0
    } else {
        (nu * PI).sin() / PI * tail_integral(|t| -x * (t.cosh() + 1.0) - nu * t)
    };
    x.exp() * (a - b)
}

fn cyl_bessel_k(nu: f64, x: f64) -> f64 {
    if x < 0.0 {
        return domain_error();
    }
    if x == 0.0 {
        return f64::INFINITY;
    }
    let nu = nu.abs();
    // DLMF 10.32.9 : ∫_0^∞ e^{-x cosh t} cosh(νt) dt.
    0.5 * (tail_integral(|t| nu * t - x * t.cosh()) + tail_integral(|t| -nu * t - x * t.cosh()))
}

fn sph_bessel(n: u32, x: f64) -> f64 {
    if x < 0.0 {
        return domain_error();
    }
    if x == 0.0 {
        return if n == 0 { 1.0 } else { 0.0 };
    }
    (FRAC_PI_2 / x).sqrt() * cyl_bessel_j(n as f64 + 0.5, x)
}

fn sph_neumann(n: u32, x: f64) -> f64 {
    if x < 0.0 {
        return domain_error();
    }
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    (FRAC_PI_2 / x).sqrt() * cyl_neumann(n as f64 + 0.5, x)
}

// ---------- exports ----------

/// Un argument NaN est rendu tel quel (comportement de la MS STL).
macro_rules! nan_guard {
    ($($a:expr),*) => {
        $( if $a.is_nan() { return $a; } )*
    };
}

macro_rules! export_pair {
    ($d:ident, $f:ident, $core:ident, ($($a:ident),*)) => {
        extern "win64" fn $d($($a: f64),*) -> f64 {
            nan_guard!($($a),*);
            $core($($a),*)
        }
        extern "win64" fn $f($($a: f32),*) -> f32 {
            nan_guard!($($a),*);
            $core($($a as f64),*) as f32
        }
    };
}

macro_rules! export_pair_u {
    ($d:ident, $f:ident, $core:ident, ($($u:ident),*), $x:ident) => {
        extern "win64" fn $d($($u: u32,)* $x: f64) -> f64 {
            nan_guard!($x);
            $core($($u,)* $x)
        }
        extern "win64" fn $f($($u: u32,)* $x: f32) -> f32 {
            nan_guard!($x);
            $core($($u,)* $x as f64) as f32
        }
    };
}

export_pair_u!(
    smf_assoc_laguerre,
    smf_assoc_laguerref,
    assoc_laguerre,
    (n, m),
    x
);
export_pair_u!(
    smf_assoc_legendre,
    smf_assoc_legendref,
    assoc_legendre,
    (l, m),
    x
);
export_pair!(smf_beta, smf_betaf, beta, (x, y));
export_pair!(smf_comp_ellint_1, smf_comp_ellint_1f, comp_ellint_1, (k));
export_pair!(smf_comp_ellint_2, smf_comp_ellint_2f, comp_ellint_2, (k));
export_pair!(
    smf_comp_ellint_3,
    smf_comp_ellint_3f,
    comp_ellint_3,
    (k, nu)
);
export_pair!(smf_cyl_bessel_i, smf_cyl_bessel_if, cyl_bessel_i, (nu, x));
export_pair!(smf_cyl_bessel_j, smf_cyl_bessel_jf, cyl_bessel_j, (nu, x));
export_pair!(smf_cyl_bessel_k, smf_cyl_bessel_kf, cyl_bessel_k, (nu, x));
export_pair!(smf_cyl_neumann, smf_cyl_neumannf, cyl_neumann, (nu, x));
export_pair!(smf_ellint_1, smf_ellint_1f, ellint_1, (k, phi));
export_pair!(smf_ellint_2, smf_ellint_2f, ellint_2, (k, phi));
export_pair!(smf_ellint_3, smf_ellint_3f, ellint_3, (k, nu, phi));
export_pair!(smf_expint, smf_expintf, expint, (x));
export_pair_u!(smf_hermite, smf_hermitef, hermite, (n), x);
export_pair_u!(smf_laguerre, smf_laguerref, laguerre, (n), x);
export_pair_u!(smf_legendre, smf_legendref, legendre, (l), x);
export_pair!(smf_riemann_zeta, smf_riemann_zetaf, riemann_zeta, (x));
export_pair_u!(smf_sph_bessel, smf_sph_besself, sph_bessel, (n), x);
export_pair_u!(
    smf_sph_legendre,
    smf_sph_legendref,
    sph_legendre,
    (l, m),
    theta
);
export_pair_u!(smf_sph_neumann, smf_sph_neumannf, sph_neumann, (n), x);

extern "win64" fn smf_hypot3(x: f64, y: f64, z: f64) -> f64 {
    hypot3_f64(x, y, z)
}
extern "win64" fn smf_hypot3f(x: f32, y: f32, z: f32) -> f32 {
    hypot3_f32(x, y, z)
}

pub fn exports() -> Exports {
    crate::exports! {
        "__std_smf_assoc_laguerre" => fn smf_assoc_laguerre,
        "__std_smf_assoc_laguerref" => fn smf_assoc_laguerref,
        "__std_smf_assoc_legendre" => fn smf_assoc_legendre,
        "__std_smf_assoc_legendref" => fn smf_assoc_legendref,
        "__std_smf_beta" => fn smf_beta,
        "__std_smf_betaf" => fn smf_betaf,
        "__std_smf_comp_ellint_1" => fn smf_comp_ellint_1,
        "__std_smf_comp_ellint_1f" => fn smf_comp_ellint_1f,
        "__std_smf_comp_ellint_2" => fn smf_comp_ellint_2,
        "__std_smf_comp_ellint_2f" => fn smf_comp_ellint_2f,
        "__std_smf_comp_ellint_3" => fn smf_comp_ellint_3,
        "__std_smf_comp_ellint_3f" => fn smf_comp_ellint_3f,
        "__std_smf_cyl_bessel_i" => fn smf_cyl_bessel_i,
        "__std_smf_cyl_bessel_if" => fn smf_cyl_bessel_if,
        "__std_smf_cyl_bessel_j" => fn smf_cyl_bessel_j,
        "__std_smf_cyl_bessel_jf" => fn smf_cyl_bessel_jf,
        "__std_smf_cyl_bessel_k" => fn smf_cyl_bessel_k,
        "__std_smf_cyl_bessel_kf" => fn smf_cyl_bessel_kf,
        "__std_smf_cyl_neumann" => fn smf_cyl_neumann,
        "__std_smf_cyl_neumannf" => fn smf_cyl_neumannf,
        "__std_smf_ellint_1" => fn smf_ellint_1,
        "__std_smf_ellint_1f" => fn smf_ellint_1f,
        "__std_smf_ellint_2" => fn smf_ellint_2,
        "__std_smf_ellint_2f" => fn smf_ellint_2f,
        "__std_smf_ellint_3" => fn smf_ellint_3,
        "__std_smf_ellint_3f" => fn smf_ellint_3f,
        "__std_smf_expint" => fn smf_expint,
        "__std_smf_expintf" => fn smf_expintf,
        "__std_smf_hermite" => fn smf_hermite,
        "__std_smf_hermitef" => fn smf_hermitef,
        "__std_smf_laguerre" => fn smf_laguerre,
        "__std_smf_laguerref" => fn smf_laguerref,
        "__std_smf_legendre" => fn smf_legendre,
        "__std_smf_legendref" => fn smf_legendref,
        "__std_smf_riemann_zeta" => fn smf_riemann_zeta,
        "__std_smf_riemann_zetaf" => fn smf_riemann_zetaf,
        "__std_smf_sph_bessel" => fn smf_sph_bessel,
        "__std_smf_sph_besself" => fn smf_sph_besself,
        "__std_smf_sph_legendre" => fn smf_sph_legendre,
        "__std_smf_sph_legendref" => fn smf_sph_legendref,
        "__std_smf_sph_neumann" => fn smf_sph_neumann,
        "__std_smf_sph_neumannf" => fn smf_sph_neumannf,
        "__std_smf_hypot3" => fn smf_hypot3,
        "__std_smf_hypot3f" => fn smf_hypot3f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, rel: f64) -> bool {
        (a - b).abs() <= rel * b.abs().max(1e-300)
    }

    macro_rules! near {
        ($a:expr, $b:expr, $rel:expr) => {{
            let (a, b) = ($a, $b);
            assert!(close(a, b, $rel), "{} = {a}, attendu {b}", stringify!($a));
        }};
    }

    #[test]
    fn quarante_quatre_exports() {
        assert_eq!(exports().len(), 44);
    }

    #[test]
    fn hypot3_comme_la_ms_stl() {
        assert_eq!(smf_hypot3(2.0, 3.0, 6.0), 7.0);
        assert_eq!(smf_hypot3f(1.0, 2.0, 2.0), 3.0);
        assert_eq!(smf_hypot3f(-1e30, 0.0, 0.0), 1e30);
        assert_eq!(smf_hypot3(f64::NAN, f64::INFINITY, 0.0), f64::INFINITY);
        near!(smf_hypot3(1e200, 1e200, 1e200), 3f64.sqrt() * 1e200, 1e-15);
    }

    #[test]
    fn polynomes() {
        near!(legendre(3, 0.3), 0.5 * (5.0 * 0.027 - 3.0 * 0.3), 1e-14);
        // P_2^1(x) = 3x√(1-x²) sans phase de Condon-Shortley.
        near!(assoc_legendre(2, 1, 0.5), 1.5 * 0.75f64.sqrt(), 1e-14);
        assert_eq!(assoc_legendre(1, 2, 0.5), 0.0);
        near!(
            laguerre(3, 2.0),
            (-8.0 + 9.0 * 4.0 - 18.0 * 2.0 + 6.0) / 6.0,
            1e-14
        );
        // L_2^1(x) = (x² - 6x + 6)/2.
        near!(assoc_laguerre(2, 1, 1.5), (2.25 - 9.0 + 6.0) / 2.0, 1e-14);
        near!(
            hermite(4, 0.7),
            16.0 * 0.7f64.powi(4) - 48.0 * 0.49 + 12.0,
            1e-13
        );
        assert!(smf_legendre(2, 1.5).is_nan());
    }

    #[test]
    fn harmoniques_spheriques() {
        // Formule directe, l et m petits.
        let fact = |n: u32| (1..=n).map(f64::from).product::<f64>();
        for l in 0..8u32 {
            for m in 0..=l {
                let th: f64 = 0.83;
                let direct = if m % 2 == 1 { -1.0 } else { 1.0 }
                    * ((2 * l + 1) as f64 / (4.0 * PI) * fact(l - m) / fact(l + m)).sqrt()
                    * assoc_legendre(l, m, th.cos());
                near!(sph_legendre(l, m, th), direct, 1e-12);
            }
        }
    }

    #[test]
    fn beta_zeta_expint() {
        near!(beta(2.0, 3.0), 1.0 / 12.0, 1e-14);
        near!(beta(0.5, 0.5), PI, 1e-14);
        assert!(smf_beta(-1.0, 2.0).is_nan());
        near!(riemann_zeta(2.0), PI * PI / 6.0, 1e-14);
        near!(riemann_zeta(4.0), PI.powi(4) / 90.0, 1e-14);
        near!(riemann_zeta(0.5), -1.460_354_508_809_586_8, 1e-13);
        near!(riemann_zeta(-1.0), -1.0 / 12.0, 1e-13);
        assert_eq!(riemann_zeta(-2.0), 0.0);
        near!(riemann_zeta(1.0001), 10_000.577_222, 1e-8);
        near!(expint(1.0), 1.895_117_816_355_936_8, 1e-14);
        near!(expint(-1.0), -0.219_383_934_395_520_27, 1e-14);
        near!(expint(-3.5), -0.006_970_139_857_548_4, 1e-12);
        near!(expint(50.0), 1.058_563_689_713_169e20, 1e-13);
    }

    #[test]
    fn elliptiques() {
        // K(k) par la moyenne arithmético-géométrique, E(k) par quadrature.
        for &k in &[0.0f64, 0.3, 0.7, 0.95] {
            let (mut a, mut b) = (1.0f64, (1.0f64 - k * k).sqrt());
            for _ in 0..40 {
                (a, b) = ((a + b) / 2.0, (a * b).sqrt());
            }
            let kk = PI / (2.0 * a);
            near!(comp_ellint_1(k), kk, 1e-14);
            let e = integrate(
                |t| (1.0 - k * k * t.sin().powi(2)).sqrt(),
                0.0,
                FRAC_PI_2,
                64,
            );
            near!(comp_ellint_2(k), e, 1e-13);
            near!(comp_ellint_3(k, 0.0), kk, 1e-14);
        }
        // Formes incomplètes contre la quadrature directe.
        let (k, nu, phi) = (0.6, 0.4, 1.1);
        let q = |f: &dyn Fn(f64) -> f64| integrate(f, 0.0, phi, 64);
        near!(
            ellint_1(k, phi),
            q(&|t| 1.0 / (1.0 - k * k * t.sin().powi(2)).sqrt()),
            1e-13
        );
        near!(
            ellint_2(k, phi),
            q(&|t| (1.0 - k * k * t.sin().powi(2)).sqrt()),
            1e-13
        );
        near!(
            ellint_3(k, nu, phi),
            q(&|t| 1.0 / ((1.0 - nu * t.sin().powi(2)) * (1.0 - k * k * t.sin().powi(2)).sqrt())),
            1e-13
        );
        near!(
            ellint_3(k, -2.5, phi),
            q(&|t| 1.0 / ((1.0 + 2.5 * t.sin().powi(2)) * (1.0 - k * k * t.sin().powi(2)).sqrt())),
            1e-13
        );
        // Périodicité : F(φ + π) = F(φ) + 2K.
        near!(
            ellint_1(k, phi + PI),
            ellint_1(k, phi) + 2.0 * comp_ellint_1(k),
            1e-14
        );
        assert!(smf_comp_ellint_1(1.5).is_nan());
    }

    #[test]
    fn bessel() {
        // Ordre demi-entier : formes fermées.
        for &x in &[0.3f64, 2.0, 7.5, 25.0, 140.0] {
            let (s, c) = x.sin_cos();
            let f = (2.0 / (PI * x)).sqrt();
            near!(cyl_bessel_j(0.5, x), f * s, 1e-11);
            near!(cyl_bessel_j(-0.5, x), f * c, 1e-11);
            near!(cyl_neumann(0.5, x), -f * c, 1e-11);
            near!(
                cyl_bessel_i(0.5, x.min(30.0)),
                (2.0 / (PI * x.min(30.0))).sqrt() * x.min(30.0).sinh(),
                1e-12
            );
            near!(
                cyl_bessel_k(0.5, x),
                (PI / (2.0 * x)).sqrt() * (-x).exp(),
                1e-12
            );
            near!(sph_bessel(1, x), s / (x * x) - c / x, 1e-10);
            near!(sph_neumann(0, x), -c / x, 1e-10);
        }
        near!(
            cyl_bessel_i(0.5, 100.0),
            (2.0 / (PI * 100.0)).sqrt() * 100f64.sinh(),
            1e-12
        );
        // Ordre entier : libm ; I et K entiers par la série et l'intégrale.
        near!(cyl_bessel_j(1.0, 2.0), 0.576_724_807_756_873_4, 1e-14);
        near!(cyl_bessel_i(0.0, 1.0), 1.266_065_877_752_008_4, 1e-14);
        near!(cyl_bessel_i(1.0, 50.0), 2.903_078_590_103_557_3e20, 1e-12);
        near!(cyl_bessel_k(0.0, 1.0), 0.421_024_438_240_708_3, 1e-13);
        near!(cyl_bessel_k(1.0, 0.1), 9.853_844_780_870_606, 1e-12);
        // Relation de récurrence J_{ν-1} + J_{ν+1} = (2ν/x) J_ν, ν non entier.
        for &x in &[3.0, 9.0, 40.0] {
            let nu = 1.3;
            near!(
                cyl_bessel_j(nu - 1.0, x) + cyl_bessel_j(nu + 1.0, x),
                2.0 * nu / x * cyl_bessel_j(nu, x),
                1e-9
            );
            near!(
                cyl_neumann(nu - 1.0, x) + cyl_neumann(nu + 1.0, x),
                2.0 * nu / x * cyl_neumann(nu, x),
                1e-9
            );
        }
        // Hankel : x très grand.
        let x = 2.0e5;
        near!(
            cyl_bessel_j(0.5, x),
            (2.0 / (PI * x)).sqrt() * x.sin(),
            1e-9
        );
        assert!(smf_cyl_bessel_j(0.5, -1.0).is_nan());
    }
}
