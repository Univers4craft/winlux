//! Contrôles communs pour les programmes x86 (#232, étape 6).
//!
//! Les contrôles de comctl32 (liste, arbre, barre d'outils, onglets,
//! en-têtes, info-bulles) reçoivent leurs éléments par messages, avec des
//! structures pleines de pointeurs (LVITEM, TVINSERTSTRUCT, TBBUTTON,
//! TCITEM…), et préviennent leur parent par WM_NOTIFY avec NMHDR et ses
//! extensions. Ce module convertit ces structures dans les deux sens ; les
//! passerelles de messages de `user.rs` l'appellent :
//!
//! - `convert_message` : message x86 vers un contrôle du runtime ;
//! - `convert_to_x86` : notification d'un contrôle vers une fenêtre x86
//!   (WM_NOTIFY, WM_DRAWITEM, WM_MEASUREITEM, WM_DELETEITEM,
//!   WM_COMPAREITEM).
//!
//! On ne recopie jamais plus d'octets que la structure x86 n'en a : la
//! taille vient de son champ cbSize, ou de son masque (champs ajoutés par
//! les versions successives de comctl32).

use super::layout::{widen, Layout, F};
use super::rd;
use std::sync::OnceLock;

use F::{Size, B, D, N, P, W};

/// Message converti (voir le fil user32 pour l'appel).
pub(super) struct Converted {
    pub wparam: usize,
    pub lparam: isize,
    /// Reçoit le résultat de l'appel, recopie les structures et rend le
    /// résultat à donner à l'appelant.
    pub done: Option<Box<dyn FnOnce(isize) -> isize>>,
}

// ---------------------------------------------------------------------
// Dispositions
// ---------------------------------------------------------------------

/// Champs de LVITEM jusqu'à iIndent (40 octets x86).
const LVITEM_CORE: &[F] = &[
    D(0, 0),
    D(4, 4),
    D(8, 8),
    D(12, 12),
    D(16, 16),
    P(20, 24),
    D(24, 32),
    D(28, 36),
    P(32, 40),
    D(36, 48),
];
/// Champs ajoutés par comctl32 5.01 et Vista (groupes, colonnes).
const LVITEM_EXTRA: &[F] = &[D(40, 52), D(44, 56), P(48, 64), P(52, 72), D(56, 80)];

const TVITEM_CORE: &[F] = &[
    D(0, 0),
    P(4, 8),
    D(8, 16),
    D(12, 20),
    P(16, 24),
    D(20, 32),
    D(24, 36),
    D(28, 40),
    D(32, 44),
    P(36, 48),
];
/// TVITEMEX : iIntegral, uStateEx, hwnd, iExpandedImage, iReserved.
const TVITEM_EXTRA: &[F] = &[D(40, 56), D(44, 60), P(48, 64), D(52, 72), D(56, 76)];

/// LVCOLUMN par tranches : jusqu'à iSubItem, iImage/iOrder, largeurs Vista.
const LVCOLUMN_FIELDS: &[F] = &[
    D(0, 0),
    D(4, 4),
    D(8, 8),
    P(12, 16),
    D(16, 24),
    D(20, 28),
    D(24, 32),
    D(28, 36),
    D(32, 40),
    D(36, 44),
    D(40, 48),
];

static LVFINDINFO: Layout = Layout {
    size32: 24,
    size64: 40,
    fields: &[D(0, 0), P(4, 8), P(8, 16), N(12, 24, 8), D(20, 32)],
};

static TCITEM: Layout = Layout {
    size32: 28,
    size64: 40,
    fields: &[
        D(0, 0),
        D(4, 4),
        D(8, 8),
        P(12, 16),
        D(16, 24),
        D(20, 28),
        P(24, 32),
    ],
};

