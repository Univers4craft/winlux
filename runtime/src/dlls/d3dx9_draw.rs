//! D3DX9 (#834) : ID3DXSprite et ID3DXFont.
//!
//! Le sprite met en file des quadrilatères texturés et les dessine par
//! DrawPrimitiveUP, regroupés par texture, avec les états que D3DX pose
//! (mélange alpha, filtrage bilinéaire, étage de texture « modulate »),
//! restaurés à End par un bloc d'états. La police rend chaque texte par le
//! GDI de Winlux dans une texture A8R8G8B8 (couverture dans l'alpha),
//! gardée en cache, puis la dessine avec un sprite.

#![allow(clippy::too_many_arguments)]

use super::d3dx9::*;
use super::legacy_com::{put_ptr, rd32, vtable, wr32};
use crate::com_call;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

type M = [f32; 16];

// Méthodes de IDirect3DDevice9 utilisées.
const DEV_ADDREF: usize = 1;
const DEV_RELEASE: usize = 2;
const DEV_CREATE_TEXTURE: usize = 23;
const DEV_SET_RENDER_STATE: usize = 57;
const DEV_CREATE_STATE_BLOCK: usize = 59;
const DEV_SET_TEXTURE: usize = 65;
const DEV_SET_TSS: usize = 67;
const DEV_SET_SAMPLER: usize = 69;
const DEV_DRAW_UP: usize = 83;
const DEV_SET_FVF: usize = 89;
const DEV_SET_VS: usize = 92;
const DEV_SET_PS: usize = 107;

const SPRITE_DONOTSAVESTATE: u32 = 1;
const SPRITE_DONOTMODIFY_RENDERSTATE: u32 = 2;
const SPRITE_OBJECTSPACE: u32 = 4;
const SPRITE_ALPHABLEND: u32 = 16;
const SPRITE_SORT_TEXTURE: u32 = 32;
const SPRITE_DO_NOT_ADDREF_TEXTURE: u32 = 256;

#[derive(Clone, Copy)]
struct Quad {
    tex: usize,
    /// x, y, z, couleur (bits), u, v pour chaque coin (haut-gauche,
    /// haut-droit, bas-gauche, bas-droit).
    v: [[f32; 6]; 4],
}

struct SpriteState {
    transform: M,
    flags: u32,
    began: bool,
    block: usize,
    quads: Vec<Quad>,
}

#[repr(C)]
struct Sprite {
    vtbl: usize,
    refs: AtomicU32,
    dev: usize,
    st: Mutex<SpriteState>,
}

unsafe fn sprite<'a>(this: usize) -> &'a Sprite {
    &*(this as *const Sprite)
}

