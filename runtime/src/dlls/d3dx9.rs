//! d3dx9_24.dll … d3dx9_43.dll : bibliothèque d'aide de Direct3D 9 (#834,
//! suite de #237), écrite d'après la documentation publique du SDK.
//!
//! Fourni ici : les maths (matrices, vecteurs, quaternions, plans, volumes
//! englobants, demi-flottants), les textures et surfaces chargées depuis un
//! fichier, la mémoire ou une ressource (BMP, PNG, JPEG, TGA, DDS… par le
//! décodeur maison `imaging`), leur conversion de format, leurs niveaux de
//! mipmap et l'enregistrement BMP, ID3DXBuffer, et (dans `d3dx9_draw`) les
//! sprites et polices. La compilation HLSL et les effets ne sont pas encore
//! là : ces fonctions rendent une erreur claire.
//!
//! Les objets rendus au programme ont la table de méthodes de son ABI
//! (`legacy_com`) : un jeu 32 bits les appelle directement. Les appels vers
//! l'appareil Direct3D 9 passent par `com_call!`, quelle que soit sa table.

#![allow(clippy::too_many_arguments)]

use super::legacy_com::{put_ptr, rd32, vtable, wr32};
use crate::com_call;
use std::sync::atomic::{AtomicU32, Ordering};

pub(crate) const S_OK: i32 = 0;
pub(crate) const E_FAIL: i32 = 0x8000_4005u32 as i32;
pub(crate) const E_NOINTERFACE: i32 = 0x8000_4002u32 as i32;
pub(crate) const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;
pub(crate) const D3DERR_INVALIDCALL: i32 = 0x8876_086Cu32 as i32;
pub(crate) const D3DERR_NOTAVAILABLE: i32 = 0x8876_086Au32 as i32;
pub(crate) const D3DXERR_INVALIDDATA: i32 = 0x8876_0B59u32 as i32;

const D3DX_DEFAULT: u32 = 0xFFFF_FFFF;
const D3DX_DEFAULT_NONPOW2: u32 = 0xFFFF_FFFE;
const D3DX_FROM_FILE: u32 = 0xFFFF_FFFD;
const D3DFMT_FROM_FILE: u32 = 0xFFFF_FFFD;

// Formats D3DFMT utiles ici.
pub(crate) const FMT_R8G8B8: u32 = 20;
pub(crate) const FMT_A8R8G8B8: u32 = 21;
pub(crate) const FMT_X8R8G8B8: u32 = 22;
const FMT_R5G6B5: u32 = 23;
const FMT_X1R5G5B5: u32 = 24;
const FMT_A1R5G5B5: u32 = 25;
const FMT_A4R4G4B4: u32 = 26;
const FMT_A8: u32 = 28;
const FMT_X4R4G4B4: u32 = 30;
const FMT_A8B8G8R8: u32 = 32;
const FMT_X8B8G8R8: u32 = 33;
const FMT_L8: u32 = 50;
const FMT_A8L8: u32 = 51;
const FMT_A4L4: u32 = 52;
const FMT_A32B32G32R32F: u32 = 116;

fn fourcc(s: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*s)
}

fn is_dxt(f: u32) -> bool {
    [b"DXT1", b"DXT2", b"DXT3", b"DXT4", b"DXT5"]
        .iter()
        .any(|c| fourcc(c) == f)
}

// ═══════════════════════════════════════════════════════════════════════
// Maths
// ═══════════════════════════════════════════════════════════════════════

type M = [f32; 16];

pub(crate) const IDENTITY: M = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

pub(crate) fn mul(a: &M, b: &M) -> M {
    let mut r = [0f32; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = (0..4).map(|k| a[i * 4 + k] * b[k * 4 + j]).sum();
        }
    }
    r
}

fn transpose(a: &M) -> M {
    let mut r = [0f32; 16];
    for i in 0..4 {
        for j in 0..4 {
            r[i * 4 + j] = a[j * 4 + i];
        }
    }
    r
}

fn determinant(m: &M) -> f32 {
    let (a, b, c, d) = (m[0], m[1], m[2], m[3]);
    let s0 = m[10] * m[15] - m[11] * m[14];
    let s1 = m[9] * m[15] - m[11] * m[13];
    let s2 = m[9] * m[14] - m[10] * m[13];
    let s3 = m[8] * m[15] - m[11] * m[12];
    let s4 = m[8] * m[14] - m[10] * m[12];
    let s5 = m[8] * m[13] - m[9] * m[12];
    a * (m[5] * s0 - m[6] * s1 + m[7] * s2) - b * (m[4] * s0 - m[6] * s3 + m[7] * s4)
        + c * (m[4] * s1 - m[5] * s3 + m[7] * s5)
        - d * (m[4] * s2 - m[5] * s4 + m[6] * s5)
}

pub(crate) fn inverse(m: &M) -> Option<(M, f32)> {
    let mut inv = [0f32; 16];
    inv[0] = m[5] * m[10] * m[15] - m[5] * m[11] * m[14] - m[9] * m[6] * m[15]
        + m[9] * m[7] * m[14]
        + m[13] * m[6] * m[11]
        - m[13] * m[7] * m[10];
    inv[4] = -m[4] * m[10] * m[15] + m[4] * m[11] * m[14] + m[8] * m[6] * m[15]
        - m[8] * m[7] * m[14]
        - m[12] * m[6] * m[11]
        + m[12] * m[7] * m[10];
    inv[8] = m[4] * m[9] * m[15] - m[4] * m[11] * m[13] - m[8] * m[5] * m[15]
        + m[8] * m[7] * m[13]
        + m[12] * m[5] * m[11]
        - m[12] * m[7] * m[9];
    inv[12] = -m[4] * m[9] * m[14] + m[4] * m[10] * m[13] + m[8] * m[5] * m[14]
        - m[8] * m[6] * m[13]
        - m[12] * m[5] * m[10]
        + m[12] * m[6] * m[9];
    inv[1] = -m[1] * m[10] * m[15] + m[1] * m[11] * m[14] + m[9] * m[2] * m[15]
        - m[9] * m[3] * m[14]
        - m[13] * m[2] * m[11]
        + m[13] * m[3] * m[10];
    inv[5] = m[0] * m[10] * m[15] - m[0] * m[11] * m[14] - m[8] * m[2] * m[15]
        + m[8] * m[3] * m[14]
        + m[12] * m[2] * m[11]
        - m[12] * m[3] * m[10];
    inv[9] = -m[0] * m[9] * m[15] + m[0] * m[11] * m[13] + m[8] * m[1] * m[15]
        - m[8] * m[3] * m[13]
        - m[12] * m[1] * m[11]
        + m[12] * m[3] * m[9];
    inv[13] = m[0] * m[9] * m[14] - m[0] * m[10] * m[13] - m[8] * m[1] * m[14]
        + m[8] * m[2] * m[13]
        + m[12] * m[1] * m[10]
        - m[12] * m[2] * m[9];
    inv[2] = m[1] * m[6] * m[15] - m[1] * m[7] * m[14] - m[5] * m[2] * m[15]
        + m[5] * m[3] * m[14]
        + m[13] * m[2] * m[7]
        - m[13] * m[3] * m[6];
    inv[6] = -m[0] * m[6] * m[15] + m[0] * m[7] * m[14] + m[4] * m[2] * m[15]
        - m[4] * m[3] * m[14]
        - m[12] * m[2] * m[7]
        + m[12] * m[3] * m[6];
    inv[10] = m[0] * m[5] * m[15] - m[0] * m[7] * m[13] - m[4] * m[1] * m[15]
        + m[4] * m[3] * m[13]
        + m[12] * m[1] * m[7]
        - m[12] * m[3] * m[5];
    inv[14] = -m[0] * m[5] * m[14] + m[0] * m[6] * m[13] + m[4] * m[1] * m[14]
        - m[4] * m[2] * m[13]
        - m[12] * m[1] * m[6]
        + m[12] * m[2] * m[5];
    inv[3] = -m[1] * m[6] * m[11] + m[1] * m[7] * m[10] + m[5] * m[2] * m[11]
        - m[5] * m[3] * m[10]
        - m[9] * m[2] * m[7]
        + m[9] * m[3] * m[6];
    inv[7] = m[0] * m[6] * m[11] - m[0] * m[7] * m[10] - m[4] * m[2] * m[11]
        + m[4] * m[3] * m[10]
        + m[8] * m[2] * m[7]
        - m[8] * m[3] * m[6];
    inv[11] = -m[0] * m[5] * m[11] + m[0] * m[7] * m[9] + m[4] * m[1] * m[11]
        - m[4] * m[3] * m[9]
        - m[8] * m[1] * m[7]
        + m[8] * m[3] * m[5];
    inv[15] = m[0] * m[5] * m[10] - m[0] * m[6] * m[9] - m[4] * m[1] * m[10]
        + m[4] * m[2] * m[9]
        + m[8] * m[1] * m[6]
        - m[8] * m[2] * m[5];
    let det = m[0] * inv[0] + m[1] * inv[4] + m[2] * inv[8] + m[3] * inv[12];
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    for v in inv.iter_mut() {
        *v /= det;
    }
    Some((inv, det))
}