/// HDITEM : 36 octets jusqu'à iOrder, puis type/pvFilter (IE5) et state.
const HDITEM_FIELDS: &[F] = &[
    D(0, 0),
    D(4, 4),
    P(8, 8),
    P(12, 16),
    D(16, 24),
    D(20, 28),
    P(24, 32),
    D(28, 40),
    D(32, 44),
    D(36, 48),
    P(40, 56),
    D(44, 64),
];

static TBBUTTON: Layout = Layout {
    size32: 20,
    size64: 32,
    fields: &[D(0, 0), D(4, 4), B(8, 8), B(9, 9), P(12, 16), P(16, 24)],
};

static TBADDBITMAP: Layout = Layout {
    size32: 8,
    size64: 16,
    fields: &[P(0, 0), P(4, 8)],
};

static TBBUTTONINFO: Layout = Layout {
    size32: 32,
    size64: 48,
    fields: &[
        Size(0, 0),
        D(4, 4),
        D(8, 8),
        D(12, 12),
        B(16, 16),
        B(17, 17),
        W(18, 18),
        P(20, 24),
        P(24, 32),
        D(28, 40),
    ],
};

/// TOOLINFO jusqu'à lpReserved (cbSize donne la version).
static TOOLINFO: Layout = Layout {
    size32: 48,
    size64: 72,
    fields: &[
        Size(0, 0),
        D(4, 4),
        P(8, 8),
        P(12, 16),
        N(16, 24, 16),
        P(32, 40),
        P(36, 48),
        P(40, 56),
        P(44, 64),
    ],
};

/// Décale une liste de champs (structure incluse dans une autre).
fn shifted(fields: &[F], d32: u16, d64: u16) -> impl Iterator<Item = F> + '_ {
    fields.iter().map(move |f| match *f {
        D(a, b) => D(a + d32, b + d64),
        W(a, b) => W(a + d32, b + d64),
        B(a, b) => B(a + d32, b + d64),
        P(a, b) => P(a + d32, b + d64),
        N(a, b, n) => N(a + d32, b + d64, n),
        Size(a, b) => Size(a + d32, b + d64),
    })
}

/// Disposition construite une fois (parties décalées mises bout à bout).
fn build(
    cell: &'static OnceLock<Layout>,
    size32: usize,
    size64: usize,
    parts: &[(&[F], u16, u16)],
) -> &'static Layout {
    cell.get_or_init(|| {
        let fields: Vec<F> = parts
            .iter()
            .flat_map(|(f, a, b)| shifted(f, *a, *b))
            .collect();
        Layout {
            size32,
            size64,
            fields: Box::leak(fields.into_boxed_slice()),
        }
    })
}

const NMHDR: &[F] = &[P(0, 0), P(4, 8), D(8, 16)];

fn lvitem(full: bool) -> &'static Layout {
    static CORE: OnceLock<Layout> = OnceLock::new();
    static FULL: OnceLock<Layout> = OnceLock::new();
    if full {
        build(&FULL, 60, 88, &[(LVITEM_CORE, 0, 0), (LVITEM_EXTRA, 0, 0)])
    } else {
        build(&CORE, 40, 52, &[(LVITEM_CORE, 0, 0)])
    }
}

fn tvitem(full: bool) -> &'static Layout {
    static CORE: OnceLock<Layout> = OnceLock::new();
    static FULL: OnceLock<Layout> = OnceLock::new();
    if full {
        build(&FULL, 60, 80, &[(TVITEM_CORE, 0, 0), (TVITEM_EXTRA, 0, 0)])
    } else {
        build(&CORE, 40, 56, &[(TVITEM_CORE, 0, 0)])
    }
}

fn tvinsert(full: bool) -> &'static Layout {
    static CORE: OnceLock<Layout> = OnceLock::new();
    static FULL: OnceLock<Layout> = OnceLock::new();
    let head: &[F] = &[P(0, 0), P(4, 8)];
    if full {
        build(
            &FULL,
            68,
            96,
            &[(head, 0, 0), (TVITEM_CORE, 8, 16), (TVITEM_EXTRA, 8, 16)],
        )
    } else {
        build(&CORE, 48, 72, &[(head, 0, 0), (TVITEM_CORE, 8, 16)])
    }
}