unsafe extern "win64" fn sp_qi(this: usize, _iid: usize, out: usize) -> i32 {
    if out == 0 {
        return E_NOINTERFACE;
    }
    sp_addref(this);
    put_ptr(out, this);
    S_OK
}
unsafe extern "win64" fn sp_addref(this: usize) -> u32 {
    sprite(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn sp_release(this: usize) -> u32 {
    let n = sprite(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let s = Box::from_raw(this as *mut Sprite);
        let st = s.st.into_inner().unwrap_or_else(|e| e.into_inner());
        release_quads(&st.quads, st.flags);
        if st.block != 0 {
            com_call!(st.block, 2, fn() -> u32);
        }
        com_call!(s.dev, DEV_RELEASE, fn() -> u32);
    }
    n
}
unsafe extern "win64" fn sp_get_device(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = sprite(this).dev;
    com_call!(d, DEV_ADDREF, fn() -> u32);
    put_ptr(out, d);
    S_OK
}
unsafe extern "win64" fn sp_get_transform(this: usize, out: *mut f32) -> i32 {
    if out.is_null() {
        return D3DERR_INVALIDCALL;
    }
    let m = super::lock(&sprite(this).st).transform;
    std::ptr::write_unaligned(out as *mut M, m);
    S_OK
}
unsafe extern "win64" fn sp_set_transform(this: usize, m: *const f32) -> i32 {
    if m.is_null() {
        return D3DERR_INVALIDCALL;
    }
    super::lock(&sprite(this).st).transform = std::ptr::read_unaligned(m as *const M);
    S_OK
}
unsafe extern "win64" fn sp_set_world_view(_this: usize, _world: usize, _view: usize) -> i32 {
    S_OK
}

unsafe fn rs(dev: usize, state: u32, v: u32) {
    com_call!(dev, DEV_SET_RENDER_STATE, fn(u32, u32) -> i32, state, v);
}
unsafe fn tss(dev: usize, stage: u32, ty: u32, v: u32) {
    com_call!(dev, DEV_SET_TSS, fn(u32, u32, u32) -> i32, stage, ty, v);
}
unsafe fn samp(dev: usize, ty: u32, v: u32) {
    com_call!(dev, DEV_SET_SAMPLER, fn(u32, u32, u32) -> i32, 0, ty, v);
}

/// États posés par D3DX pour dessiner des sprites.
unsafe fn set_states(dev: usize, flags: u32) {
    let blend = flags & SPRITE_ALPHABLEND != 0;
    rs(dev, 27, blend as u32); // ALPHABLENDENABLE
    rs(dev, 19, 5); // SRCBLEND = SRCALPHA
    rs(dev, 20, 6); // DESTBLEND = INVSRCALPHA
    rs(dev, 171, 1); // BLENDOP = ADD
    rs(dev, 15, blend as u32); // ALPHATESTENABLE
    rs(dev, 24, 0); // ALPHAREF
    rs(dev, 25, 5); // ALPHAFUNC = GREATER
    rs(dev, 22, 1); // CULLMODE = NONE
    rs(dev, 8, 3); // FILLMODE = SOLID
    rs(dev, 137, 0); // LIGHTING
    rs(dev, 28, 0); // FOGENABLE
    rs(dev, 29, 0); // SPECULARENABLE
    rs(dev, 168, 0xF); // COLORWRITEENABLE
    rs(dev, 174, 0); // SCISSORTESTENABLE
    rs(dev, 206, 0); // SEPARATEALPHABLENDENABLE
    if flags & SPRITE_OBJECTSPACE == 0 {
        rs(dev, 7, 0); // ZENABLE
    }
    tss(dev, 0, 1, 4); // COLOROP = MODULATE
    tss(dev, 0, 2, 2); // COLORARG1 = TEXTURE
    tss(dev, 0, 3, 0); // COLORARG2 = DIFFUSE
    tss(dev, 0, 4, 4); // ALPHAOP = MODULATE
    tss(dev, 0, 5, 2);
    tss(dev, 0, 6, 0);
    tss(dev, 0, 11, 0); // TEXCOORDINDEX
    tss(dev, 0, 24, 0); // TEXTURETRANSFORMFLAGS
    tss(dev, 1, 1, 1); // étage 1 : DISABLE
    tss(dev, 1, 4, 1);
    samp(dev, 1, 3); // ADDRESSU = CLAMP
    samp(dev, 2, 3); // ADDRESSV = CLAMP
    samp(dev, 5, 2); // MAGFILTER = LINEAR
    samp(dev, 6, 2); // MINFILTER = LINEAR
    samp(dev, 7, 2); // MIPFILTER = LINEAR
    samp(dev, 11, 0); // SRGBTEXTURE
    com_call!(dev, DEV_SET_VS, fn(usize) -> i32, 0);
    com_call!(dev, DEV_SET_PS, fn(usize) -> i32, 0);
}

unsafe extern "win64" fn sp_begin(this: usize, flags: u32) -> i32 {
    let s = sprite(this);
    let mut st = super::lock(&s.st);
    if st.began {
        return D3DERR_INVALIDCALL;
    }
    st.flags = flags;
    st.began = true;
    if flags & SPRITE_DONOTSAVESTATE == 0 {
        let mut b: usize = 0;
        if com_call!(
            s.dev,
            DEV_CREATE_STATE_BLOCK,
            fn(u32, *mut usize) -> i32,
            1,
            &mut b
        ) >= 0
        {
            st.block = b;
        }
    }
    drop(st);
    if flags & SPRITE_DONOTMODIFY_RENDERSTATE == 0 {
        set_states(s.dev, flags);
    }
    S_OK
}

/// Taille du niveau 0 d'une texture.
pub(crate) unsafe fn texture_size(tex: usize) -> (u32, u32) {
    let mut d = [0u32; 8];
    if com_call!(tex, 17, fn(u32, *mut u32) -> i32, 0, d.as_mut_ptr()) < 0 {
        return (1, 1);
    }
    (d[6].max(1), d[7].max(1))
}

unsafe fn transform(m: &M, x: f32, y: f32, z: f32) -> [f32; 3] {
    [
        x * m[0] + y * m[4] + z * m[8] + m[12],
        x * m[1] + y * m[5] + z * m[9] + m[13],
        x * m[2] + y * m[6] + z * m[10] + m[14],
    ]
}

unsafe extern "win64" fn sp_draw(
    this: usize,
    tex: usize,
    src: usize,
    center: *const f32,
    pos: *const f32,
    color: u32,
) -> i32 {
    let s = sprite(this);
    if tex == 0 {
        return D3DERR_INVALIDCALL;
    }
    let mut st = super::lock(&s.st);
    if !st.began {
        return D3DERR_INVALIDCALL;
    }
    let (tw, th) = texture_size(tex);
    let r = if src == 0 {
        [0, 0, tw as i32, th as i32]
    } else {
        std::ptr::read_unaligned(src as *const [i32; 4])
    };
    let (w, h) = ((r[2] - r[0]) as f32, (r[3] - r[1]) as f32);
    let c = if center.is_null() {
        [0.0; 3]
    } else {
        std::ptr::read_unaligned(center as *const [f32; 3])
    };
    let p = if pos.is_null() {
        [0.0; 3]
    } else {
        std::ptr::read_unaligned(pos as *const [f32; 3])
    };
    let m = st.transform;
    let mut q = Quad {
        tex,
        v: [[0.0; 6]; 4],
    };
    for (i, (cx, cy)) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)].iter().enumerate() {
        let xyz = transform(&m, cx - c[0] + p[0], cy - c[1] + p[1], p[2] - c[2]);
        q.v[i] = [
            xyz[0],
            xyz[1],
            xyz[2],
            f32::from_bits(color),
            (r[0] as f32 + cx) / tw as f32,
            (r[1] as f32 + cy) / th as f32,
        ];
    }
    if st.flags & SPRITE_DO_NOT_ADDREF_TEXTURE == 0 {
        com_call!(tex, 1, fn() -> u32);
    }
    st.quads.push(q);
    S_OK
}