type V3 = [f32; 3];
fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn normalize3(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l == 0.0 {
        [0.0; 3]
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

fn translation(x: f32, y: f32, z: f32) -> M {
    let mut m = IDENTITY;
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}
fn scaling(x: f32, y: f32, z: f32) -> M {
    let mut m = IDENTITY;
    m[0] = x;
    m[5] = y;
    m[10] = z;
    m
}
fn rot_x(a: f32) -> M {
    let (s, c) = a.sin_cos();
    let mut m = IDENTITY;
    m[5] = c;
    m[6] = s;
    m[9] = -s;
    m[10] = c;
    m
}
fn rot_y(a: f32) -> M {
    let (s, c) = a.sin_cos();
    let mut m = IDENTITY;
    m[0] = c;
    m[2] = -s;
    m[8] = s;
    m[10] = c;
    m
}
pub(crate) fn rot_z(a: f32) -> M {
    let (s, c) = a.sin_cos();
    let mut m = IDENTITY;
    m[0] = c;
    m[1] = s;
    m[4] = -s;
    m[5] = c;
    m
}
fn rot_axis(v: V3, a: f32) -> M {
    let [x, y, z] = normalize3(v);
    let (s, c) = a.sin_cos();
    let t = 1.0 - c;
    let mut m = IDENTITY;
    m[0] = t * x * x + c;
    m[1] = t * x * y + s * z;
    m[2] = t * x * z - s * y;
    m[4] = t * x * y - s * z;
    m[5] = t * y * y + c;
    m[6] = t * y * z + s * x;
    m[8] = t * x * z + s * y;
    m[9] = t * y * z - s * x;
    m[10] = t * z * z + c;
    m
}
fn rot_quat(q: [f32; 4]) -> M {
    let [x, y, z, w] = q;
    let mut m = IDENTITY;
    m[0] = 1.0 - 2.0 * (y * y + z * z);
    m[1] = 2.0 * (x * y + z * w);
    m[2] = 2.0 * (x * z - y * w);
    m[4] = 2.0 * (x * y - z * w);
    m[5] = 1.0 - 2.0 * (x * x + z * z);
    m[6] = 2.0 * (y * z + x * w);
    m[8] = 2.0 * (x * z + y * w);
    m[9] = 2.0 * (y * z - x * w);
    m[10] = 1.0 - 2.0 * (x * x + y * y);
    m
}
fn quat_from_matrix(m: &M) -> [f32; 4] {
    let tr = m[0] + m[5] + m[10];
    if tr > 0.0 {
        let s = (tr + 1.0).sqrt() * 2.0;
        [
            (m[6] - m[9]) / s,
            (m[8] - m[2]) / s,
            (m[1] - m[4]) / s,
            0.25 * s,
        ]
    } else if m[0] > m[5] && m[0] > m[10] {
        let s = (1.0 + m[0] - m[5] - m[10]).sqrt() * 2.0;
        [
            0.25 * s,
            (m[1] + m[4]) / s,
            (m[8] + m[2]) / s,
            (m[6] - m[9]) / s,
        ]
    } else if m[5] > m[10] {
        let s = (1.0 + m[5] - m[0] - m[10]).sqrt() * 2.0;
        [
            (m[1] + m[4]) / s,
            0.25 * s,
            (m[6] + m[9]) / s,
            (m[8] - m[2]) / s,
        ]
    } else {
        let s = (1.0 + m[10] - m[0] - m[5]).sqrt() * 2.0;
        [
            (m[8] + m[2]) / s,
            (m[6] + m[9]) / s,
            0.25 * s,
            (m[1] - m[4]) / s,
        ]
    }
}

fn transform4(v: [f32; 4], m: &M) -> [f32; 4] {
    let mut r = [0f32; 4];
    for (j, o) in r.iter_mut().enumerate() {
        *o = v[0] * m[j] + v[1] * m[4 + j] + v[2] * m[8 + j] + v[3] * m[12 + j];
    }
    r
}

unsafe fn rm(p: *const f32) -> M {
    if p.is_null() {
        IDENTITY
    } else {
        std::ptr::read_unaligned(p as *const M)
    }
}
unsafe fn rv<const N: usize>(p: *const f32) -> [f32; N] {
    if p.is_null() {
        [0.0; N]
    } else {
        std::ptr::read_unaligned(p as *const [f32; N])
    }
}
unsafe fn wv<const N: usize>(p: *mut f32, v: [f32; N]) -> *mut f32 {
    if !p.is_null() {
        std::ptr::write_unaligned(p as *mut [f32; N], v);
    }
    p
}

unsafe extern "win64" fn matrix_multiply(out: *mut f32, a: *const f32, b: *const f32) -> *mut f32 {
    let m: M = mul(&rm(a), &rm(b));
    wv(out, m)
}
unsafe extern "win64" fn matrix_multiply_transpose(
    out: *mut f32,
    a: *const f32,
    b: *const f32,
) -> *mut f32 {
    let m: M = transpose(&mul(&rm(a), &rm(b)));
    wv(out, m)
}
unsafe extern "win64" fn matrix_transpose(out: *mut f32, a: *const f32) -> *mut f32 {
    let m: M = transpose(&rm(a));
    wv(out, m)
}
unsafe extern "win64" fn matrix_translation(out: *mut f32, x: f32, y: f32, z: f32) -> *mut f32 {
    let m: M = translation(x, y, z);
    wv(out, m)
}
unsafe extern "win64" fn matrix_scaling(out: *mut f32, x: f32, y: f32, z: f32) -> *mut f32 {
    let m: M = scaling(x, y, z);
    wv(out, m)
}
unsafe extern "win64" fn matrix_rotation_x(out: *mut f32, a: f32) -> *mut f32 {
    let m: M = rot_x(a);
    wv(out, m)
}
unsafe extern "win64" fn matrix_rotation_y(out: *mut f32, a: f32) -> *mut f32 {
    let m: M = rot_y(a);
    wv(out, m)
}
unsafe extern "win64" fn matrix_rotation_z(out: *mut f32, a: f32) -> *mut f32 {
    let m: M = rot_z(a);
    wv(out, m)
}
unsafe extern "win64" fn matrix_rotation_axis(out: *mut f32, v: *const f32, a: f32) -> *mut f32 {
    let m: M = rot_axis(rv::<3>(v), a);
    wv(out, m)
}
unsafe extern "win64" fn matrix_rotation_quaternion(out: *mut f32, q: *const f32) -> *mut f32 {
    let m: M = rot_quat(rv::<4>(q));
    wv(out, m)
}
unsafe extern "win64" fn matrix_rotation_ypr(
    out: *mut f32,
    yaw: f32,
    pitch: f32,
    roll: f32,
) -> *mut f32 {
    let m: M = mul(&mul(&rot_z(roll), &rot_x(pitch)), &rot_y(yaw));
    wv(out, m)
}

fn look_at(eye: V3, at: V3, up: V3, rh: bool) -> M {
    let z = normalize3(if rh { sub(eye, at) } else { sub(at, eye) });
    let x = normalize3(cross(up, z));
    let y = cross(z, x);
    [
        x[0],
        y[0],
        z[0],
        0.0,
        x[1],
        y[1],
        z[1],
        0.0,
        x[2],
        y[2],
        z[2],
        0.0,
        -dot(x, eye),
        -dot(y, eye),
        -dot(z, eye),
        1.0,
    ]
}
unsafe extern "win64" fn matrix_look_at_lh(
    out: *mut f32,
    e: *const f32,
    a: *const f32,
    u: *const f32,
) -> *mut f32 {
    let m: M = look_at(rv::<3>(e), rv::<3>(a), rv::<3>(u), false);
    wv(out, m)
}
unsafe extern "win64" fn matrix_look_at_rh(
    out: *mut f32,
    e: *const f32,
    a: *const f32,
    u: *const f32,
) -> *mut f32 {
    let m: M = look_at(rv::<3>(e), rv::<3>(a), rv::<3>(u), true);
    wv(out, m)
}

fn persp(xs: f32, ys: f32, zn: f32, zf: f32, rh: bool) -> M {
    let mut m = [0f32; 16];
    m[0] = xs;
    m[5] = ys;
    if rh {
        m[10] = zf / (zn - zf);
        m[11] = -1.0;
    } else {
        m[10] = zf / (zf - zn);
        m[11] = 1.0;
    }
    m[14] = zn * zf / (zn - zf);
    m
}
fn persp_fov(fovy: f32, aspect: f32, zn: f32, zf: f32, rh: bool) -> M {
    let ys = 1.0 / (fovy / 2.0).tan();
    persp(ys / aspect, ys, zn, zf, rh)
}
unsafe extern "win64" fn matrix_perspective_fov_lh(
    out: *mut f32,
    f: f32,
    a: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = persp_fov(f, a, n, z, false);
    wv(out, m)
}
unsafe extern "win64" fn matrix_perspective_fov_rh(
    out: *mut f32,
    f: f32,
    a: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = persp_fov(f, a, n, z, true);
    wv(out, m)
}
unsafe extern "win64" fn matrix_perspective_lh(
    out: *mut f32,
    w: f32,
    h: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = persp(2.0 * n / w, 2.0 * n / h, n, z, false);
    wv(out, m)
}
unsafe extern "win64" fn matrix_perspective_rh(
    out: *mut f32,
    w: f32,
    h: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = persp(2.0 * n / w, 2.0 * n / h, n, z, true);
    wv(out, m)
}

fn persp_off(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32, rh: bool) -> M {
    let mut m = persp(2.0 * zn / (r - l), 2.0 * zn / (t - b), zn, zf, rh);
    if rh {
        m[8] = (l + r) / (r - l);
        m[9] = (t + b) / (t - b);
    } else {
        m[8] = (l + r) / (l - r);
        m[9] = (t + b) / (b - t);
    }
    m
}
unsafe extern "win64" fn matrix_perspective_off_center_lh(
    out: *mut f32,
    l: f32,
    r: f32,
    b: f32,
    t: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = persp_off(l, r, b, t, n, z, false);
    wv(out, m)
}
unsafe extern "win64" fn matrix_perspective_off_center_rh(
    out: *mut f32,
    l: f32,
    r: f32,
    b: f32,
    t: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = persp_off(l, r, b, t, n, z, true);
    wv(out, m)
}

pub(crate) fn ortho_off(l: f32, r: f32, b: f32, t: f32, zn: f32, zf: f32, rh: bool) -> M {
    let mut m = IDENTITY;
    m[0] = 2.0 / (r - l);
    m[5] = 2.0 / (t - b);
    m[10] = if rh { 1.0 / (zn - zf) } else { 1.0 / (zf - zn) };
    m[12] = (l + r) / (l - r);
    m[13] = (t + b) / (b - t);
    m[14] = zn / (zn - zf);
    m
}
unsafe extern "win64" fn matrix_ortho_lh(
    out: *mut f32,
    w: f32,
    h: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = ortho_off(-w / 2.0, w / 2.0, -h / 2.0, h / 2.0, n, z, false);
    wv(out, m)
}
unsafe extern "win64" fn matrix_ortho_rh(
    out: *mut f32,
    w: f32,
    h: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = ortho_off(-w / 2.0, w / 2.0, -h / 2.0, h / 2.0, n, z, true);
    wv(out, m)
}
unsafe extern "win64" fn matrix_ortho_off_center_lh(
    out: *mut f32,
    l: f32,
    r: f32,
    b: f32,
    t: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = ortho_off(l, r, b, t, n, z, false);
    wv(out, m)
}
unsafe extern "win64" fn matrix_ortho_off_center_rh(
    out: *mut f32,
    l: f32,
    r: f32,
    b: f32,
    t: f32,
    n: f32,
    z: f32,
) -> *mut f32 {
    let m: M = ortho_off(l, r, b, t, n, z, true);
    wv(out, m)
}

fn plane_normalized(p: [f32; 4]) -> [f32; 4] {
    let l = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt();
    if l == 0.0 {
        [0.0; 4]
    } else {
        [p[0] / l, p[1] / l, p[2] / l, p[3] / l]
    }
}
unsafe extern "win64" fn matrix_reflect(out: *mut f32, plane: *const f32) -> *mut f32 {
    let m: M = {
        let p = plane_normalized(rv::<4>(plane));
        let mut m = IDENTITY;
        for i in 0..3 {
            for j in 0..3 {
                m[i * 4 + j] = if i == j { 1.0 } else { 0.0 } - 2.0 * p[i] * p[j];
            }
            m[12 + i] = -2.0 * p[3] * p[i];
        }
        m
    };
    wv(out, m)
}
unsafe extern "win64" fn matrix_shadow(
    out: *mut f32,
    light: *const f32,
    plane: *const f32,
) -> *mut f32 {
    let m: M = {
        let p = plane_normalized(rv::<4>(plane));
        let l = rv::<4>(light);
        let d = p[0] * l[0] + p[1] * l[1] + p[2] * l[2] + p[3] * l[3];
        let mut m = [0f32; 16];
        for i in 0..4 {
            for j in 0..4 {
                m[i * 4 + j] = if i == j { d } else { 0.0 } - p[i] * l[j];
            }
        }
        m
    };
    wv(out, m)
}
unsafe extern "win64" fn matrix_affine_transformation_2d(
    out: *mut f32,
    s: f32,
    rc: *const f32,
    rot: f32,
    t: *const f32,
) -> *mut f32 {
    let m: M = {
        let c = rv::<2>(rc);
        let tr = rv::<2>(t);
        let mut m = scaling(s, s, 1.0);
        m = mul(&m, &translation(-c[0], -c[1], 0.0));
        m = mul(&m, &rot_z(rot));
        m = mul(&m, &translation(c[0] + tr[0], c[1] + tr[1], 0.0));
        m
    };
    wv(out, m)
}
unsafe extern "win64" fn matrix_affine_transformation(
    out: *mut f32,
    s: f32,
    rc: *const f32,
    rot: *const f32,
    t: *const f32,
) -> *mut f32 {
    let m: M = {
        let c = rv::<3>(rc);
        let tr = rv::<3>(t);
        let mut m = scaling(s, s, s);
        m = mul(&m, &translation(-c[0], -c[1], -c[2]));
        if !rot.is_null() {
            m = mul(&m, &rot_quat(rv::<4>(rot)));
        }
        m = mul(&m, &translation(c[0] + tr[0], c[1] + tr[1], c[2] + tr[2]));
        m
    };
    wv(out, m)
}
unsafe extern "win64" fn matrix_transformation_2d(
    out: *mut f32,
    sc: *const f32,
    srot: f32,
    s: *const f32,
    rc: *const f32,
    rot: f32,
    t: *const f32,
) -> *mut f32 {
    let m: M = {
        let scen = rv::<2>(sc);
        let sv = if s.is_null() { [1.0, 1.0] } else { rv::<2>(s) };
        let c = rv::<2>(rc);
        let tr = rv::<2>(t);
        let mut m = translation(-scen[0], -scen[1], 0.0);
        m = mul(&m, &rot_z(-srot));
        m = mul(&m, &scaling(sv[0], sv[1], 1.0));
        m = mul(&m, &rot_z(srot));
        m = mul(&m, &translation(scen[0] - c[0], scen[1] - c[1], 0.0));
        m = mul(&m, &rot_z(rot));
        m = mul(&m, &translation(c[0] + tr[0], c[1] + tr[1], 0.0));
        m
    };
    wv(out, m)
}

unsafe extern "win64" fn matrix_inverse(out: *mut f32, det: *mut f32, m: *const f32) -> *mut f32 {
    match inverse(&rm(m)) {
        Some((inv, d)) => {
            if !det.is_null() {
                det.write_unaligned(d);
            }
            wv(out, inv)
        }
        None => std::ptr::null_mut(),
    }
}
unsafe extern "win64" fn matrix_determinant(m: *const f32) -> f32 {
    determinant(&rm(m))
}
unsafe extern "win64" fn matrix_decompose(
    s: *mut f32,
    r: *mut f32,
    t: *mut f32,
    m: *const f32,
) -> i32 {
    let m = rm(m);
    let row = |i: usize| [m[i * 4], m[i * 4 + 1], m[i * 4 + 2]];
    let sc = [
        dot(row(0), row(0)).sqrt(),
        dot(row(1), row(1)).sqrt(),
        dot(row(2), row(2)).sqrt(),
    ];
    wv(s, sc);
    wv(t, [m[12], m[13], m[14]]);
    if sc.contains(&0.0) {
        return D3DERR_INVALIDCALL;
    }
    let mut n = IDENTITY;
    for i in 0..3 {
        for j in 0..3 {
            n[i * 4 + j] = m[i * 4 + j] / sc[i];
        }
    }
    wv(r, quat_from_matrix(&n));
    S_OK
}

// --- Vecteurs ---

unsafe extern "win64" fn vec2_normalize(out: *mut f32, v: *const f32) -> *mut f32 {
    let v = rv::<2>(v);
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    wv(
        out,
        if l == 0.0 {
            [0.0; 2]
        } else {
            [v[0] / l, v[1] / l]
        },
    )
}
unsafe extern "win64" fn vec3_normalize(out: *mut f32, v: *const f32) -> *mut f32 {
    wv(out, normalize3(rv::<3>(v)))
}
unsafe extern "win64" fn vec4_normalize(out: *mut f32, v: *const f32) -> *mut f32 {
    let v = rv::<4>(v);
    let l = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    wv(out, if l == 0.0 { [0.0; 4] } else { v.map(|x| x / l) })
}
unsafe extern "win64" fn vec2_transform(out: *mut f32, v: *const f32, m: *const f32) -> *mut f32 {
    let v = rv::<2>(v);
    wv(out, transform4([v[0], v[1], 0.0, 1.0], &rm(m)))
}
unsafe extern "win64" fn vec2_transform_coord(
    out: *mut f32,
    v: *const f32,
    m: *const f32,
) -> *mut f32 {
    let v = rv::<2>(v);
    let r = transform4([v[0], v[1], 0.0, 1.0], &rm(m));
    let w = if r[3] == 0.0 { 1.0 } else { r[3] };
    wv(out, [r[0] / w, r[1] / w])
}
unsafe extern "win64" fn vec2_transform_normal(
    out: *mut f32,
    v: *const f32,
    m: *const f32,
) -> *mut f32 {
    let v = rv::<2>(v);
    let r = transform4([v[0], v[1], 0.0, 0.0], &rm(m));
    wv(out, [r[0], r[1]])
}
unsafe extern "win64" fn vec3_transform(out: *mut f32, v: *const f32, m: *const f32) -> *mut f32 {
    let v = rv::<3>(v);
    wv(out, transform4([v[0], v[1], v[2], 1.0], &rm(m)))
}
fn coord(v: V3, m: &M) -> V3 {
    let r = transform4([v[0], v[1], v[2], 1.0], m);
    let w = if r[3] == 0.0 { 1.0 } else { r[3] };
    [r[0] / w, r[1] / w, r[2] / w]
}
unsafe extern "win64" fn vec3_transform_coord(
    out: *mut f32,
    v: *const f32,
    m: *const f32,
) -> *mut f32 {
    wv(out, coord(rv::<3>(v), &rm(m)))
}
unsafe extern "win64" fn vec3_transform_normal(
    out: *mut f32,
    v: *const f32,
    m: *const f32,
) -> *mut f32 {
    let v = rv::<3>(v);
    let r = transform4([v[0], v[1], v[2], 0.0], &rm(m));
    wv(out, [r[0], r[1], r[2]])
}
unsafe fn transform_array(
    out: *mut f32,
    os: u32,
    v: *const f32,
    vs: u32,
    m: *const f32,
    n: u32,
    f: fn(V3, &M) -> Vec<f32>,
) -> *mut f32 {
    let m = rm(m);
    for i in 0..n as usize {
        let r = f(rv::<3>((v as usize + i * vs as usize) as *const f32), &m);
        std::ptr::copy_nonoverlapping(
            r.as_ptr(),
            (out as usize + i * os as usize) as *mut f32,
            r.len(),
        );
    }
    out
}
unsafe extern "win64" fn vec3_transform_coord_array(
    out: *mut f32,
    os: u32,
    v: *const f32,
    vs: u32,
    m: *const f32,
    n: u32,
) -> *mut f32 {
    transform_array(out, os, v, vs, m, n, |v, m| coord(v, m).to_vec())
}
unsafe extern "win64" fn vec3_transform_normal_array(
    out: *mut f32,
    os: u32,
    v: *const f32,
    vs: u32,
    m: *const f32,
    n: u32,
) -> *mut f32 {
    transform_array(out, os, v, vs, m, n, |v, m| {
        transform4([v[0], v[1], v[2], 0.0], m)[..3].to_vec()
    })
}
unsafe extern "win64" fn vec3_transform_array(
    out: *mut f32,
    os: u32,
    v: *const f32,
    vs: u32,
    m: *const f32,
    n: u32,
) -> *mut f32 {
    transform_array(out, os, v, vs, m, n, |v, m| {
        transform4([v[0], v[1], v[2], 1.0], m).to_vec()
    })
}
unsafe extern "win64" fn vec4_transform(out: *mut f32, v: *const f32, m: *const f32) -> *mut f32 {
    wv(out, transform4(rv::<4>(v), &rm(m)))
}
unsafe extern "win64" fn vec4_cross(
    out: *mut f32,
    u: *const f32,
    v: *const f32,
    w: *const f32,
) -> *mut f32 {
    let (u, v, w) = (rv::<4>(u), rv::<4>(v), rv::<4>(w));
    let a = v[0] * w[1] - v[1] * w[0];
    let b = v[0] * w[2] - v[2] * w[0];
    let c = v[0] * w[3] - v[3] * w[0];
    let d = v[1] * w[2] - v[2] * w[1];
    let e = v[1] * w[3] - v[3] * w[1];
    let f = v[2] * w[3] - v[3] * w[2];
    wv(
        out,
        [
            f * u[1] - e * u[2] + d * u[3],
            -(f * u[0] - c * u[2] + b * u[3]),
            e * u[0] - c * u[1] + a * u[3],
            -(d * u[0] - b * u[1] + a * u[2]),
        ],
    )
}
unsafe extern "win64" fn vec3_barycentric(
    out: *mut f32,
    a: *const f32,
    b: *const f32,
    c: *const f32,
    f: f32,
    g: f32,
) -> *mut f32 {
    let (a, b, c) = (rv::<3>(a), rv::<3>(b), rv::<3>(c));
    let mut r = [0f32; 3];
    for i in 0..3 {
        r[i] = a[i] + f * (b[i] - a[i]) + g * (c[i] - a[i]);
    }
    wv(out, r)
}

/// D3DVIEWPORT9 { X, Y, Width, Height : DWORD ; MinZ, MaxZ : float }.
unsafe fn viewport(p: usize) -> [f32; 6] {
    if p == 0 {
        return [0.0, 0.0, 1.0, 1.0, 0.0, 1.0];
    }
    [
        rd32(p) as f32,
        rd32(p + 4) as f32,
        rd32(p + 8) as f32,
        rd32(p + 12) as f32,
        f32::from_bits(rd32(p + 16)),
        f32::from_bits(rd32(p + 20)),
    ]
}
unsafe extern "win64" fn vec3_project(
    out: *mut f32,
    v: *const f32,
    vp: usize,
    proj: *const f32,
    view: *const f32,
    world: *const f32,
) -> *mut f32 {
    let m = mul(&mul(&rm(world), &rm(view)), &rm(proj));
    let p = coord(rv::<3>(v), &m);
    let vp = viewport(vp);
    wv(
        out,
        [
            vp[0] + vp[2] * (1.0 + p[0]) / 2.0,
            vp[1] + vp[3] * (1.0 - p[1]) / 2.0,
            vp[4] + p[2] * (vp[5] - vp[4]),
        ],
    )
}
unsafe extern "win64" fn vec3_unproject(
    out: *mut f32,
    v: *const f32,
    vp: usize,
    proj: *const f32,
    view: *const f32,
    world: *const f32,
) -> *mut f32 {
    let m = mul(&mul(&rm(world), &rm(view)), &rm(proj));
    let Some((inv, _)) = inverse(&m) else {
        return std::ptr::null_mut();
    };
    let vp = viewport(vp);
    let p = rv::<3>(v);
    let n = [
        2.0 * (p[0] - vp[0]) / vp[2] - 1.0,
        1.0 - 2.0 * (p[1] - vp[1]) / vp[3],
        (p[2] - vp[4]) / (vp[5] - vp[4]),
    ];
    wv(out, coord(n, &inv))
}

// --- Quaternions et plans ---

fn qmul(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    // D3DXQuaternionMultiply(q1, q2) = rotation q1 puis q2 = q2·q1.
    let (q1, q2) = (a, b);
    [
        q2[3] * q1[0] + q2[0] * q1[3] + q2[1] * q1[2] - q2[2] * q1[1],
        q2[3] * q1[1] - q2[0] * q1[2] + q2[1] * q1[3] + q2[2] * q1[0],
        q2[3] * q1[2] + q2[0] * q1[1] - q2[1] * q1[0] + q2[2] * q1[3],
        q2[3] * q1[3] - q2[0] * q1[0] - q2[1] * q1[1] - q2[2] * q1[2],
    ]
}
unsafe extern "win64" fn quat_multiply(out: *mut f32, a: *const f32, b: *const f32) -> *mut f32 {
    wv(out, qmul(rv::<4>(a), rv::<4>(b)))
}
unsafe extern "win64" fn quat_rotation_axis(out: *mut f32, v: *const f32, a: f32) -> *mut f32 {
    let v = normalize3(rv::<3>(v));
    let (s, c) = (a / 2.0).sin_cos();
    wv(out, [v[0] * s, v[1] * s, v[2] * s, c])
}
unsafe extern "win64" fn quat_rotation_matrix(out: *mut f32, m: *const f32) -> *mut f32 {
    wv(out, quat_from_matrix(&rm(m)))
}
unsafe extern "win64" fn quat_rotation_ypr(out: *mut f32, y: f32, p: f32, r: f32) -> *mut f32 {
    let (sy, cy) = (y / 2.0).sin_cos();
    let (sp, cp) = (p / 2.0).sin_cos();
    let (sr, cr) = (r / 2.0).sin_cos();
    wv(
        out,
        [
            cy * sp * cr + sy * cp * sr,
            sy * cp * cr - cy * sp * sr,
            cy * cp * sr - sy * sp * cr,
            cy * cp * cr + sy * sp * sr,
        ],
    )
}
unsafe extern "win64" fn quat_normalize(out: *mut f32, q: *const f32) -> *mut f32 {
    vec4_normalize(out, q)
}
unsafe extern "win64" fn quat_inverse(out: *mut f32, q: *const f32) -> *mut f32 {
    let q = rv::<4>(q);
    let n = q.iter().map(|x| x * x).sum::<f32>();
    if n == 0.0 {
        return wv(out, [0.0; 4]);
    }
    wv(out, [-q[0] / n, -q[1] / n, -q[2] / n, q[3] / n])
}
unsafe extern "win64" fn quat_slerp(
    out: *mut f32,
    a: *const f32,
    b: *const f32,
    t: f32,
) -> *mut f32 {
    let (a, mut b) = (rv::<4>(a), rv::<4>(b));
    let mut d: f32 = (0..4).map(|i| a[i] * b[i]).sum();
    if d < 0.0 {
        b = b.map(|x| -x);
        d = -d;
    }
    let (ka, kb) = if 1.0 - d > 1e-4 {
        let th = d.min(1.0).acos();
        let s = th.sin();
        (((1.0 - t) * th).sin() / s, (t * th).sin() / s)
    } else {
        (1.0 - t, t)
    };
    let mut r = [0f32; 4];
    for i in 0..4 {
        r[i] = ka * a[i] + kb * b[i];
    }
    wv(out, r)
}
unsafe extern "win64" fn quat_to_axis_angle(q: *const f32, axis: *mut f32, angle: *mut f32) {
    let q = rv::<4>(q);
    wv(axis, [q[0], q[1], q[2]]);
    if !angle.is_null() {
        angle.write_unaligned(2.0 * q[3].clamp(-1.0, 1.0).acos());
    }
}
unsafe extern "win64" fn plane_normalize(out: *mut f32, p: *const f32) -> *mut f32 {
    wv(out, plane_normalized(rv::<4>(p)))
}
unsafe extern "win64" fn plane_from_point_normal(
    out: *mut f32,
    p: *const f32,
    n: *const f32,
) -> *mut f32 {
    let (p, n) = (rv::<3>(p), rv::<3>(n));
    wv(out, [n[0], n[1], n[2], -dot(p, n)])
}
unsafe extern "win64" fn plane_from_points(
    out: *mut f32,
    a: *const f32,
    b: *const f32,
    c: *const f32,
) -> *mut f32 {
    let (a, b, c) = (rv::<3>(a), rv::<3>(b), rv::<3>(c));
    let n = normalize3(cross(sub(b, a), sub(c, a)));
    wv(out, [n[0], n[1], n[2], -dot(n, a)])
}
unsafe extern "win64" fn plane_intersect_line(
    out: *mut f32,
    p: *const f32,
    a: *const f32,
    b: *const f32,
) -> *mut f32 {
    let (p, a, b) = (rv::<4>(p), rv::<3>(a), rv::<3>(b));
    let n = [p[0], p[1], p[2]];
    let d = sub(b, a);
    let den = dot(n, d);
    if den == 0.0 {
        return std::ptr::null_mut();
    }
    let t = -(dot(n, a) + p[3]) / den;
    wv(out, [a[0] + t * d[0], a[1] + t * d[1], a[2] + t * d[2]])
}
unsafe extern "win64" fn plane_transform(out: *mut f32, p: *const f32, m: *const f32) -> *mut f32 {
    wv(out, transform4(rv::<4>(p), &rm(m)))
}

// --- Volumes englobants et lancers de rayons ---

unsafe fn positions(first: usize, n: u32, stride: u32) -> Vec<V3> {
    (0..n as usize)
        .map(|i| rv::<3>((first + i * stride as usize) as *const f32))
        .collect()
}
unsafe extern "win64" fn compute_bounding_sphere(
    first: usize,
    n: u32,
    stride: u32,
    center: *mut f32,
    radius: *mut f32,
) -> i32 {
    if first == 0 || center.is_null() || radius.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let p = positions(first, n, stride);
    let mut c = [0f32; 3];
    for v in &p {
        for i in 0..3 {
            c[i] += v[i];
        }
    }
    if n > 0 {
        c = c.map(|x| x / n as f32);
    }
    let r = p
        .iter()
        .map(|v| dot(sub(*v, c), sub(*v, c)).sqrt())
        .fold(0f32, f32::max);
    wv(center, c);
    radius.write_unaligned(r);
    S_OK
}
unsafe extern "win64" fn compute_bounding_box(
    first: usize,
    n: u32,
    stride: u32,
    min: *mut f32,
    max: *mut f32,
) -> i32 {
    if first == 0 || min.is_null() || max.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let p = positions(first, n, stride);
    let mut lo = [f32::MAX; 3];
    let mut hi = [f32::MIN; 3];
    for v in &p {
        for i in 0..3 {
            lo[i] = lo[i].min(v[i]);
            hi[i] = hi[i].max(v[i]);
        }
    }
    if p.is_empty() {
        lo = [0.0; 3];
        hi = [0.0; 3];
    }
    wv(min, lo);
    wv(max, hi);
    S_OK
}
unsafe extern "win64" fn intersect_tri(
    p0: *const f32,
    p1: *const f32,
    p2: *const f32,
    pos: *const f32,
    dir: *const f32,
    pu: *mut f32,
    pv: *mut f32,
    pd: *mut f32,
) -> i32 {
    let (a, b, c, o, d) = (
        rv::<3>(p0),
        rv::<3>(p1),
        rv::<3>(p2),
        rv::<3>(pos),
        rv::<3>(dir),
    );
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-12 {
        return 0;
    }
    let t = sub(o, a);
    let u = dot(t, p) / det;
    if !(0.0..=1.0).contains(&u) {
        return 0;
    }
    let q = cross(t, e1);
    let v = dot(d, q) / det;
    if v < 0.0 || u + v > 1.0 {
        return 0;
    }
    let dist = dot(e2, q) / det;
    if dist < 0.0 {
        return 0;
    }
    for (o, x) in [(pu, u), (pv, v), (pd, dist)] {
        if !o.is_null() {
            o.write_unaligned(x);
        }
    }
    1
}
unsafe extern "win64" fn sphere_bound_probe(
    center: *const f32,
    radius: f32,
    pos: *const f32,
    dir: *const f32,
) -> i32 {
    let (c, o, d) = (rv::<3>(center), rv::<3>(pos), rv::<3>(dir));
    let m = sub(o, c);
    let a = dot(d, d);
    let b = 2.0 * dot(m, d);
    let cc = dot(m, m) - radius * radius;
    let disc = b * b - 4.0 * a * cc;
    if disc < 0.0 {
        return 0;
    }
    // Sphère devant l'origine du rayon (ou autour d'elle).
    i32::from((-b + disc.sqrt()) >= 0.0)
}
unsafe extern "win64" fn box_bound_probe(
    min: *const f32,
    max: *const f32,
    pos: *const f32,
    dir: *const f32,
) -> i32 {
    let (lo, hi, o, d) = (rv::<3>(min), rv::<3>(max), rv::<3>(pos), rv::<3>(dir));
    let (mut t0, mut t1) = (f32::MIN, f32::MAX);
    for i in 0..3 {
        if d[i].abs() < 1e-12 {
            if o[i] < lo[i] || o[i] > hi[i] {
                return 0;
            }
            continue;
        }
        let a = (lo[i] - o[i]) / d[i];
        let b = (hi[i] - o[i]) / d[i];
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    i32::from(t0 <= t1 && t1 >= 0.0)
}

// --- Demi-flottants ---

pub(crate) fn f32_to_f16(f: f32) -> u16 {
    let b = f.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xFF) as i32;
    let man = b & 0x7F_FFFF;
    if exp == 0xFF {
        return sign | 0x7C00 | if man != 0 { 0x200 } else { 0 };
    }
    let e = exp - 127 + 15;
    if e >= 0x1F {
        return sign | 0x7BFF; // D3DX sature au plus grand nombre fini
    }
    if e <= 0 {
        if e < -10 {
            return sign;
        }
        let m = (man | 0x80_0000) >> (1 - e);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    let h = ((e as u32) << 10) | (man >> 13);
    let round = (man >> 12) & 1;
    sign | (h + round).min(0x7BFF) as u16
}
pub(crate) fn f16_to_f32(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1F) as u32;
    let man = (h & 0x3FF) as u32;
    let bits = if exp == 0 {
        if man == 0 {
            sign
        } else {
            let mut e = 127 - 15 + 1;
            let mut m = man;
            while m & 0x400 == 0 {
                m <<= 1;
                e -= 1;
            }
            sign | (e << 23) | ((m & 0x3FF) << 13)
        }
    } else if exp == 0x1F {
        sign | 0x7F80_0000 | (man << 13)
    } else {
        sign | ((exp + 127 - 15) << 23) | (man << 13)
    };
    f32::from_bits(bits)
}
unsafe extern "win64" fn float32_to_16_array(out: *mut u16, src: *const f32, n: u32) -> *mut u16 {
    for i in 0..n as usize {
        out.add(i)
            .write_unaligned(f32_to_f16(src.add(i).read_unaligned()));
    }
    out
}
unsafe extern "win64" fn float16_to_32_array(out: *mut f32, src: *const u16, n: u32) -> *mut f32 {
    for i in 0..n as usize {
        out.add(i)
            .write_unaligned(f16_to_f32(src.add(i).read_unaligned()));
    }
    out
}

// ═══════════════════════════════════════════════════════════════════════
// ID3DXBuffer
// ═══════════════════════════════════════════════════════════════════════

#[repr(C)]
struct Buffer {
    vtbl: usize,
    refs: AtomicU32,
    data: Vec<u8>,
}

unsafe extern "win64" fn buf_qi(this: usize, _iid: usize, out: usize) -> i32 {
    if out == 0 {
        return E_NOINTERFACE;
    }
    buf_addref(this);
    put_ptr(out, this);
    S_OK
}
unsafe extern "win64" fn buf_addref(this: usize) -> u32 {
    (*(this as *const Buffer))
        .refs
        .fetch_add(1, Ordering::Relaxed)
        + 1
}
unsafe extern "win64" fn buf_release(this: usize) -> u32 {
    let n = (*(this as *const Buffer))
        .refs
        .fetch_sub(1, Ordering::AcqRel)
        - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut Buffer));
    }
    n
}
unsafe extern "win64" fn buf_pointer(this: usize) -> usize {
    (*(this as *const Buffer)).data.as_ptr() as usize
}
unsafe extern "win64" fn buf_size(this: usize) -> u32 {
    (*(this as *const Buffer)).data.len() as u32
}