/// LVCOLUMN limitée aux champs que le masque annonce.
fn lvcolumn(mask: u32) -> &'static Layout {
    static L: [OnceLock<Layout>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let (i, n, s32, s64) = if mask & 0x1c0 != 0 {
        (2, 11, 44, 56)
    } else if mask & 0x30 != 0 {
        (1, 8, 32, 40)
    } else {
        (0, 6, 24, 32)
    };
    build(&L[i], s32, s64, &[(&LVCOLUMN_FIELDS[..n], 0, 0)])
}

fn hditem(mask: u32) -> &'static Layout {
    static L: [OnceLock<Layout>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
    let (i, n, s32, s64) = if mask & 0x400 != 0 {
        (2, 12, 48, 72)
    } else if mask & 0x100 != 0 {
        (1, 11, 44, 64)
    } else {
        (0, 9, 36, 48)
    };
    build(&L[i], s32, s64, &[(&HDITEM_FIELDS[..n], 0, 0)])
}

/// LVITEM complet si le masque demande les champs récents (groupes,
/// colonnes, formats de colonnes).
fn lvitem_for(ptr: u32) -> &'static Layout {
    lvitem(ptr != 0 && rd(ptr) & (0x100 | 0x200 | 0x1_0000) != 0)
}

fn tvitem_for(ptr: u32) -> &'static Layout {
    tvitem(ptr != 0 && rd(ptr) & (0x80 | 0x100 | 0x200) != 0)
}

// ---------------------------------------------------------------------
// x86 → contrôle du runtime
// ---------------------------------------------------------------------

/// Structure x86 à `ptr` (`given` octets) passée en 64 bits ; recopiée au
/// retour si `back`.
fn struct_in(
    layout: &'static Layout,
    ptr: u32,
    given: usize,
    wparam: usize,
    back: bool,
) -> Converted {
    let given = given.min(layout.size32);
    let mut x86 = vec![0u8; layout.size32 + 8];
    unsafe {
        std::ptr::copy_nonoverlapping(ptr as usize as *const u8, x86.as_mut_ptr(), given);
    }
    let mut wide = unsafe { layout.boxed64(x86.as_ptr() as usize) };
    let lparam = wide.as_mut_ptr() as isize;
    Converted {
        wparam,
        lparam,
        done: Some(Box::new(move |r| {
            if back {
                unsafe {
                    layout.to32(wide.as_ptr() as *const u8, x86.as_mut_ptr() as usize);
                    std::ptr::copy_nonoverlapping(x86.as_ptr(), ptr as usize as *mut u8, given);
                }
            }
            drop(wide);
            r
        })),
    }
}

/// Tableau de `count` TBBUTTON x86 en 64 bits.
fn tbbuttons(ptr: u32, count: u32) -> Vec<u64> {
    let count = count.min(4096) as usize;
    let mut out = vec![0u64; count * TBBUTTON.size64 / 8 + 1];
    for i in 0..count {
        unsafe {
            TBBUTTON.to64(
                ptr as usize + i * TBBUTTON.size32,
                (out.as_mut_ptr() as *mut u8).add(i * TBBUTTON.size64),
            )
        };
    }
    out
}

fn keep(wparam: usize, lparam: isize, buf: Vec<u64>) -> Converted {
    Converted {
        wparam,
        lparam,
        done: Some(Box::new(move |r| {
            drop(buf);
            r
        })),
    }
}