unsafe fn release_quads(quads: &[Quad], flags: u32) {
    if flags & SPRITE_DO_NOT_ADDREF_TEXTURE == 0 {
        for q in quads {
            com_call!(q.tex, 2, fn() -> u32);
        }
    }
}

unsafe fn flush(s: &Sprite, st: &mut SpriteState) {
    if st.quads.is_empty() {
        return;
    }
    let mut quads = std::mem::take(&mut st.quads);
    if st.flags & SPRITE_SORT_TEXTURE != 0 {
        quads.sort_by_key(|q| q.tex);
    }
    let screen = st.flags & SPRITE_OBJECTSPACE == 0;
    // Écran : XYZRHW | DIFFUSE | TEX1 (décalage d'un demi-pixel comme D3D9) ;
    // espace objet : XYZ | DIFFUSE | TEX1, transformé par l'appareil.
    let (fvf, stride) = if screen { (0x144, 28) } else { (0x142, 24) };
    com_call!(s.dev, DEV_SET_FVF, fn(u32) -> i32, fvf);
    let mut i = 0;
    while i < quads.len() {
        let tex = quads[i].tex;
        let mut verts: Vec<f32> = Vec::new();
        while i < quads.len() && quads[i].tex == tex {
            let q = &quads[i];
            for k in [0usize, 1, 2, 2, 1, 3] {
                let v = q.v[k];
                if screen {
                    verts.extend_from_slice(&[v[0] - 0.5, v[1] - 0.5, v[2], 1.0, v[3], v[4], v[5]]);
                } else {
                    verts.extend_from_slice(&v);
                }
            }
            i += 1;
        }
        com_call!(s.dev, DEV_SET_TEXTURE, fn(u32, usize) -> i32, 0, tex);
        let n = verts.len() as u32 * 4 / stride / 3;
        com_call!(
            s.dev,
            DEV_DRAW_UP,
            fn(u32, u32, *const f32, u32) -> i32,
            4,
            n,
            verts.as_ptr(),
            stride
        );
    }
    com_call!(s.dev, DEV_SET_TEXTURE, fn(u32, usize) -> i32, 0, 0);
    release_quads(&quads, st.flags);
}

unsafe extern "win64" fn sp_flush(this: usize) -> i32 {
    let s = sprite(this);
    let mut st = super::lock(&s.st);
    if !st.began {
        return D3DERR_INVALIDCALL;
    }
    flush(s, &mut st);
    S_OK
}
unsafe extern "win64" fn sp_end(this: usize) -> i32 {
    let s = sprite(this);
    let mut st = super::lock(&s.st);
    if !st.began {
        return D3DERR_INVALIDCALL;
    }
    flush(s, &mut st);
    st.began = false;
    let b = std::mem::take(&mut st.block);
    drop(st);
    if b != 0 {
        com_call!(b, 5, fn() -> i32); // Apply
        com_call!(b, 2, fn() -> u32);
    }
    S_OK
}
unsafe extern "win64" fn ok0(_this: usize) -> i32 {
    S_OK
}