/// Nouvel ID3DXBuffer contenant `data`.
pub(crate) fn new_buffer(data: Vec<u8>) -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    let vt = vtable(
        "ID3DXBuffer",
        &[
            a(buf_qi as *const (), 2),
            a(buf_addref as *const (), 0),
            a(buf_release as *const (), 0),
            a(buf_pointer as *const (), 0),
            a(buf_size as *const (), 0),
        ],
    );
    Box::into_raw(Box::new(Buffer {
        vtbl: vt,
        refs: AtomicU32::new(1),
        data,
    })) as usize
}

/// Tampon d'erreurs texte (terminé par 0) rendu à `out` s'il est demandé.
pub(crate) unsafe fn error_buffer(out: usize, msg: &str) {
    if out != 0 {
        let mut v = msg.as_bytes().to_vec();
        v.push(0);
        put_ptr(out, new_buffer(v));
    }
}

unsafe extern "win64" fn create_buffer(size: u32, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    put_ptr(out, new_buffer(vec![0; size as usize]));
    S_OK
}

// ═══════════════════════════════════════════════════════════════════════
// Pixels : conversion entre RGBA 8 bits et les formats D3DFMT
// ═══════════════════════════════════════════════════════════════════════

/// Octets par pixel d'un format non compressé géré (0 : non géré).
pub(crate) fn bpp(f: u32) -> usize {
    match f {
        FMT_A8R8G8B8 | FMT_X8R8G8B8 | FMT_A8B8G8R8 | FMT_X8B8G8R8 => 4,
        FMT_R8G8B8 => 3,
        FMT_R5G6B5 | FMT_X1R5G5B5 | FMT_A1R5G5B5 | FMT_A4R4G4B4 | FMT_X4R4G4B4 | FMT_A8L8 => 2,
        FMT_A8 | FMT_L8 | FMT_A4L4 => 1,
        FMT_A32B32G32R32F => 16,
        _ => 0,
    }
}