/// Classe (en minuscules) de la fenêtre visée.
fn class_of(hwnd: usize) -> String {
    crate::dlls::gui::window_class(hwnd)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

/// Message x86 vers un contrôle commun : structures converties en 64
/// bits. `None` : rien à convertir (le message passe tel quel).
pub(super) fn convert_message(
    hwnd: usize,
    msg: u32,
    wparam: u32,
    lparam: u32,
) -> Option<Converted> {
    // Tous les messages ci-dessous passent une structure par lParam (sauf
    // TB_BUTTONSTRUCTSIZE, traité à part).
    if msg < 0x400 {
        return None;
    }
    let class = class_of(hwnd);
    let wp = widen(wparam) as usize;
    match class.as_str() {
        "syslistview32" => match msg {
            // GETITEM, SETITEM, INSERTITEM (A/W), SETITEMSTATE,
            // GETITEMTEXT, SETITEMTEXT (A/W).
            0x1005 | 0x104B | 0x102D | 0x1073 if lparam != 0 => {
                let l = lvitem_for(lparam);
                Some(struct_in(l, lparam, l.size32, wp, true))
            }
            0x1006 | 0x1007 | 0x104C | 0x104D | 0x102B | 0x102E | 0x1074 if lparam != 0 => {
                let l = lvitem_for(lparam);
                Some(struct_in(l, lparam, l.size32, wp, false))
            }
            // FINDITEM A/W.
            0x100D | 0x1053 if lparam != 0 => Some(struct_in(&LVFINDINFO, lparam, 24, wp, false)),
            // GETCOLUMN, SETCOLUMN, INSERTCOLUMN (A/W).
            0x1019 | 0x105F | 0x101A | 0x1060 | 0x101B | 0x1061 if lparam != 0 => {
                let l = lvcolumn(rd(lparam));
                let back = matches!(msg, 0x1019 | 0x105F);
                Some(struct_in(l, lparam, l.size32, wp, back))
            }
            // SORTITEMS, SORTITEMSEX : fonction de comparaison x86.
            0x1030 | 0x1051 if lparam != 0 => Some(Converted {
                wparam: wp,
                lparam: super::callback64(lparam) as isize,
                done: None,
            }),
            _ => None,
        },
        "systreeview32" => match msg {
            0x1100 | 0x1132 if lparam != 0 => {
                let full = rd(lparam + 8) & (0x80 | 0x100 | 0x200) != 0;
                let l = tvinsert(full);
                Some(struct_in(l, lparam, l.size32, wp, false))
            }
            0x110C | 0x113E if lparam != 0 => {
                let l = tvitem_for(lparam);
                Some(struct_in(l, lparam, l.size32, wp, true))
            }
            0x110D | 0x113F if lparam != 0 => {
                let l = tvitem_for(lparam);
                Some(struct_in(l, lparam, l.size32, wp, false))
            }
            // GETITEMRECT : le RECT reçoit d'abord le HTREEITEM.
            0x1104 if lparam != 0 => {
                let mut buf = vec![0u64; 3];
                buf[0] = widen(rd(lparam));
                let lp = buf.as_mut_ptr() as isize;
                Some(Converted {
                    wparam: wp,
                    lparam: lp,
                    done: Some(Box::new(move |r| {
                        unsafe {
                            std::ptr::copy_nonoverlapping(
                                buf.as_ptr() as *const u8,
                                lparam as usize as *mut u8,
                                16,
                            )
                        };
                        r
                    })),
                })
            }
            // SORTCHILDRENCB : TVSORTCB { hParent, lpfnCompare, lParam }.
            0x1115 if lparam != 0 => {
                let buf = vec![
                    widen(rd(lparam)),
                    super::callback64(rd(lparam + 4)) as u64,
                    widen(rd(lparam + 8)),
                ];
                Some(keep(wp, buf.as_ptr() as isize, buf))
            }
            _ => None,
        },
        "systabcontrol32" => match msg {
            0x1305 | 0x133C if lparam != 0 => Some(struct_in(&TCITEM, lparam, 28, wp, true)),
            // SETITEM, INSERTITEM : TCITEMHEADER (24 octets) ou TCITEM.
            0x1306 | 0x133D | 0x1307 | 0x133E if lparam != 0 => {
                let given = if rd(lparam) & 0x8 != 0 { 28 } else { 24 };
                Some(struct_in(&TCITEM, lparam, given, wp, false))
            }
            _ => None,
        },
        "sysheader32" => match msg {
            0x1203 | 0x120B if lparam != 0 => {
                let l = hditem(rd(lparam));
                Some(struct_in(l, lparam, l.size32, wp, true))
            }
            0x1201 | 0x120A | 0x1204 | 0x120C if lparam != 0 => {
                let l = hditem(rd(lparam));
                Some(struct_in(l, lparam, l.size32, wp, false))
            }
            _ => None,
        },
        "toolbarwindow32" => match msg {
            // TB_BUTTONSTRUCTSIZE : sizeof(TBBUTTON) x86 → x64.
            0x41E => Some(Converted {
                wparam: TBBUTTON.size64,
                lparam: lparam as isize,
                done: None,
            }),
            // ADDBUTTONS A/W (wParam boutons), INSERTBUTTON A/W (un seul).
            0x414 | 0x444 if lparam != 0 => {
                let buf = tbbuttons(lparam, wparam);
                Some(keep(wp, buf.as_ptr() as isize, buf))
            }
            0x415 | 0x443 if lparam != 0 => {
                let buf = tbbuttons(lparam, 1);
                Some(keep(wp, buf.as_ptr() as isize, buf))
            }
            0x417 if lparam != 0 => Some(struct_in(&TBBUTTON, lparam, 20, wp, true)),
            0x413 if lparam != 0 => Some(struct_in(&TBADDBITMAP, lparam, 8, wp, false)),
            0x43F | 0x441 if lparam != 0 => Some(struct_in(
                &TBBUTTONINFO,
                lparam,
                rd(lparam) as usize,
                wp,
                true,
            )),
            0x440 | 0x442 if lparam != 0 => Some(struct_in(
                &TBBUTTONINFO,
                lparam,
                rd(lparam) as usize,
                wp,
                false,
            )),
            _ => None,
        },
        "tooltips_class32" => match msg {
            // GETTOOLINFO A/W, ENUMTOOLS A/W, GETCURRENTTOOL A/W : recopiés.
            0x408 | 0x435 | 0x40E | 0x43A | 0x40F | 0x43B if lparam != 0 => {
                Some(struct_in(&TOOLINFO, lparam, rd(lparam) as usize, wp, true))
            }
            // ADDTOOL, DELTOOL, NEWTOOLRECT, SETTOOLINFO, UPDATETIPTEXT (A/W).
            0x404 | 0x432 | 0x405 | 0x433 | 0x406 | 0x434 | 0x409 | 0x436 | 0x40C | 0x439
                if lparam != 0 =>
            {
                Some(struct_in(&TOOLINFO, lparam, rd(lparam) as usize, wp, false))
            }
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------
// Contrôle du runtime → fenêtre x86
// ---------------------------------------------------------------------

/// Structure 64 bits à `ptr` donnée au code x86 ; recopiée au retour si
/// `back`. Le tampon x86 a de la marge : un programme qui lit une
/// extension que le contrôle n'envoie pas lit des zéros.
fn struct_out(layout: &'static Layout, ptr: usize, wparam: usize, back: bool) -> Converted {
    let mut x86 = vec![0u64; (layout.size32 + 256) / 8];
    unsafe { layout.to32(ptr as *const u8, x86.as_mut_ptr() as usize) };
    let lparam = x86.as_mut_ptr() as isize;
    Converted {
        wparam,
        lparam,
        done: Some(Box::new(move |r| {
            if back {
                unsafe { layout.to64(x86.as_ptr() as usize, ptr as *mut u8) };
            }
            drop(x86);
            r
        })),
    }
}

/// Disposition de la notification `code` (NMHDR et son extension) et si
/// le parent peut la modifier.
fn notify_layout(code: i32) -> (&'static Layout, bool) {
    static NM: OnceLock<Layout> = OnceLock::new();
    static NMLISTVIEW: OnceLock<Layout> = OnceLock::new();
    static NMLVDISPINFO: OnceLock<Layout> = OnceLock::new();
    static NMKEYDOWN: OnceLock<Layout> = OnceLock::new();
    static NMTREEVIEW: OnceLock<Layout> = OnceLock::new();
    static NMTVDISPINFO: OnceLock<Layout> = OnceLock::new();
    static NMTOOLBAR: OnceLock<Layout> = OnceLock::new();
    static NMTBHOTITEM: OnceLock<Layout> = OnceLock::new();
    static NMUPDOWN: OnceLock<Layout> = OnceLock::new();
    let nm = &[(NMHDR, 0u16, 0u16)];
    match code {
        // LVN_ITEMCHANGING … BEGINRDRAG : NMLISTVIEW.
        -111..=-100 => {
            let tail: &[F] = &[
                D(12, 24),
                D(16, 28),
                D(20, 32),
                D(24, 36),
                D(28, 40),
                N(32, 44, 8),
                P(40, 56),
            ];
            (
                build(&NMLISTVIEW, 44, 64, &[(NMHDR, 0, 0), (tail, 0, 0)]),
                code == -100,
            )
        }
        // LVN_GETDISPINFO / SETDISPINFO (A/W) : NMHDR + LVITEM.
        -150 | -151 | -177 | -178 => (
            build(
                &NMLVDISPINFO,
                12 + 60,
                24 + 88,
                &[(NMHDR, 0, 0), (LVITEM_CORE, 12, 24), (LVITEM_EXTRA, 12, 24)],
            ),
            true,
        ),
        // LVN_KEYDOWN, TVN_KEYDOWN, TCN_KEYDOWN : compactées (pshpack1).
        -155 | -412 | -550 => {
            let tail: &[F] = &[W(12, 24), D(14, 26)];
            (
                build(&NMKEYDOWN, 18, 30, &[(NMHDR, 0, 0), (tail, 0, 0)]),
                false,
            )
        }
        // TVN_SELCHANGING … DELETEITEM (A et W) : NMTREEVIEW.
        -409..=-401 | -458..=-450 if !matches!(code, -403 | -404 | -452 | -453) => {
            let action: &[F] = &[D(12, 24)];
            let pt: &[F] = &[N(96, 144, 8)];
            (
                build(
                    &NMTREEVIEW,
                    104,
                    152,
                    &[
                        (NMHDR, 0, 0),
                        (action, 0, 0),
                        (TVITEM_CORE, 16, 32),
                        (TVITEM_CORE, 56, 88),
                        (pt, 0, 0),
                    ],
                ),
                false,
            )
        }
        // TVN_GETDISPINFO / SETDISPINFO (A/W) : NMHDR + TVITEM.
        -403 | -404 | -452 | -453 => (
            build(
                &NMTVDISPINFO,
                12 + 40,
                24 + 56,
                &[(NMHDR, 0, 0), (TVITEM_CORE, 12, 24)],
            ),
            true,
        ),
        // TBN_GETBUTTONINFO … DROPDOWN : NMTOOLBAR.
        -720..=-700 if code != -713 => {
            let tail: &[F] = &[D(12, 24), D(36, 64), P(40, 72), N(44, 80, 16)];
            let button: &[F] = TBBUTTON.fields;
            (
                build(
                    &NMTOOLBAR,
                    60,
                    96,
                    &[(NMHDR, 0, 0), (tail, 0, 0), (button, 16, 32)],
                ),
                true,
            )
        }
        // TBN_HOTITEMCHANGE : NMTBHOTITEM.
        -713 => {
            let tail: &[F] = &[D(12, 24), D(16, 28), D(20, 32)];
            (
                build(&NMTBHOTITEM, 24, 40, &[(NMHDR, 0, 0), (tail, 0, 0)]),
                false,
            )
        }
        // UDN_DELTAPOS : NMUPDOWN (le parent peut changer le pas).
        -722 => {
            let tail: &[F] = &[D(12, 24), D(16, 28)];
            (
                build(&NMUPDOWN, 20, 32, &[(NMHDR, 0, 0), (tail, 0, 0)]),
                true,
            )
        }
        _ => (build(&NM, 12, 24, nm), false),
    }
}

/// NMMOUSE des clics sur une barre d'outils ou d'état.
fn nmmouse() -> &'static Layout {
    static L: OnceLock<Layout> = OnceLock::new();
    let tail: &[F] = &[P(12, 24), P(16, 32), N(20, 40, 8), P(28, 48)];
    build(&L, 32, 56, &[(NMHDR, 0, 0), (tail, 0, 0)])
}

static DRAWITEMSTRUCT: Layout = Layout {
    size32: 48,
    size64: 64,
    fields: &[
        D(0, 0),
        D(4, 4),
        D(8, 8),
        D(12, 12),
        D(16, 16),
        P(20, 24),
        P(24, 32),
        N(28, 40, 16),
        P(44, 56),
    ],
};

static MEASUREITEMSTRUCT: Layout = Layout {
    size32: 24,
    size64: 32,
    fields: &[D(0, 0), D(4, 4), D(8, 8), D(12, 12), D(16, 16), P(20, 24)],
};

static DELETEITEMSTRUCT: Layout = Layout {
    size32: 20,
    size64: 32,
    fields: &[D(0, 0), D(4, 4), D(8, 8), P(12, 16), P(16, 24)],
};

static COMPAREITEMSTRUCT: Layout = Layout {
    size32: 32,
    size64: 56,
    fields: &[
        D(0, 0),
        D(4, 4),
        P(8, 8),
        D(12, 16),
        P(16, 24),
        D(20, 32),
        P(24, 40),
        D(28, 48),
    ],
};

/// Message d'un contrôle du runtime vers une fenêtre x86 : structures
/// converties en 32 bits. `done` reçoit le résultat x86 étendu.
pub(super) fn convert_to_x86(
    _hwnd: usize,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> Option<Converted> {
    if lparam == 0 {
        return None;
    }
    let ptr = lparam as usize;
    match msg {
        // WM_NOTIFY
        0x004E => {
            let code = unsafe { ((ptr + 16) as *const i32).read_unaligned() };
            let from = unsafe { (ptr as *const usize).read_unaligned() };
            let (layout, back) = match code {
                // NM_CLICK … NM_RDBLCLK d'une barre d'outils ou d'état.
                -6..=-2
                    if matches!(
                        class_of(from).as_str(),
                        "toolbarwindow32" | "msctls_statusbar32"
                    ) =>
                {
                    (nmmouse(), false)
                }
                _ => notify_layout(code),
            };
            Some(struct_out(layout, ptr, wparam, back))
        }
        // WM_DRAWITEM, WM_MEASUREITEM, WM_DELETEITEM, WM_COMPAREITEM
        0x002B => Some(struct_out(&DRAWITEMSTRUCT, ptr, wparam, false)),
        0x002C => Some(struct_out(&MEASUREITEMSTRUCT, ptr, wparam, true)),
        0x002D => Some(struct_out(&DELETEITEMSTRUCT, ptr, wparam, false)),
        0x0039 => Some(struct_out(&COMPAREITEMSTRUCT, ptr, wparam, false)),
        _ => None,
    }
}

// ---------------------------------------------------------------------
// Fonctions exportées
// ---------------------------------------------------------------------

/// CreateToolbarEx : treize arguments, dont un tableau de TBBUTTON x86.
extern "win64" fn create_toolbar_ex32(args: u32, target: usize) -> u64 {
    let a = |i: u32| rd(args + 4 * i);
    let buttons = a(6);
    let count = a(7);
    let buf = if buttons != 0 {
        tbbuttons(buttons, count)
    } else {
        Vec::new()
    };
    let slots: Vec<u64> = (0..13u32)
        .map(|i| match i {
            6 => {
                if buttons != 0 {
                    buf.as_ptr() as u64
                } else {
                    0
                }
            }
            12 => TBBUTTON.size64 as u64,
            _ => widen(a(i)),
        })
        .collect();
    let mut xmm0 = 0u64;
    let r = unsafe { super::winlux_wow_call64(target, slots.as_ptr(), slots.len(), &mut xmm0) };
    drop(buf);
    r
}

/// Passerelles x86 propres à comctl32.
pub(super) fn override_for(dll: &str, func: &str) -> Option<super::crt::Override> {
    if dll != "comctl32" {
        return None;
    }
    match func {
        "CreateToolbarEx" => Some(super::crt::Override::Raw {
            target: create_toolbar_ex32 as *const () as usize,
            aux: crate::dlls::lookup(dll, func)?,
            pop: Some(13 * 4),
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ends(l: &Layout) -> (usize, usize) {
        let mut e = (0, 0);
        for f in l.fields {
            let (a, b, n32, n64) = match *f {
                D(a, b) | Size(a, b) => (a, b, 4, 4),
                W(a, b) => (a, b, 2, 2),
                B(a, b) => (a, b, 1, 1),
                P(a, b) => (a, b, 4, 8),
                N(a, b, n) => (a, b, n, n),
            };
            e.0 = e.0.max(a as usize + n32 as usize);
            e.1 = e.1.max(b as usize + n64 as usize);
        }
        e
    }

    #[test]
    fn dispositions_dans_leurs_tailles() {
        let all: Vec<&Layout> = vec![
            lvitem(false),
            lvitem(true),
            tvitem(false),
            tvitem(true),
            tvinsert(false),
            tvinsert(true),
            lvcolumn(0),
            lvcolumn(0x10),
            lvcolumn(0x40),
            hditem(0),
            hditem(0x100),
            hditem(0x400),
            &LVFINDINFO,
            &TCITEM,
            &TBBUTTON,
            &TBBUTTONINFO,
            &TOOLINFO,
            &DRAWITEMSTRUCT,
            &MEASUREITEMSTRUCT,
            &DELETEITEMSTRUCT,
            &COMPAREITEMSTRUCT,
            nmmouse(),
        ];
        for l in all {
            let (e32, e64) = ends(l);
            assert!(e32 <= l.size32, "x86 {e32} > {}", l.size32);
            assert!(e64 <= l.size64, "x64 {e64} > {}", l.size64);
        }
        for code in [-101, -150, -155, -402, -452, -403, -710, -713, -722, -2] {
            let (l, _) = notify_layout(code);
            let (e32, e64) = ends(l);
            assert!(e32 <= l.size32 && e64 <= l.size64, "code {code}");
        }
    }

    #[test]
    fn tbbutton_converti() {
        // Mémoire sous 2 Go : les adresses x86 tiennent sur 32 bits.
        let low = super::super::alloc_low(4096, false);
        let x86 = unsafe { std::slice::from_raw_parts_mut(low as *mut u8, 40) };
        x86[0..4].copy_from_slice(&3i32.to_le_bytes());
        x86[4..8].copy_from_slice(&101i32.to_le_bytes());
        x86[8] = 4;
        x86[16..20].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
        x86[20..24].copy_from_slice(&7i32.to_le_bytes());
        let v = tbbuttons(low as u32, 2);
        let b = unsafe { std::slice::from_raw_parts(v.as_ptr() as *const u8, 64) };
        assert_eq!(&b[0..4], &3i32.to_le_bytes());
        assert_eq!(b[8], 4);
        assert_eq!(u64::from_le_bytes(b[24..32].try_into().unwrap()), u64::MAX);
        assert_eq!(&b[32..36], &7i32.to_le_bytes());
    }
}