fn sprite_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    vtable(
        "ID3DXSprite",
        &[
            a(sp_qi as *const (), 2),
            a(sp_addref as *const (), 0),
            a(sp_release as *const (), 0),
            a(sp_get_device as *const (), 1),
            a(sp_get_transform as *const (), 1),
            a(sp_set_transform as *const (), 1),
            a(sp_set_world_view as *const (), 2),
            a(sp_set_world_view as *const (), 2),
            a(sp_begin as *const (), 1),
            a(sp_draw as *const (), 5),
            a(sp_flush as *const (), 0),
            a(sp_end as *const (), 0),
            a(ok0 as *const (), 0),
            a(ok0 as *const (), 0),
        ],
    )
}

pub(crate) unsafe fn new_sprite(dev: usize) -> usize {
    com_call!(dev, DEV_ADDREF, fn() -> u32);
    Box::into_raw(Box::new(Sprite {
        vtbl: sprite_vtable(),
        refs: AtomicU32::new(1),
        dev,
        st: Mutex::new(SpriteState {
            transform: IDENTITY,
            flags: 0,
            began: false,
            block: 0,
            quads: Vec::new(),
        }),
    })) as usize
}

unsafe extern "win64" fn create_sprite(dev: usize, out: usize) -> i32 {
    if dev == 0 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    put_ptr(out, new_sprite(dev));
    S_OK
}

// ═══════════════════════════════════════════════════════════════════════
// ID3DXFont
// ═══════════════════════════════════════════════════════════════════════

const DT_CENTER: u32 = 0x1;
const DT_RIGHT: u32 = 0x2;
const DT_VCENTER: u32 = 0x4;
const DT_BOTTOM: u32 = 0x8;
const DT_WORDBREAK: u32 = 0x10;
const DT_NOCLIP: u32 = 0x100;
const DT_CALCRECT: u32 = 0x400;

#[derive(Clone)]
struct FontDesc {
    height: i32,
    width: u32,
    weight: u32,
    mips: u32,
    italic: u32,
    charset: u8,
    out_precision: u8,
    quality: u8,
    pitch: u8,
    face: String,
}

struct Cached {
    tex: usize,
    w: u32,
    h: u32,
}

#[repr(C)]
struct Font {
    vtbl: usize,
    refs: AtomicU32,
    dev: usize,
    desc: FontDesc,
    hdc: usize,
    hfont: usize,
    sprite: usize,
    cache: Mutex<HashMap<(Vec<u16>, u32, u32), Cached>>,
}

unsafe fn font<'a>(this: usize) -> &'a Font {
    &*(this as *const Font)
}

/// Fonction exportée par le runtime (GDI, user32), appelée en win64.
fn api(dll: &str, name: &str) -> usize {
    super::lookup(dll, name).unwrap_or(0)
}

unsafe fn gdi_dc_and_font(d: &FontDesc) -> (usize, usize) {
    let create_dc: extern "win64" fn(usize) -> usize =
        std::mem::transmute(api("gdi32", "CreateCompatibleDC"));
    let create_font: unsafe extern "win64" fn(
        i32,
        i32,
        i32,
        i32,
        i32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        u32,
        *const u16,
    ) -> usize = std::mem::transmute(api("gdi32", "CreateFontW"));
    let select: extern "win64" fn(usize, usize) -> usize =
        std::mem::transmute(api("gdi32", "SelectObject"));
    let face: Vec<u16> = d.face.encode_utf16().chain(std::iter::once(0)).collect();
    let dc = create_dc(0);
    let f = create_font(
        d.height,
        d.width as i32,
        0,
        0,
        d.weight as i32,
        d.italic,
        0,
        0,
        d.charset as u32,
        d.out_precision as u32,
        0,
        d.quality as u32,
        d.pitch as u32,
        face.as_ptr(),
    );
    if dc != 0 && f != 0 {
        select(dc, f);
    }
    (dc, f)
}