fn has_alpha(f: u32) -> bool {
    matches!(
        f,
        FMT_A8R8G8B8 | FMT_A8B8G8R8 | FMT_A1R5G5B5 | FMT_A4R4G4B4 | FMT_A8 | FMT_A8L8 | FMT_A4L4
    ) || f == FMT_A32B32G32R32F
        || is_dxt(f)
}

fn q(v: u8, bits: u32) -> u32 {
    (v as u32 * ((1 << bits) - 1) + 127) / 255
}
fn ex(v: u32, bits: u32) -> u8 {
    let max = (1u32 << bits) - 1;
    ((v & max) * 255 / max) as u8
}
fn luma(c: [u8; 4]) -> u8 {
    ((c[0] as u32 * 77 + c[1] as u32 * 150 + c[2] as u32 * 29) >> 8) as u8
}

/// Écrit le pixel RGBA `c` au format `f` en `p`.
pub(crate) unsafe fn encode(f: u32, c: [u8; 4], p: *mut u8) {
    let [r, g, b, a] = c;
    let w16 = |v: u32| (p as *mut u16).write_unaligned(v as u16);
    match f {
        FMT_A8R8G8B8 | FMT_X8R8G8B8 => {
            let a = if f == FMT_X8R8G8B8 { 255 } else { a };
            (p as *mut u32).write_unaligned(u32::from_le_bytes([b, g, r, a]))
        }
        FMT_A8B8G8R8 | FMT_X8B8G8R8 => {
            let a = if f == FMT_X8B8G8R8 { 255 } else { a };
            (p as *mut u32).write_unaligned(u32::from_le_bytes([r, g, b, a]))
        }
        FMT_R8G8B8 => {
            *p = b;
            *p.add(1) = g;
            *p.add(2) = r;
        }
        FMT_R5G6B5 => w16(q(r, 5) << 11 | q(g, 6) << 5 | q(b, 5)),
        FMT_X1R5G5B5 => w16(0x8000 | q(r, 5) << 10 | q(g, 5) << 5 | q(b, 5)),
        FMT_A1R5G5B5 => w16(u32::from(a >= 128) << 15 | q(r, 5) << 10 | q(g, 5) << 5 | q(b, 5)),
        FMT_A4R4G4B4 => w16(q(a, 4) << 12 | q(r, 4) << 8 | q(g, 4) << 4 | q(b, 4)),
        FMT_X4R4G4B4 => w16(0xF000 | q(r, 4) << 8 | q(g, 4) << 4 | q(b, 4)),
        FMT_A8 => *p = a,
        FMT_L8 => *p = luma(c),
        FMT_A8L8 => w16((a as u32) << 8 | luma(c) as u32),
        FMT_A4L4 => *p = (q(a, 4) << 4 | q(luma(c), 4)) as u8,
        FMT_A32B32G32R32F => {
            for (i, v) in [r, g, b, a].iter().enumerate() {
                (p as *mut f32).add(i).write_unaligned(*v as f32 / 255.0);
            }
        }
        _ => {}
    }
}

/// Lit un pixel au format `f` en RGBA.
pub(crate) unsafe fn decode(f: u32, p: *const u8) -> [u8; 4] {
    let r16 = || (p as *const u16).read_unaligned() as u32;
    match f {
        FMT_A8R8G8B8 | FMT_X8R8G8B8 => {
            let [b, g, r, a] = (p as *const u32).read_unaligned().to_le_bytes();
            [r, g, b, if f == FMT_X8R8G8B8 { 255 } else { a }]
        }
        FMT_A8B8G8R8 | FMT_X8B8G8R8 => {
            let [r, g, b, a] = (p as *const u32).read_unaligned().to_le_bytes();
            [r, g, b, if f == FMT_X8B8G8R8 { 255 } else { a }]
        }
        FMT_R8G8B8 => [*p.add(2), *p.add(1), *p, 255],
        FMT_R5G6B5 => {
            let v = r16();
            [ex(v >> 11, 5), ex(v >> 5, 6), ex(v, 5), 255]
        }
        FMT_X1R5G5B5 | FMT_A1R5G5B5 => {
            let v = r16();
            let a = if f == FMT_X1R5G5B5 || v & 0x8000 != 0 {
                255
            } else {
                0
            };
            [ex(v >> 10, 5), ex(v >> 5, 5), ex(v, 5), a]
        }
        FMT_A4R4G4B4 | FMT_X4R4G4B4 => {
            let v = r16();
            let a = if f == FMT_X4R4G4B4 {
                255
            } else {
                ex(v >> 12, 4)
            };
            [ex(v >> 8, 4), ex(v >> 4, 4), ex(v, 4), a]
        }
        FMT_A8 => [0, 0, 0, *p],
        FMT_L8 => [*p, *p, *p, 255],
        FMT_A8L8 => {
            let v = r16();
            let l = v as u8;
            [l, l, l, (v >> 8) as u8]
        }
        FMT_A4L4 => {
            let l = ex(*p as u32, 4);
            [l, l, l, ex((*p >> 4) as u32, 4)]
        }
        FMT_A32B32G32R32F => {
            let g = |i: usize| {
                ((p as *const f32).add(i).read_unaligned().clamp(0.0, 1.0) * 255.0 + 0.5) as u8
            };
            [g(0), g(1), g(2), g(3)]
        }
        _ => [0, 0, 0, 255],
    }
}