unsafe fn draw_text_gdi(dc: usize, text: &[u16], rect: &mut [i32; 4], format: u32) -> i32 {
    let draw: unsafe extern "win64" fn(usize, *const u16, i32, *mut i32, u32) -> i32 =
        std::mem::transmute(api("user32", "DrawTextW"));
    draw(
        dc,
        text.as_ptr(),
        text.len() as i32,
        rect.as_mut_ptr(),
        format,
    )
}

/// Texture du texte (blanc, couverture dans l'alpha) et sa taille.
unsafe fn render_text(
    f: &Font,
    text: &[u16],
    format: u32,
    wrap_w: u32,
) -> Option<(usize, u32, u32)> {
    let key = (
        text.to_vec(),
        format & !(DT_VCENTER | DT_BOTTOM | DT_NOCLIP),
        wrap_w,
    );
    let mut cache = super::lock(&f.cache);
    if let Some(c) = cache.get(&key) {
        return Some((c.tex, c.w, c.h));
    }
    if cache.len() > 256 {
        for (_, c) in cache.drain() {
            com_call!(c.tex, 2, fn() -> u32);
        }
    }
    let flags = format & !(DT_VCENTER | DT_BOTTOM | DT_NOCLIP | DT_CALCRECT);
    let mut r = [0, 0, wrap_w as i32, 0];
    draw_text_gdi(f.hdc, text, &mut r, flags | DT_CALCRECT);
    let (tw, th) = (
        (r[2] - r[0]).clamp(1, 4096) as u32,
        (r[3] - r[1]).clamp(1, 4096) as u32,
    );
    // DIB 32 bits de haut en bas, texte blanc sur noir.
    let mut bih = [0u8; 40];
    bih[0..4].copy_from_slice(&40u32.to_le_bytes());
    bih[4..8].copy_from_slice(&(tw as i32).to_le_bytes());
    bih[8..12].copy_from_slice(&(-(th as i32)).to_le_bytes());
    bih[12..14].copy_from_slice(&1u16.to_le_bytes());
    bih[14..16].copy_from_slice(&32u16.to_le_bytes());
    let create_dib: unsafe extern "win64" fn(
        usize,
        *const u8,
        u32,
        *mut usize,
        usize,
        u32,
    ) -> usize = std::mem::transmute(api("gdi32", "CreateDIBSection"));
    let select: extern "win64" fn(usize, usize) -> usize =
        std::mem::transmute(api("gdi32", "SelectObject"));
    let delete: extern "win64" fn(usize) -> i32 = std::mem::transmute(api("gdi32", "DeleteObject"));
    let text_color: extern "win64" fn(usize, u32) -> u32 =
        std::mem::transmute(api("gdi32", "SetTextColor"));
    let bk_mode: extern "win64" fn(usize, u32) -> u32 =
        std::mem::transmute(api("gdi32", "SetBkMode"));
    let mut bits: usize = 0;
    let bmp = create_dib(f.hdc, bih.as_ptr(), 0, &mut bits, 0, 0);
    if bmp == 0 || bits == 0 {
        return None;
    }
    std::ptr::write_bytes(bits as *mut u8, 0, tw as usize * th as usize * 4);
    let old = select(f.hdc, bmp);
    text_color(f.hdc, 0x00FF_FFFF);
    bk_mode(f.hdc, 1);
    let mut rr = [0, 0, tw as i32, th as i32];
    draw_text_gdi(f.hdc, text, &mut rr, flags | DT_NOCLIP);
    let px = std::slice::from_raw_parts(bits as *const u8, tw as usize * th as usize * 4);
    let img = Rgba {
        w: tw,
        h: th,
        px: px
            .chunks_exact(4)
            .flat_map(|p| [255, 255, 255, p[0].max(p[1]).max(p[2])])
            .collect(),
    };
    select(f.hdc, old);
    delete(bmp);
    let mut t: usize = 0;
    let hr = com_call!(
        f.dev,
        DEV_CREATE_TEXTURE,
        fn(u32, u32, u32, u32, u32, u32, *mut usize, usize) -> i32,
        tw,
        th,
        1,
        0,
        FMT_A8R8G8B8,
        1,
        &mut t,
        0
    );
    if hr < 0 || t == 0 {
        return None;
    }
    let mut lr = [0u64; 2];
    if com_call!(
        t,
        19,
        fn(u32, *mut u64, usize, u32) -> i32,
        0,
        lr.as_mut_ptr(),
        0,
        0
    ) >= 0
    {
        let (pitch, b) = super::legacy_com::locked_rect(lr.as_ptr() as usize);
        store(&img, FMT_A8R8G8B8, b, pitch);
        com_call!(t, 20, fn(u32) -> i32, 0);
    }
    cache.insert(
        key,
        Cached {
            tex: t,
            w: tw,
            h: th,
        },
    );
    Some((t, tw, th))
}

unsafe fn draw_text(
    this: usize,
    sprite_in: usize,
    text: Vec<u16>,
    rect: *mut i32,
    format: u32,
    color: u32,
) -> i32 {
    let f = font(this);
    let r = if rect.is_null() {
        None
    } else {
        Some(std::ptr::read_unaligned(rect as *const [i32; 4]))
    };
    let wrap_w = match r {
        Some(r) if format & DT_WORDBREAK != 0 => (r[2] - r[0]).max(1) as u32,
        _ => 0,
    };
    if format & DT_CALCRECT != 0 {
        let mut c = [0, 0, wrap_w as i32, 0];
        draw_text_gdi(f.hdc, &text, &mut c, (format & !DT_NOCLIP) | DT_CALCRECT);
        let (tw, th) = (c[2] - c[0], c[3] - c[1]);
        if let Some(mut r) = r {
            r[2] = r[0] + tw;
            r[3] = r[1] + th;
            std::ptr::write_unaligned(rect as *mut [i32; 4], r);
        }
        return th;
    }
    if text.is_empty() {
        return 0;
    }
    let Some((tex, tw, th)) = render_text(f, &text, format, wrap_w) else {
        return 0;
    };
    let (tw, th) = (tw as i32, th as i32);
    let [rl, rt, rr, rb] = r.unwrap_or([0, 0, 0, 0]);
    let x = if format & DT_RIGHT != 0 {
        rr - tw
    } else if format & DT_CENTER != 0 {
        rl + (rr - rl - tw) / 2
    } else {
        rl
    };
    let y = if format & DT_BOTTOM != 0 {
        rb - th
    } else if format & DT_VCENTER != 0 {
        rt + (rb - rt - th) / 2
    } else {
        rt
    };
    // Découpe au rectangle (sauf DT_NOCLIP).
    let mut src = [0, 0, tw, th];
    let (mut px, mut py) = (x, y);
    if r.is_some() && format & DT_NOCLIP == 0 {
        let cl = [x.max(rl), y.max(rt), (x + tw).min(rr), (y + th).min(rb)];
        if cl[2] <= cl[0] || cl[3] <= cl[1] {
            return th;
        }
        src = [cl[0] - x, cl[1] - y, cl[2] - x, cl[3] - y];
        px = cl[0];
        py = cl[1];
    }
    let pos = [px as f32, py as f32, 0.0f32];
    let own = sprite_in == 0;
    let sp = if own { f.sprite } else { sprite_in };
    if own {
        com_call!(
            sp,
            8,
            fn(u32) -> i32,
            SPRITE_ALPHABLEND | SPRITE_SORT_TEXTURE
        );
    }
    com_call!(
        sp,
        9,
        fn(usize, *const i32, *const f32, *const f32, u32) -> i32,
        tex,
        src.as_ptr(),
        std::ptr::null(),
        pos.as_ptr(),
        color
    );
    if own {
        com_call!(sp, 11, fn() -> i32);
    }
    th
}

unsafe extern "win64" fn ft_draw_text_w(
    this: usize,
    sprite: usize,
    s: *const u16,
    count: i32,
    rect: *mut i32,
    format: u32,
    color: u32,
) -> i32 {
    if s.is_null() {
        return 0;
    }
    let n = if count < 0 {
        (0..).take_while(|&i| *s.add(i) != 0).count()
    } else {
        count as usize
    };
    let text = std::slice::from_raw_parts(s, n).to_vec();
    draw_text(this, sprite, text, rect, format, color)
}
unsafe extern "win64" fn ft_draw_text_a(
    this: usize,
    sprite: usize,
    s: *const u8,
    count: i32,
    rect: *mut i32,
    format: u32,
    color: u32,
) -> i32 {
    if s.is_null() {
        return 0;
    }
    let n = if count < 0 {
        (0..).take_while(|&i| *s.add(i) != 0).count()
    } else {
        count as usize
    };
    let text: Vec<u16> = super::decode_ansi(std::slice::from_raw_parts(s, n))
        .encode_utf16()
        .collect();
    draw_text(this, sprite, text, rect, format, color)
}