/// Image RGBA 8 bits en mémoire (alpha non prémultiplié).
#[derive(Clone)]
pub(crate) struct Rgba {
    pub w: u32,
    pub h: u32,
    pub px: Vec<u8>,
}

impl Rgba {
    pub fn get(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.w as usize + x as usize) * 4;
        [self.px[i], self.px[i + 1], self.px[i + 2], self.px[i + 3]]
    }
    /// Sous-rectangle [x0, y0, x1, y1) mis à l'échelle w × h : moyenne des
    /// pixels couverts en réduction, plus proche voisin en agrandissement
    /// (D3DX_FILTER_BOX / POINT), ou bilinéaire si `linear`.
    pub fn resample(&self, r: [u32; 4], w: u32, h: u32, linear: bool) -> Rgba {
        let (sw, sh) = ((r[2] - r[0]).max(1), (r[3] - r[1]).max(1));
        let mut out = Rgba {
            w,
            h,
            px: vec![0; w as usize * h as usize * 4],
        };
        if w == 0 || h == 0 || self.w == 0 || self.h == 0 {
            return out;
        }
        for y in 0..h {
            for x in 0..w {
                let c = if sw > w || sh > h || !linear {
                    let x0 = r[0] + x * sw / w;
                    let x1 = (r[0] + ((x + 1) * sw).div_ceil(w)).max(x0 + 1).min(self.w);
                    let y0 = r[1] + y * sh / h;
                    let y1 = (r[1] + ((y + 1) * sh).div_ceil(h)).max(y0 + 1).min(self.h);
                    let (x0, y0) = (x0.min(self.w - 1), y0.min(self.h - 1));
                    let mut acc = [0u32; 4];
                    let mut n = 0u32;
                    for yy in y0..y1.max(y0 + 1) {
                        for xx in x0..x1.max(x0 + 1) {
                            let p = self.get(xx.min(self.w - 1), yy.min(self.h - 1));
                            // Moyenne pondérée par l'alpha : pas de franges sombres.
                            let a = p[3] as u32;
                            acc[0] += p[0] as u32 * a;
                            acc[1] += p[1] as u32 * a;
                            acc[2] += p[2] as u32 * a;
                            acc[3] += a;
                            n += 1;
                        }
                    }
                    match std::num::NonZeroU32::new(acc[3]) {
                        None => [0, 0, 0, 0],
                        Some(a) => [
                            (acc[0] / a) as u8,
                            (acc[1] / a) as u8,
                            (acc[2] / a) as u8,
                            (a.get() / n) as u8,
                        ],
                    }
                } else {
                    let fx = r[0] as f32 + (x as f32 + 0.5) * sw as f32 / w as f32 - 0.5;
                    let fy = r[1] as f32 + (y as f32 + 0.5) * sh as f32 / h as f32 - 0.5;
                    let x0 = fx.floor().max(0.0) as u32;
                    let y0 = fy.floor().max(0.0) as u32;
                    let (x0, y0) = (x0.min(self.w - 1), y0.min(self.h - 1));
                    let x1 = (x0 + 1).min(r[2].max(1) - 1).min(self.w - 1);
                    let y1 = (y0 + 1).min(r[3].max(1) - 1).min(self.h - 1);
                    let tx = (fx - x0 as f32).clamp(0.0, 1.0);
                    let ty = (fy - y0 as f32).clamp(0.0, 1.0);
                    let (a, b, c, d) = (
                        self.get(x0, y0),
                        self.get(x1, y0),
                        self.get(x0, y1),
                        self.get(x1, y1),
                    );
                    let mut o = [0u8; 4];
                    for i in 0..4 {
                        let top = a[i] as f32 * (1.0 - tx) + b[i] as f32 * tx;
                        let bot = c[i] as f32 * (1.0 - tx) + d[i] as f32 * tx;
                        o[i] = (top * (1.0 - ty) + bot * ty + 0.5) as u8;
                    }
                    o
                };
                let i = (y as usize * w as usize + x as usize) * 4;
                out.px[i..i + 4].copy_from_slice(&c);
            }
        }
        out
    }
    /// D3DX : les pixels de la couleur clé (ARGB) deviennent noir transparent.
    pub fn apply_color_key(&mut self, key: u32) {
        if key == 0 {
            return;
        }
        let [kb, kg, kr, ka] = key.to_le_bytes();
        for p in self.px.as_chunks_mut::<4>().0 {
            if *p == [kr, kg, kb, ka] {
                *p = [0; 4];
            }
        }
    }
}

/// D3DX_FILTER : POINT (2) garde le plus proche voisin, LINEAR (3) est
/// bilinéaire, les autres (NONE, BOX, TRIANGLE, défaut) font une moyenne.
fn linear_filter(filter: u32) -> bool {
    filter & 0xFF == 3
}

/// Copie `img` (déjà à la taille de la cible) dans une zone verrouillée.
pub(crate) unsafe fn store(img: &Rgba, fmt: u32, bits: usize, pitch: i32) {
    let bp = bpp(fmt);
    if bp == 0 || bits == 0 {
        return;
    }
    for y in 0..img.h {
        let row = (bits as isize + y as isize * pitch as isize) as *mut u8;
        for x in 0..img.w {
            encode(fmt, img.get(x, y), row.add(x as usize * bp));
        }
    }
}

/// Lit une zone verrouillée en RGBA.
pub(crate) unsafe fn load(fmt: u32, bits: usize, pitch: i32, w: u32, h: u32) -> Option<Rgba> {
    let bp = bpp(fmt);
    if bp == 0 || bits == 0 {
        return None;
    }
    let mut px = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        let row = (bits as isize + y as isize * pitch as isize) as *const u8;
        for x in 0..w {
            px.extend_from_slice(&decode(fmt, row.add(x as usize * bp)));
        }
    }
    Some(Rgba { w, h, px })
}

// ═══════════════════════════════════════════════════════════════════════
// Fichiers d'image
// ═══════════════════════════════════════════════════════════════════════

/// D3DXIMAGE_FILEFORMAT.
fn file_format(d: &[u8]) -> u32 {
    if d.starts_with(b"BM") {
        0
    } else if d.starts_with(&[0xFF, 0xD8]) {
        1
    } else if d.starts_with(b"\x89PNG") {
        3
    } else if d.starts_with(b"DDS ") {
        4
    } else if d.starts_with(b"P") && d.len() > 1 && (b'1'..=b'7').contains(&d[1]) {
        5
    } else {
        // TGA (sans signature) ou format reconnu par le seul décodeur.
        2
    }
}

/// Image d'un fichier DDS gardée telle quelle (blocs DXT et niveaux).
struct RawDds {
    fmt: u32,
    w: u32,
    h: u32,
    mips: u32,
    data: Vec<u8>,
}

fn raw_dds(d: &[u8]) -> Option<RawDds> {
    if d.len() < 128 || &d[..4] != b"DDS " {
        return None;
    }
    let u = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
    let (h, w) = (u(12), u(16));
    let mips = if u(8) & 0x20000 != 0 { u(28).max(1) } else { 1 };
    let pf_flags = u(80);
    let fcc = u(84);
    // Cube ou volume : pas de lecture brute (le décodeur maison s'en charge).
    if u(112) & 0x200 != 0 || u(8) & 0x80_0000 != 0 {
        return None;
    }
    if pf_flags & 0x4 == 0 || !is_dxt(fcc) || w == 0 || h == 0 {
        return None;
    }
    Some(RawDds {
        fmt: fcc,
        w,
        h,
        mips,
        data: d[128..].to_vec(),
    })
}

fn dxt_level_size(fmt: u32, w: u32, h: u32) -> (usize, usize) {
    let block = if fmt == fourcc(b"DXT1") { 8 } else { 16 };
    let bw = w.div_ceil(4).max(1) as usize;
    let bh = h.div_ceil(4).max(1) as usize;
    (bw * block, bw * bh * block)
}

struct Decoded {
    img: Rgba,
    /// Format source annoncé dans D3DXIMAGE_INFO.
    fmt: u32,
    ff: u32,
    mips: u32,
}

fn decode_image(d: &[u8], hint: &str) -> Option<Decoded> {
    let ff = file_format(d);
    let ext = if hint.is_empty() {
        if ff == 2 {
            "tga"
        } else {
            ""
        }
    } else {
        hint
    };
    let img = crate::imaging::decode_bytes(d, ext)
        .or_else(|_| crate::imaging::decode_bytes(d, "tga"))
        .ok()?;
    let page = img.pages.into_iter().next()?;
    let r = page.into_raster();
    let alpha = r.px.as_chunks::<4>().0.iter().any(|p| p[3] != 255);
    let (fmt, mips) = match raw_dds(d) {
        Some(raw) => (raw.fmt, raw.mips),
        None => (
            match ff {
                1 | 5 => FMT_R8G8B8,
                0 if !alpha => FMT_R8G8B8,
                _ if alpha => FMT_A8R8G8B8,
                _ => FMT_X8R8G8B8,
            },
            1,
        ),
    };
    Some(Decoded {
        img: Rgba {
            w: r.w,
            h: r.h,
            px: r.px,
        },
        fmt,
        ff,
        mips,
    })
}

unsafe fn write_info(out: usize, w: u32, h: u32, mips: u32, fmt: u32, ff: u32) {
    if out == 0 {
        return;
    }
    for (i, v) in [w, h, 1, mips, fmt, 3, ff].iter().enumerate() {
        wr32(out + 4 * i, *v);
    }
}

unsafe fn read_path_a(p: *const u8) -> Option<std::path::PathBuf> {
    (!p.is_null()).then(|| crate::paths::to_linux(&super::cstr(p)))
}
unsafe fn read_path_w(p: *const u16) -> Option<std::path::PathBuf> {
    (!p.is_null()).then(|| crate::paths::to_linux(&super::wstr(p)))
}
fn ext_of(p: &std::path::Path) -> String {
    p.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}
fn read_file(p: &std::path::Path) -> Option<Vec<u8>> {
    let d = std::fs::read(p);
    if d.is_err() {
        crate::log_warn!("d3dx9", "fichier introuvable : {}", p.display());
    }
    d.ok()
}

/// Données d'une ressource du module : RT_RCDATA, sinon RT_BITMAP (à qui
/// l'on redonne l'en-tête de fichier BMP).
unsafe fn resource_bytes(module: usize, name: usize, wide: bool) -> Option<Vec<u8>> {
    use super::resource::*;
    for ty in [10usize, 2] {
        let h = if wide {
            find_resource_w(module, name as *const u16, ty as *const u16)
        } else {
            find_resource_a(module, name as *const u8, ty as *const u8)
        };
        if h == 0 {
            continue;
        }
        let size = sizeof_resource(module, h) as usize;
        let p = lock_resource(load_resource(module, h));
        if p == 0 || size == 0 {
            continue;
        }
        let d = std::slice::from_raw_parts(p as *const u8, size).to_vec();
        if ty == 2 {
            let hs = u32::from_le_bytes([d[0], d[1], d[2], d[3]]) as usize;
            let bits = u16::from_le_bytes([d[14], d[15]]) as usize;
            let used = u32::from_le_bytes([d[32], d[33], d[34], d[35]]) as usize;
            let pal = if bits <= 8 {
                if used == 0 {
                    1 << bits
                } else {
                    used
                }
            } else {
                0
            };
            let off = 14 + hs + pal * 4;
            let mut f = Vec::with_capacity(14 + d.len());
            f.extend_from_slice(b"BM");
            f.extend_from_slice(&((14 + d.len()) as u32).to_le_bytes());
            f.extend_from_slice(&[0; 4]);
            f.extend_from_slice(&(off as u32).to_le_bytes());
            f.extend_from_slice(&d);
            return Some(f);
        }
        return Some(d);
    }
    None
}

// ═══════════════════════════════════════════════════════════════════════
// Textures
// ═══════════════════════════════════════════════════════════════════════

fn pow2(v: u32) -> u32 {
    v.max(1).next_power_of_two()
}

fn mip_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// Format retenu pour une texture : D3DX remplace les formats qu'il ne sait
/// pas produire par le plus proche qu'il sait écrire.
fn target_format(req: u32, src: u32, color_key: u32) -> u32 {
    let want = if req == 0 || req == D3DFMT_FROM_FILE {
        src
    } else {
        req
    };
    if want == src && is_dxt(src) {
        return src;
    }
    if bpp(want) != 0 && want != FMT_R8G8B8 {
        return want;
    }
    if has_alpha(want) || color_key != 0 || want == FMT_R8G8B8 && has_alpha(src) {
        FMT_A8R8G8B8
    } else {
        FMT_X8R8G8B8
    }
}

/// (pas, adresse) d'un niveau verrouillé de texture.
unsafe fn lock_level(tex: usize, level: u32, flags: u32) -> Option<(i32, usize)> {
    let mut lr = [0u64; 2];
    let hr = com_call!(
        tex,
        19,
        fn(u32, *mut u64, usize, u32) -> i32,
        level,
        lr.as_mut_ptr(),
        0,
        flags
    );
    (hr >= 0).then(|| super::legacy_com::locked_rect(lr.as_ptr() as usize))
}
unsafe fn unlock_level(tex: usize, level: u32) {
    com_call!(tex, 20, fn(u32) -> i32, level);
}
/// D3DSURFACE_DESC { Format, Type, Usage, Pool, MultiSampleType,
/// MultiSampleQuality, Width, Height }.
unsafe fn level_desc(tex: usize, level: u32) -> Option<[u32; 8]> {
    let mut d = [0u32; 8];
    (com_call!(tex, 17, fn(u32, *mut u32) -> i32, level, d.as_mut_ptr()) >= 0).then_some(d)
}

/// Remplit les niveaux `from+1..` d'une texture à partir du niveau `from`.
unsafe fn fill_mips(tex: usize, from: u32, filter: u32) -> i32 {
    let levels = com_call!(tex, 13, fn() -> u32);
    let Some(d) = level_desc(tex, from) else {
        return D3DERR_INVALIDCALL;
    };
    if bpp(d[0]) == 0 {
        return S_OK;
    }
    let Some((pitch, bits)) = lock_level(tex, from, 0x10) else {
        return D3DERR_INVALIDCALL;
    };
    let src = load(d[0], bits, pitch, d[6], d[7]);
    unlock_level(tex, from);
    let Some(src) = src else {
        return D3DERR_INVALIDCALL;
    };
    for l in from + 1..levels {
        let Some(ld) = level_desc(tex, l) else {
            break;
        };
        let img = src.resample([0, 0, src.w, src.h], ld[6], ld[7], linear_filter(filter));
        if let Some((p, b)) = lock_level(tex, l, 0) {
            store(&img, ld[0], b, p);
            unlock_level(tex, l);
        }
    }
    S_OK
}

unsafe fn create_texture_raw(
    dev: usize,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
) -> Result<usize, i32> {
    let mut t: usize = 0;
    let hr = com_call!(
        dev,
        23,
        fn(u32, u32, u32, u32, u32, u32, *mut usize, usize) -> i32,
        w,
        h,
        mips,
        usage,
        fmt,
        pool,
        &mut t,
        0
    );
    if hr < 0 || t == 0 {
        return Err(if hr < 0 { hr } else { E_FAIL });
    }
    Ok(t)
}

/// D3DXCreateTextureFromFileInMemoryEx et toutes ses variantes.
unsafe fn texture_from_bytes(
    dev: usize,
    d: &[u8],
    hint: &str,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    out: usize,
) -> i32 {
    if dev == 0 || out == 0 || d.is_empty() {
        return D3DERR_INVALIDCALL;
    }
    put_ptr(out, 0);
    // DDS compressé demandé tel quel : blocs recopiés sans décodage.
    if let Some(raw) = raw_dds(d) {
        let keep = (fmt == 0 || fmt == D3DFMT_FROM_FILE || fmt == raw.fmt)
            && [0, D3DX_DEFAULT, D3DX_DEFAULT_NONPOW2, D3DX_FROM_FILE, raw.w].contains(&w)
            && [0, D3DX_DEFAULT, D3DX_DEFAULT_NONPOW2, D3DX_FROM_FILE, raw.h].contains(&h)
            && key == 0;
        if keep {
            let levels = match mips {
                0 | D3DX_DEFAULT | D3DX_FROM_FILE => raw.mips,
                n => n.min(raw.mips),
            };
            let t = match create_texture_raw(dev, raw.w, raw.h, levels, usage, raw.fmt, pool) {
                Ok(t) => t,
                Err(e) => return e,
            };
            let mut off = 0usize;
            let (mut lw, mut lh) = (raw.w, raw.h);
            for l in 0..levels {
                let (row, size) = dxt_level_size(raw.fmt, lw, lh);
                if off + size > raw.data.len() {
                    break;
                }
                if let Some((pitch, bits)) = lock_level(t, l, 0) {
                    for r in 0..size / row {
                        std::ptr::copy_nonoverlapping(
                            raw.data.as_ptr().add(off + r * row),
                            (bits as isize + r as isize * pitch as isize) as *mut u8,
                            row,
                        );
                    }
                    unlock_level(t, l);
                }
                off += size;
                lw = (lw / 2).max(1);
                lh = (lh / 2).max(1);
            }
            write_info(info, raw.w, raw.h, raw.mips, raw.fmt, 4);
            put_ptr(out, t);
            return S_OK;
        }
    }
    let Some(mut src) = decode_image(d, hint) else {
        crate::log_warn!("d3dx9", "image illisible ({} octets)", d.len());
        return D3DXERR_INVALIDDATA;
    };
    write_info(info, src.img.w, src.img.h, src.mips, src.fmt, src.ff);
    src.img.apply_color_key(key);
    let pick = |req: u32, n: u32| match req {
        0 | D3DX_DEFAULT => pow2(n),
        D3DX_DEFAULT_NONPOW2 | D3DX_FROM_FILE => n,
        v => v,
    };
    let tw = pick(w, src.img.w).min(8192);
    let th = pick(h, src.img.h).min(8192);
    let levels = match mips {
        0 | D3DX_DEFAULT => mip_count(tw, th),
        D3DX_FROM_FILE => 1,
        n => n.min(mip_count(tw, th)),
    };
    let tf = target_format(fmt, src.fmt, key);
    let t = match create_texture_raw(dev, tw, th, levels, usage, tf, pool) {
        Ok(t) => t,
        Err(e) => return e,
    };
    let real = level_desc(t, 0).map_or(tf, |d| d[0]);
    let img = if (tw, th) == (src.img.w, src.img.h) {
        src.img
    } else {
        src.img
            .resample([0, 0, src.img.w, src.img.h], tw, th, linear_filter(filter))
    };
    if let Some((pitch, bits)) = lock_level(t, 0, 0) {
        store(&img, real, bits, pitch);
        unlock_level(t, 0);
    }
    if levels > 1 && mip_filter != 1 {
        fill_mips(t, 0, mip_filter);
    }
    put_ptr(out, t);
    S_OK
}

const DEF: u32 = D3DX_DEFAULT;

unsafe extern "win64" fn create_texture_from_file_in_memory_ex(
    dev: usize,
    data: *const u8,
    size: u32,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    _pal: usize,
    out: usize,
) -> i32 {
    if data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = std::slice::from_raw_parts(data, size as usize);
    texture_from_bytes(
        dev, d, "", w, h, mips, usage, fmt, pool, filter, mip_filter, key, info, out,
    )
}
unsafe extern "win64" fn create_texture_from_file_in_memory(
    dev: usize,
    data: *const u8,
    size: u32,
    out: usize,
) -> i32 {
    create_texture_from_file_in_memory_ex(
        dev, data, size, DEF, DEF, DEF, 0, 0, 1, DEF, DEF, 0, 0, 0, out,
    )
}
unsafe fn from_path(
    dev: usize,
    p: Option<std::path::PathBuf>,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    out: usize,
) -> i32 {
    let Some(p) = p else {
        return D3DERR_INVALIDCALL;
    };
    let Some(d) = read_file(&p) else {
        return D3DERR_NOTAVAILABLE;
    };
    texture_from_bytes(
        dev,
        &d,
        &ext_of(&p),
        w,
        h,
        mips,
        usage,
        fmt,
        pool,
        filter,
        mip_filter,
        key,
        info,
        out,
    )
}
unsafe extern "win64" fn create_texture_from_file_ex_a(
    dev: usize,
    path: *const u8,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    _pal: usize,
    out: usize,
) -> i32 {
    from_path(
        dev,
        read_path_a(path),
        w,
        h,
        mips,
        usage,
        fmt,
        pool,
        filter,
        mip_filter,
        key,
        info,
        out,
    )
}
unsafe extern "win64" fn create_texture_from_file_ex_w(
    dev: usize,
    path: *const u16,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    _pal: usize,
    out: usize,
) -> i32 {
    from_path(
        dev,
        read_path_w(path),
        w,
        h,
        mips,
        usage,
        fmt,
        pool,
        filter,
        mip_filter,
        key,
        info,
        out,
    )
}
unsafe extern "win64" fn create_texture_from_file_a(
    dev: usize,
    path: *const u8,
    out: usize,
) -> i32 {
    from_path(
        dev,
        read_path_a(path),
        DEF,
        DEF,
        DEF,
        0,
        0,
        1,
        DEF,
        DEF,
        0,
        0,
        out,
    )
}
unsafe extern "win64" fn create_texture_from_file_w(
    dev: usize,
    path: *const u16,
    out: usize,
) -> i32 {
    from_path(
        dev,
        read_path_w(path),
        DEF,
        DEF,
        DEF,
        0,
        0,
        1,
        DEF,
        DEF,
        0,
        0,
        out,
    )
}
unsafe fn from_resource(
    dev: usize,
    module: usize,
    name: usize,
    wide: bool,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    out: usize,
) -> i32 {
    let Some(d) = resource_bytes(module, name, wide) else {
        return D3DXERR_INVALIDDATA;
    };
    texture_from_bytes(
        dev, &d, "", w, h, mips, usage, fmt, pool, filter, mip_filter, key, info, out,
    )
}
unsafe extern "win64" fn create_texture_from_resource_a(
    dev: usize,
    module: usize,
    name: usize,
    out: usize,
) -> i32 {
    from_resource(
        dev, module, name, false, DEF, DEF, DEF, 0, 0, 1, DEF, DEF, 0, 0, out,
    )
}
unsafe extern "win64" fn create_texture_from_resource_w(
    dev: usize,
    module: usize,
    name: usize,
    out: usize,
) -> i32 {
    from_resource(
        dev, module, name, true, DEF, DEF, DEF, 0, 0, 1, DEF, DEF, 0, 0, out,
    )
}
unsafe extern "win64" fn create_texture_from_resource_ex_a(
    dev: usize,
    module: usize,
    name: usize,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    _pal: usize,
    out: usize,
) -> i32 {
    from_resource(
        dev, module, name, false, w, h, mips, usage, fmt, pool, filter, mip_filter, key, info, out,
    )
}
unsafe extern "win64" fn create_texture_from_resource_ex_w(
    dev: usize,
    module: usize,
    name: usize,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    filter: u32,
    mip_filter: u32,
    key: u32,
    info: usize,
    _pal: usize,
    out: usize,
) -> i32 {
    from_resource(
        dev, module, name, true, w, h, mips, usage, fmt, pool, filter, mip_filter, key, info, out,
    )
}

/// D3DXCheckTextureRequirements : ajuste taille, niveaux et format.
unsafe extern "win64" fn check_texture_requirements(
    _dev: usize,
    w: *mut u32,
    h: *mut u32,
    mips: *mut u32,
    _usage: u32,
    fmt: *mut u32,
    _pool: u32,
) -> i32 {
    let rw = if w.is_null() { 256 } else { w.read_unaligned() };
    let rh = if h.is_null() { 256 } else { h.read_unaligned() };
    let fix = |v: u32| match v {
        0 | D3DX_DEFAULT => 256,
        v => v.min(8192),
    };
    let (tw, th) = (fix(rw), fix(rh));
    if !w.is_null() {
        w.write_unaligned(tw);
    }
    if !h.is_null() {
        h.write_unaligned(th);
    }
    if !mips.is_null() {
        let m = mips.read_unaligned();
        let full = mip_count(tw, th);
        mips.write_unaligned(if m == 0 || m == D3DX_DEFAULT {
            full
        } else {
            m.min(full)
        });
    }
    if !fmt.is_null() {
        let f = fmt.read_unaligned();
        let ok = is_dxt(f) || (bpp(f) != 0 && f != FMT_R8G8B8);
        fmt.write_unaligned(if ok {
            f
        } else if has_alpha(f) {
            FMT_A8R8G8B8
        } else {
            FMT_X8R8G8B8
        });
    }
    S_OK
}

unsafe extern "win64" fn create_texture(
    dev: usize,
    w: u32,
    h: u32,
    mips: u32,
    usage: u32,
    fmt: u32,
    pool: u32,
    out: usize,
) -> i32 {
    if dev == 0 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let (mut w, mut h, mut m, mut f) = (w, h, mips, fmt);
    check_texture_requirements(dev, &mut w, &mut h, &mut m, usage, &mut f, pool);
    match create_texture_raw(dev, w, h, m, usage, f, pool) {
        Ok(t) => {
            put_ptr(out, t);
            S_OK
        }
        Err(e) => e,
    }
}

unsafe extern "win64" fn filter_texture(
    tex: usize,
    _pal: usize,
    src_level: u32,
    filter: u32,
) -> i32 {
    if tex == 0 {
        return D3DERR_INVALIDCALL;
    }
    let from = if src_level == D3DX_DEFAULT {
        0
    } else {
        src_level
    };
    fill_mips(tex, from, filter)
}

// --- Informations d'image ---