unsafe extern "win64" fn ft_qi(this: usize, _iid: usize, out: usize) -> i32 {
    if out == 0 {
        return E_NOINTERFACE;
    }
    ft_addref(this);
    put_ptr(out, this);
    S_OK
}
unsafe extern "win64" fn ft_addref(this: usize) -> u32 {
    font(this).refs.fetch_add(1, Ordering::Relaxed) + 1
}
unsafe extern "win64" fn ft_release(this: usize) -> u32 {
    let n = font(this).refs.fetch_sub(1, Ordering::AcqRel) - 1;
    if n == 0 {
        let f = Box::from_raw(this as *mut Font);
        for (_, c) in f.cache.into_inner().unwrap_or_else(|e| e.into_inner()) {
            com_call!(c.tex, 2, fn() -> u32);
        }
        sp_release(f.sprite);
        let delete: extern "win64" fn(usize) -> i32 =
            std::mem::transmute(api("gdi32", "DeleteObject"));
        let delete_dc: extern "win64" fn(usize) -> i32 =
            std::mem::transmute(api("gdi32", "DeleteDC"));
        delete_dc(f.hdc);
        delete(f.hfont);
        com_call!(f.dev, DEV_RELEASE, fn() -> u32);
    }
    n
}
unsafe extern "win64" fn ft_get_device(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let d = font(this).dev;
    com_call!(d, DEV_ADDREF, fn() -> u32);
    put_ptr(out, d);
    S_OK
}
unsafe fn write_desc(d: &FontDesc, out: usize, wide: bool) {
    wr32(out, d.height as u32);
    wr32(out + 4, d.width);
    wr32(out + 8, d.weight);
    wr32(out + 12, d.mips);
    wr32(out + 16, d.italic);
    for (i, b) in [d.charset, d.out_precision, d.quality, d.pitch]
        .iter()
        .enumerate()
    {
        *((out + 20 + i) as *mut u8) = *b;
    }
    if wide {
        let w: Vec<u16> = d
            .face
            .encode_utf16()
            .take(31)
            .chain(std::iter::once(0))
            .collect();
        std::ptr::copy_nonoverlapping(w.as_ptr(), (out + 24) as *mut u16, w.len());
    } else {
        super::legacy_com::put_str(out + 24, 32, &d.face);
    }
}
unsafe extern "win64" fn ft_get_desc_a(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    write_desc(&font(this).desc, out, false);
    S_OK
}
unsafe extern "win64" fn ft_get_desc_w(this: usize, out: usize) -> i32 {
    if out == 0 {
        return D3DERR_INVALIDCALL;
    }
    write_desc(&font(this).desc, out, true);
    S_OK
}
unsafe extern "win64" fn ft_get_tm_a(this: usize, out: usize) -> i32 {
    let f: unsafe extern "win64" fn(usize, usize) -> i32 =
        std::mem::transmute(api("gdi32", "GetTextMetricsA"));
    i32::from(f(font(this).hdc, out) != 0)
}
unsafe extern "win64" fn ft_get_tm_w(this: usize, out: usize) -> i32 {
    let f: unsafe extern "win64" fn(usize, usize) -> i32 =
        std::mem::transmute(api("gdi32", "GetTextMetricsW"));
    i32::from(f(font(this).hdc, out) != 0)
}
unsafe extern "win64" fn ft_get_dc(this: usize) -> usize {
    font(this).hdc
}
unsafe extern "win64" fn ft_get_glyph_data(
    _this: usize,
    _glyph: u32,
    tex: usize,
    _rect: usize,
    _origin: usize,
) -> i32 {
    put_ptr(tex, 0);
    D3DERR_NOTAVAILABLE
}
unsafe extern "win64" fn ok2(_this: usize, _a: usize, _b: usize) -> i32 {
    S_OK
}
unsafe extern "win64" fn ft_reset(this: usize) -> i32 {
    // Les textures du cache survivent à Reset (pool géré), rien à refaire.
    let _ = this;
    S_OK
}