unsafe fn image_info(d: &[u8], hint: &str, out: usize) -> i32 {
    if let Some(raw) = raw_dds(d) {
        write_info(out, raw.w, raw.h, raw.mips, raw.fmt, 4);
        return S_OK;
    }
    match decode_image(d, hint) {
        Some(s) => {
            write_info(out, s.img.w, s.img.h, s.mips, s.fmt, s.ff);
            S_OK
        }
        None => D3DXERR_INVALIDDATA,
    }
}
unsafe extern "win64" fn get_image_info_from_file_a(path: *const u8, out: usize) -> i32 {
    let Some(p) = read_path_a(path) else {
        return D3DERR_INVALIDCALL;
    };
    match read_file(&p) {
        Some(d) => image_info(&d, &ext_of(&p), out),
        None => D3DERR_NOTAVAILABLE,
    }
}
unsafe extern "win64" fn get_image_info_from_file_w(path: *const u16, out: usize) -> i32 {
    let Some(p) = read_path_w(path) else {
        return D3DERR_INVALIDCALL;
    };
    match read_file(&p) {
        Some(d) => image_info(&d, &ext_of(&p), out),
        None => D3DERR_NOTAVAILABLE,
    }
}
unsafe extern "win64" fn get_image_info_from_file_in_memory(
    data: *const u8,
    size: u32,
    out: usize,
) -> i32 {
    if data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    image_info(std::slice::from_raw_parts(data, size as usize), "", out)
}

// ═══════════════════════════════════════════════════════════════════════
// Surfaces
// ═══════════════════════════════════════════════════════════════════════

unsafe fn surface_desc(s: usize) -> Option<[u32; 8]> {
    let mut d = [0u32; 8];
    (com_call!(s, 12, fn(*mut u32) -> i32, d.as_mut_ptr()) >= 0).then_some(d)
}
unsafe fn read_rect(p: usize, w: u32, h: u32) -> [u32; 4] {
    if p == 0 {
        return [0, 0, w, h];
    }
    let r = std::ptr::read_unaligned(p as *const [i32; 4]);
    [
        r[0].clamp(0, w as i32) as u32,
        r[1].clamp(0, h as i32) as u32,
        r[2].clamp(0, w as i32) as u32,
        r[3].clamp(0, h as i32) as u32,
    ]
}

/// Écrit `src` (sous-rectangle `sr`) dans la surface `dst` (zone `drect`).
unsafe fn blit_to_surface(
    dst: usize,
    drect: usize,
    src: &Rgba,
    sr: [u32; 4],
    filter: u32,
    key: u32,
) -> i32 {
    let Some(d) = surface_desc(dst) else {
        return D3DERR_INVALIDCALL;
    };
    if bpp(d[0]) == 0 {
        crate::log_warn!("d3dx9", "format de surface 0x{:x} non géré", d[0]);
        return D3DERR_INVALIDCALL;
    }
    let dr = read_rect(drect, d[6], d[7]);
    let (w, h) = (dr[2].saturating_sub(dr[0]), dr[3].saturating_sub(dr[1]));
    if w == 0 || h == 0 || sr[2] <= sr[0] || sr[3] <= sr[1] {
        return S_OK;
    }
    let mut img = src.resample(sr, w, h, linear_filter(filter));
    img.apply_color_key(key);
    let rect = [dr[0] as i32, dr[1] as i32, dr[2] as i32, dr[3] as i32];
    let mut lr = [0u64; 2];
    if com_call!(
        dst,
        13,
        fn(*mut u64, *const i32, u32) -> i32,
        lr.as_mut_ptr(),
        rect.as_ptr(),
        0
    ) < 0
    {
        return D3DERR_INVALIDCALL;
    }
    let (pitch, bits) = super::legacy_com::locked_rect(lr.as_ptr() as usize);
    store(&img, d[0], bits, pitch);
    com_call!(dst, 14, fn() -> i32);
    S_OK
}

unsafe fn surface_from_bytes(
    dst: usize,
    drect: usize,
    d: &[u8],
    hint: &str,
    srect: usize,
    filter: u32,
    key: u32,
    info: usize,
) -> i32 {
    let Some(s) = decode_image(d, hint) else {
        return D3DXERR_INVALIDDATA;
    };
    write_info(info, s.img.w, s.img.h, s.mips, s.fmt, s.ff);
    let sr = read_rect(srect, s.img.w, s.img.h);
    blit_to_surface(dst, drect, &s.img, sr, filter, key)
}
unsafe extern "win64" fn load_surface_from_file_a(
    dst: usize,
    _pal: usize,
    drect: usize,
    path: *const u8,
    srect: usize,
    filter: u32,
    key: u32,
    info: usize,
) -> i32 {
    let Some(p) = read_path_a(path) else {
        return D3DERR_INVALIDCALL;
    };
    match read_file(&p) {
        Some(d) => surface_from_bytes(dst, drect, &d, &ext_of(&p), srect, filter, key, info),
        None => D3DERR_NOTAVAILABLE,
    }
}
unsafe extern "win64" fn load_surface_from_file_w(
    dst: usize,
    _pal: usize,
    drect: usize,
    path: *const u16,
    srect: usize,
    filter: u32,
    key: u32,
    info: usize,
) -> i32 {
    let Some(p) = read_path_w(path) else {
        return D3DERR_INVALIDCALL;
    };
    match read_file(&p) {
        Some(d) => surface_from_bytes(dst, drect, &d, &ext_of(&p), srect, filter, key, info),
        None => D3DERR_NOTAVAILABLE,
    }
}
unsafe extern "win64" fn load_surface_from_file_in_memory(
    dst: usize,
    _pal: usize,
    drect: usize,
    data: *const u8,
    size: u32,
    srect: usize,
    filter: u32,
    key: u32,
    info: usize,
) -> i32 {
    if data.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let d = std::slice::from_raw_parts(data, size as usize);
    surface_from_bytes(dst, drect, d, "", srect, filter, key, info)
}
unsafe extern "win64" fn load_surface_from_resource_a(
    dst: usize,
    _pal: usize,
    drect: usize,
    module: usize,
    name: usize,
    srect: usize,
    filter: u32,
    key: u32,
    info: usize,
) -> i32 {
    match resource_bytes(module, name, false) {
        Some(d) => surface_from_bytes(dst, drect, &d, "", srect, filter, key, info),
        None => D3DXERR_INVALIDDATA,
    }
}
unsafe extern "win64" fn load_surface_from_resource_w(
    dst: usize,
    _pal: usize,
    drect: usize,
    module: usize,
    name: usize,
    srect: usize,
    filter: u32,
    key: u32,
    info: usize,
) -> i32 {
    match resource_bytes(module, name, true) {
        Some(d) => surface_from_bytes(dst, drect, &d, "", srect, filter, key, info),
        None => D3DXERR_INVALIDDATA,
    }
}
unsafe extern "win64" fn load_surface_from_memory(
    dst: usize,
    _dpal: usize,
    drect: usize,
    src: usize,
    sfmt: u32,
    spitch: u32,
    _spal: usize,
    srect: usize,
    filter: u32,
    key: u32,
) -> i32 {
    if src == 0 || srect == 0 {
        return D3DERR_INVALIDCALL;
    }
    let r = std::ptr::read_unaligned(srect as *const [i32; 4]);
    let (w, h) = ((r[2] - r[0]).max(0) as u32, (r[3] - r[1]).max(0) as u32);
    let base = src as isize + r[1] as isize * spitch as isize + r[0] as isize * bpp(sfmt) as isize;
    let Some(img) = load(sfmt, base as usize, spitch as i32, w, h) else {
        crate::log_warn!("d3dx9", "format source 0x{:x} non géré", sfmt);
        return D3DERR_INVALIDCALL;
    };
    blit_to_surface(dst, drect, &img, [0, 0, w, h], filter, key)
}
unsafe fn read_surface(s: usize, rect: [u32; 4]) -> Option<Rgba> {
    let d = surface_desc(s)?;
    let r = [
        rect[0] as i32,
        rect[1] as i32,
        rect[2] as i32,
        rect[3] as i32,
    ];
    let mut lr = [0u64; 2];
    if com_call!(
        s,
        13,
        fn(*mut u64, *const i32, u32) -> i32,
        lr.as_mut_ptr(),
        r.as_ptr(),
        0x10
    ) < 0
    {
        return None;
    }
    let (pitch, bits) = super::legacy_com::locked_rect(lr.as_ptr() as usize);
    let img = load(d[0], bits, pitch, rect[2] - rect[0], rect[3] - rect[1]);
    com_call!(s, 14, fn() -> i32);
    img
}
unsafe extern "win64" fn load_surface_from_surface(
    dst: usize,
    _dpal: usize,
    drect: usize,
    src: usize,
    _spal: usize,
    srect: usize,
    filter: u32,
    key: u32,
) -> i32 {
    let Some(d) = surface_desc(src) else {
        return D3DERR_INVALIDCALL;
    };
    let sr = read_rect(srect, d[6], d[7]);
    let Some(img) = read_surface(src, sr) else {
        return D3DERR_INVALIDCALL;
    };
    blit_to_surface(dst, drect, &img, [0, 0, img.w, img.h], filter, key)
}

/// BMP 32 bits de bas en haut, alpha conservé.
pub(crate) fn encode_bmp(img: &Rgba) -> Vec<u8> {
    let size = img.w as usize * img.h as usize * 4;
    let mut f = Vec::with_capacity(54 + size);
    f.extend_from_slice(b"BM");
    f.extend_from_slice(&((54 + size) as u32).to_le_bytes());
    f.extend_from_slice(&[0; 4]);
    f.extend_from_slice(&54u32.to_le_bytes());
    f.extend_from_slice(&40u32.to_le_bytes());
    f.extend_from_slice(&(img.w as i32).to_le_bytes());
    f.extend_from_slice(&(img.h as i32).to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&32u16.to_le_bytes());
    f.extend_from_slice(&[0; 4]);
    f.extend_from_slice(&(size as u32).to_le_bytes());
    f.extend_from_slice(&[0; 16]);
    for y in (0..img.h).rev() {
        for x in 0..img.w {
            let [r, g, b, a] = img.get(x, y);
            f.extend_from_slice(&[b, g, r, a]);
        }
    }
    f
}

unsafe fn save_surface(path: Option<std::path::PathBuf>, ff: u32, s: usize, rect: usize) -> i32 {
    let Some(p) = path else {
        return D3DERR_INVALIDCALL;
    };
    if ff != 0 && ff != 6 {
        crate::log_warn!(
            "d3dx9",
            "enregistrement au format {ff} non géré : BMP écrit à la place"
        );
    }
    let Some(d) = surface_desc(s) else {
        return D3DERR_INVALIDCALL;
    };
    let Some(img) = read_surface(s, read_rect(rect, d[6], d[7])) else {
        return D3DERR_INVALIDCALL;
    };
    match std::fs::write(&p, encode_bmp(&img)) {
        Ok(()) => S_OK,
        Err(_) => D3DERR_INVALIDCALL,
    }
}
unsafe extern "win64" fn save_surface_to_file_a(
    path: *const u8,
    ff: u32,
    s: usize,
    _pal: usize,
    rect: usize,
) -> i32 {
    save_surface(read_path_a(path), ff, s, rect)
}
unsafe extern "win64" fn save_surface_to_file_w(
    path: *const u16,
    ff: u32,
    s: usize,
    _pal: usize,
    rect: usize,
) -> i32 {
    save_surface(read_path_w(path), ff, s, rect)
}
unsafe fn save_texture(path: Option<std::path::PathBuf>, ff: u32, tex: usize) -> i32 {
    let mut s: usize = 0;
    if tex == 0 || com_call!(tex, 18, fn(u32, *mut usize) -> i32, 0, &mut s) < 0 || s == 0 {
        return D3DERR_INVALIDCALL;
    }
    let hr = save_surface(path, ff, s, 0);
    com_call!(s, 2, fn() -> u32);
    hr
}
unsafe extern "win64" fn save_texture_to_file_a(
    path: *const u8,
    ff: u32,
    tex: usize,
    _pal: usize,
) -> i32 {
    save_texture(read_path_a(path), ff, tex)
}
unsafe extern "win64" fn save_texture_to_file_w(
    path: *const u16,
    ff: u32,
    tex: usize,
    _pal: usize,
) -> i32 {
    save_texture(read_path_w(path), ff, tex)
}

// ═══════════════════════════════════════════════════════════════════════
// Divers : versions, profils, shaders et effets (pas encore compilés)
// ═══════════════════════════════════════════════════════════════════════