fn font_vtable() -> usize {
    let a = |f: *const (), n: u8| (f as usize, n);
    vtable(
        "ID3DXFont",
        &[
            a(ft_qi as *const (), 2),
            a(ft_addref as *const (), 0),
            a(ft_release as *const (), 0),
            a(ft_get_device as *const (), 1),
            a(ft_get_desc_a as *const (), 1),
            a(ft_get_desc_w as *const (), 1),
            a(ft_get_tm_a as *const (), 1),
            a(ft_get_tm_w as *const (), 1),
            a(ft_get_dc as *const (), 0),
            a(ft_get_glyph_data as *const (), 4),
            a(ok2 as *const (), 2), // PreloadCharacters(first, last)
            a(ok2 as *const (), 2), // PreloadGlyphs(first, last)
            a(ok2 as *const (), 2), // PreloadTextA(str, count)
            a(ok2 as *const (), 2), // PreloadTextW(str, count)
            a(ft_draw_text_a as *const (), 6),
            a(ft_draw_text_w as *const (), 6),
            a(ft_reset as *const (), 0), // OnLostDevice
            a(ft_reset as *const (), 0), // OnResetDevice
        ],
    )
}

unsafe fn new_font(dev: usize, desc: FontDesc, out: usize) -> i32 {
    if dev == 0 || out == 0 {
        return D3DERR_INVALIDCALL;
    }
    let (hdc, hfont) = gdi_dc_and_font(&desc);
    if hdc == 0 {
        return E_OUTOFMEMORY;
    }
    com_call!(dev, DEV_ADDREF, fn() -> u32);
    let f = Box::into_raw(Box::new(Font {
        vtbl: font_vtable(),
        refs: AtomicU32::new(1),
        dev,
        desc,
        hdc,
        hfont,
        sprite: new_sprite(dev),
        cache: Mutex::new(HashMap::new()),
    }));
    put_ptr(out, f as usize);
    S_OK
}

unsafe extern "win64" fn create_font_w(
    dev: usize,
    height: i32,
    width: u32,
    weight: u32,
    mips: u32,
    italic: i32,
    charset: u32,
    out_precision: u32,
    quality: u32,
    pitch: u32,
    face: *const u16,
    out: usize,
) -> i32 {
    let face = if face.is_null() {
        String::new()
    } else {
        super::wstr(face)
    };
    new_font(
        dev,
        FontDesc {
            height,
            width,
            weight,
            mips,
            italic: italic as u32,
            charset: charset as u8,
            out_precision: out_precision as u8,
            quality: quality as u8,
            pitch: pitch as u8,
            face,
        },
        out,
    )
}
unsafe extern "win64" fn create_font_a(
    dev: usize,
    height: i32,
    width: u32,
    weight: u32,
    mips: u32,
    italic: i32,
    charset: u32,
    out_precision: u32,
    quality: u32,
    pitch: u32,
    face: *const u8,
    out: usize,
) -> i32 {
    let face = if face.is_null() {
        String::new()
    } else {
        super::cstr(face)
    };
    new_font(
        dev,
        FontDesc {
            height,
            width,
            weight,
            mips,
            italic: italic as u32,
            charset: charset as u8,
            out_precision: out_precision as u8,
            quality: quality as u8,
            pitch: pitch as u8,
            face,
        },
        out,
    )
}
unsafe fn read_desc(p: usize, wide: bool) -> FontDesc {
    let b = |o: usize| *((p + o) as *const u8);
    FontDesc {
        height: rd32(p) as i32,
        width: rd32(p + 4),
        weight: rd32(p + 8),
        mips: rd32(p + 12),
        italic: rd32(p + 16),
        charset: b(20),
        out_precision: b(21),
        quality: b(22),
        pitch: b(23),
        face: if wide {
            let s = std::slice::from_raw_parts((p + 24) as *const u16, 32);
            let n = s.iter().position(|&c| c == 0).unwrap_or(32);
            String::from_utf16_lossy(&s[..n])
        } else {
            let s = std::slice::from_raw_parts((p + 24) as *const u8, 32);
            let n = s.iter().position(|&c| c == 0).unwrap_or(32);
            super::decode_ansi(&s[..n])
        },
    }
}
unsafe extern "win64" fn create_font_indirect_a(dev: usize, desc: usize, out: usize) -> i32 {
    if desc == 0 {
        return D3DERR_INVALIDCALL;
    }
    new_font(dev, read_desc(desc, false), out)
}
unsafe extern "win64" fn create_font_indirect_w(dev: usize, desc: usize, out: usize) -> i32 {
    if desc == 0 {
        return D3DERR_INVALIDCALL;
    }
    new_font(dev, read_desc(desc, true), out)
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "D3DXCreateSprite" => fn create_sprite,
        "D3DXCreateFontA" => fn create_font_a,
        "D3DXCreateFontW" => fn create_font_w,
        "D3DXCreateFontIndirectA" => fn create_font_indirect_a,
        "D3DXCreateFontIndirectW" => fn create_font_indirect_w,
    }
}