unsafe extern "win64" fn check_version(_d3d: u32, _d3dx: u32) -> i32 {
    1
}
unsafe extern "win64" fn debug_mute(_mute: i32) -> i32 {
    0
}
unsafe extern "win64" fn get_driver_level(_dev: usize) -> u32 {
    900
}
static VS_PROFILE: &[u8] = b"vs_3_0\0";
static PS_PROFILE: &[u8] = b"ps_3_0\0";
unsafe extern "win64" fn get_vertex_shader_profile(_dev: usize) -> usize {
    VS_PROFILE.as_ptr() as usize
}
unsafe extern "win64" fn get_pixel_shader_profile(_dev: usize) -> usize {
    PS_PROFILE.as_ptr() as usize
}
unsafe extern "win64" fn get_shader_version(f: *const u32) -> u32 {
    if f.is_null() {
        0
    } else {
        f.read_unaligned()
    }
}
unsafe extern "win64" fn get_shader_size(f: *const u32) -> u32 {
    if f.is_null() {
        return 0;
    }
    let mut i = 1usize;
    loop {
        let t = f.add(i).read_unaligned();
        if t == 0x0000_FFFF {
            return ((i + 1) * 4) as u32;
        }
        // Commentaire : longueur en mots dans les bits 16..30.
        i += if t & 0xFFFF == 0xFFFE {
            1 + ((t >> 16) & 0x7FFF) as usize
        } else {
            1
        };
        if i > 1 << 20 {
            return 0;
        }
    }
}

pub fn exports() -> crate::dlls::Exports {
    let mut e = crate::exports! {
        "D3DXMatrixMultiply" => fn matrix_multiply,
        "D3DXMatrixMultiplyTranspose" => fn matrix_multiply_transpose,
        "D3DXMatrixTranspose" => fn matrix_transpose,
        "D3DXMatrixInverse" => fn matrix_inverse,
        "D3DXMatrixDeterminant" => fn matrix_determinant,
        "D3DXMatrixDecompose" => fn matrix_decompose,
        "D3DXMatrixTranslation" => fn matrix_translation,
        "D3DXMatrixScaling" => fn matrix_scaling,
        "D3DXMatrixRotationX" => fn matrix_rotation_x,
        "D3DXMatrixRotationY" => fn matrix_rotation_y,
        "D3DXMatrixRotationZ" => fn matrix_rotation_z,
        "D3DXMatrixRotationAxis" => fn matrix_rotation_axis,
        "D3DXMatrixRotationQuaternion" => fn matrix_rotation_quaternion,
        "D3DXMatrixRotationYawPitchRoll" => fn matrix_rotation_ypr,
        "D3DXMatrixLookAtLH" => fn matrix_look_at_lh,
        "D3DXMatrixLookAtRH" => fn matrix_look_at_rh,
        "D3DXMatrixPerspectiveFovLH" => fn matrix_perspective_fov_lh,
        "D3DXMatrixPerspectiveFovRH" => fn matrix_perspective_fov_rh,
        "D3DXMatrixPerspectiveLH" => fn matrix_perspective_lh,
        "D3DXMatrixPerspectiveRH" => fn matrix_perspective_rh,
        "D3DXMatrixPerspectiveOffCenterLH" => fn matrix_perspective_off_center_lh,
        "D3DXMatrixPerspectiveOffCenterRH" => fn matrix_perspective_off_center_rh,
        "D3DXMatrixOrthoLH" => fn matrix_ortho_lh,
        "D3DXMatrixOrthoRH" => fn matrix_ortho_rh,
        "D3DXMatrixOrthoOffCenterLH" => fn matrix_ortho_off_center_lh,
        "D3DXMatrixOrthoOffCenterRH" => fn matrix_ortho_off_center_rh,
        "D3DXMatrixReflect" => fn matrix_reflect,
        "D3DXMatrixShadow" => fn matrix_shadow,
        "D3DXMatrixAffineTransformation" => fn matrix_affine_transformation,
        "D3DXMatrixAffineTransformation2D" => fn matrix_affine_transformation_2d,
        "D3DXMatrixTransformation2D" => fn matrix_transformation_2d,
        "D3DXVec2Normalize" => fn vec2_normalize,
        "D3DXVec2Transform" => fn vec2_transform,
        "D3DXVec2TransformCoord" => fn vec2_transform_coord,
        "D3DXVec2TransformNormal" => fn vec2_transform_normal,
        "D3DXVec3Normalize" => fn vec3_normalize,
        "D3DXVec3Transform" => fn vec3_transform,
        "D3DXVec3TransformCoord" => fn vec3_transform_coord,
        "D3DXVec3TransformNormal" => fn vec3_transform_normal,
        "D3DXVec3TransformArray" => fn vec3_transform_array,
        "D3DXVec3TransformCoordArray" => fn vec3_transform_coord_array,
        "D3DXVec3TransformNormalArray" => fn vec3_transform_normal_array,
        "D3DXVec3Project" => fn vec3_project,
        "D3DXVec3Unproject" => fn vec3_unproject,
        "D3DXVec3BaryCentric" => fn vec3_barycentric,
        "D3DXVec4Normalize" => fn vec4_normalize,
        "D3DXVec4Transform" => fn vec4_transform,
        "D3DXVec4Cross" => fn vec4_cross,
        "D3DXQuaternionMultiply" => fn quat_multiply,
        "D3DXQuaternionRotationAxis" => fn quat_rotation_axis,
        "D3DXQuaternionRotationMatrix" => fn quat_rotation_matrix,
        "D3DXQuaternionRotationYawPitchRoll" => fn quat_rotation_ypr,
        "D3DXQuaternionNormalize" => fn quat_normalize,
        "D3DXQuaternionInverse" => fn quat_inverse,
        "D3DXQuaternionSlerp" => fn quat_slerp,
        "D3DXQuaternionToAxisAngle" => fn quat_to_axis_angle,
        "D3DXPlaneNormalize" => fn plane_normalize,
        "D3DXPlaneFromPointNormal" => fn plane_from_point_normal,
        "D3DXPlaneFromPoints" => fn plane_from_points,
        "D3DXPlaneIntersectLine" => fn plane_intersect_line,
        "D3DXPlaneTransform" => fn plane_transform,
        "D3DXComputeBoundingSphere" => fn compute_bounding_sphere,
        "D3DXComputeBoundingBox" => fn compute_bounding_box,
        "D3DXIntersectTri" => fn intersect_tri,
        "D3DXSphereBoundProbe" => fn sphere_bound_probe,
        "D3DXBoxBoundProbe" => fn box_bound_probe,
        "D3DXFloat32To16Array" => fn float32_to_16_array,
        "D3DXFloat16To32Array" => fn float16_to_32_array,
        "D3DXCreateBuffer" => fn create_buffer,
        "D3DXCreateTexture" => fn create_texture,
        "D3DXCheckTextureRequirements" => fn check_texture_requirements,
        "D3DXCreateTextureFromFileA" => fn create_texture_from_file_a,
        "D3DXCreateTextureFromFileW" => fn create_texture_from_file_w,
        "D3DXCreateTextureFromFileExA" => fn create_texture_from_file_ex_a,
        "D3DXCreateTextureFromFileExW" => fn create_texture_from_file_ex_w,
        "D3DXCreateTextureFromFileInMemory" => fn create_texture_from_file_in_memory,
        "D3DXCreateTextureFromFileInMemoryEx" => fn create_texture_from_file_in_memory_ex,
        "D3DXCreateTextureFromResourceA" => fn create_texture_from_resource_a,
        "D3DXCreateTextureFromResourceW" => fn create_texture_from_resource_w,
        "D3DXCreateTextureFromResourceExA" => fn create_texture_from_resource_ex_a,
        "D3DXCreateTextureFromResourceExW" => fn create_texture_from_resource_ex_w,
        "D3DXFilterTexture" => fn filter_texture,
        "D3DXGetImageInfoFromFileA" => fn get_image_info_from_file_a,
        "D3DXGetImageInfoFromFileW" => fn get_image_info_from_file_w,
        "D3DXGetImageInfoFromFileInMemory" => fn get_image_info_from_file_in_memory,
        "D3DXLoadSurfaceFromFileA" => fn load_surface_from_file_a,
        "D3DXLoadSurfaceFromFileW" => fn load_surface_from_file_w,
        "D3DXLoadSurfaceFromFileInMemory" => fn load_surface_from_file_in_memory,
        "D3DXLoadSurfaceFromResourceA" => fn load_surface_from_resource_a,
        "D3DXLoadSurfaceFromResourceW" => fn load_surface_from_resource_w,
        "D3DXLoadSurfaceFromMemory" => fn load_surface_from_memory,
        "D3DXLoadSurfaceFromSurface" => fn load_surface_from_surface,
        "D3DXSaveSurfaceToFileA" => fn save_surface_to_file_a,
        "D3DXSaveSurfaceToFileW" => fn save_surface_to_file_w,
        "D3DXSaveTextureToFileA" => fn save_texture_to_file_a,
        "D3DXSaveTextureToFileW" => fn save_texture_to_file_w,
        "D3DXCheckVersion" => fn check_version,
        "D3DXDebugMute" => fn debug_mute,
        "D3DXGetDriverLevel" => fn get_driver_level,
        "D3DXGetVertexShaderProfile" => fn get_vertex_shader_profile,
        "D3DXGetPixelShaderProfile" => fn get_pixel_shader_profile,
        "D3DXGetShaderVersion" => fn get_shader_version,
        "D3DXGetShaderSize" => fn get_shader_size,
    };
    e.extend(super::d3dx9_draw::exports());
    e.extend(super::d3dx9_shader::exports());
    e.extend(super::d3dx9_mesh::exports());
    e.extend(super::d3dx9_effect::exports());
    e
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn matrices_et_inverse() {
        let m = mul(
            &mul(&scaling(2.0, 3.0, 4.0), &rot_y(0.7)),
            &translation(1.0, -2.0, 5.0),
        );
        let (inv, det) = inverse(&m).unwrap();
        assert!(close(&mul(&m, &inv), &IDENTITY));
        assert!((det - determinant(&m)).abs() < 1e-3);
        assert!((det - 24.0).abs() < 1e-3);
        // Un point transformé par la perspective LH tombe dans [0, 1] en z.
        let p = persp_fov(std::f32::consts::FRAC_PI_4, 4.0 / 3.0, 1.0, 100.0, false);
        let near = coord([0.0, 0.0, 1.0], &p);
        let far = coord([0.0, 0.0, 100.0], &p);
        assert!(near[2].abs() < 1e-5 && (far[2] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn quaternions_et_matrices_concordent() {
        let q = unsafe {
            let mut q = [0f32; 4];
            quat_rotation_ypr(q.as_mut_ptr(), 0.3, -0.4, 1.1);
            q
        };
        let a = rot_quat(q);
        let b = mul(&mul(&rot_z(1.1), &rot_x(-0.4)), &rot_y(0.3));
        assert!(close(&a, &b));
        assert!(close(&quat_from_matrix(&a), &q) || close(&quat_from_matrix(&a), &q.map(|x| -x)));
        assert!(close(&rot_axis([0.0, 0.0, 1.0], 0.5), &rot_z(0.5)));
    }

    #[test]
    fn demi_flottants() {
        for v in [0.0f32, 1.0, -2.5, 0.000_061_035_156, 65504.0, 0.333_251_95] {
            assert_eq!(f16_to_f32(f32_to_f16(v)), v);
        }
        assert_eq!(f32_to_f16(1.0), 0x3C00);
        assert_eq!(f32_to_f16(1e9), 0x7BFF);
    }

    #[test]
    fn formats_de_pixels() {
        let c = [200u8, 100, 50, 128];
        for f in [
            FMT_A8R8G8B8,
            FMT_A8B8G8R8,
            FMT_A4R4G4B4,
            FMT_A1R5G5B5,
            FMT_R5G6B5,
            FMT_A8L8,
            FMT_A32B32G32R32F,
        ] {
            let mut b = [0u8; 16];
            unsafe {
                encode(f, c, b.as_mut_ptr());
                let d = decode(f, b.as_ptr());
                let tol = match f {
                    FMT_A4R4G4B4 => 17,
                    FMT_A1R5G5B5 | FMT_R5G6B5 => 9,
                    FMT_A8L8 => 200,
                    _ => 1,
                };
                for i in 0..3 {
                    assert!((d[i] as i32 - c[i] as i32).abs() <= tol, "format {f}");
                }
            }
        }
    }

    #[test]
    fn reduction_et_cle_de_couleur() {
        let mut img = Rgba {
            w: 4,
            h: 4,
            px: [255u8, 0, 255, 255].repeat(16),
        };
        img.px[0..4].copy_from_slice(&[255, 255, 255, 255]);
        let half = img.resample([0, 0, 4, 4], 2, 2, false);
        assert_eq!(half.get(1, 1), [255, 0, 255, 255]);
        assert_eq!(half.get(0, 0)[1], 63);
        img.apply_color_key(0xFFFF_00FF);
        assert_eq!(img.get(1, 1), [0, 0, 0, 0]);
        assert_eq!(img.get(0, 0), [255, 255, 255, 255]);
        assert_eq!(mip_count(256, 64), 9);
        assert_eq!(target_format(0, FMT_R8G8B8, 0), FMT_X8R8G8B8);
        assert_eq!(target_format(0, FMT_R8G8B8, 0xFF00_0000), FMT_A8R8G8B8);
        assert_eq!(
            target_format(fourcc(b"DXT5"), FMT_A8R8G8B8, 0),
            FMT_A8R8G8B8
        );
    }

    #[test]
    fn taille_de_shader() {
        let ps = [
            0xFFFF_0101u32,
            0x0002_FFFE,
            1,
            2,
            0x42,
            0xB00F_0000,
            0x0000_FFFF,
        ];
        assert_eq!(unsafe { get_shader_size(ps.as_ptr()) }, 28);
    }
}
