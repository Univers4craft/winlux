//! SetBoundsRect et GetBoundsRect : rectangle d'accumulation d'un DC (#3216).
//!
//! Provenance : pages Learn de SetBoundsRect et GetBoundsRect (paramètres,
//! valeurs de retour, remarque « DCB_SET = DCB_ACCUMULATE | DCB_RESET ») ;
//! valeurs numériques des DCB_* tirées de windows-rs 0.58 (MIT,
//! Win32/Graphics/Gdi).
//!
//! Documenté : activation et désactivation (désactivée par défaut), remise à
//! vide, union d'un rectangle de l'application, état précédent rendu par
//! SetBoundsRect, rectangle rendu et vidé par GetBoundsRect, 0 pour un DC
//! invalide.
//!
//! Inférences, faute de texte Learn :
//! - les tracés GDI n'agrandissent pas encore le rectangle : il ne contient
//!   que ce que l'application y met elle-même ;
//! - DCB_ENABLE et DCB_DISABLE ensemble : DCB_DISABLE l'emporte ;
//! - un rectangle vide passé avec DCB_ACCUMULATE ne change rien ;
//! - GetBoundsRect rend, comme SetBoundsRect, l'état d'activation combiné à
//!   DCB_SET ou DCB_RESET (Learn cite les quatre valeurs).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::dlls::gui::{self, Rect};

pub const DCB_RESET: u32 = 1;
pub const DCB_ACCUMULATE: u32 = 2;
pub const DCB_SET: u32 = DCB_RESET | DCB_ACCUMULATE;
pub const DCB_ENABLE: u32 = 4;
pub const DCB_DISABLE: u32 = 8;

#[derive(Clone, Copy, Default)]
struct Bounds {
    enabled: bool,
    rect: Option<Rect>,
}

fn bounds() -> &'static Mutex<HashMap<usize, Bounds>> {
    static B: OnceLock<Mutex<HashMap<usize, Bounds>>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(HashMap::new()))
}

fn state(b: &Bounds) -> u32 {
    (if b.enabled { DCB_ENABLE } else { DCB_DISABLE })
        | if b.rect.is_some() { DCB_SET } else { DCB_RESET }
}

fn union(a: Option<Rect>, r: Rect) -> Option<Rect> {
    if r.right <= r.left || r.bottom <= r.top {
        return a;
    }
    Some(match a {
        None => r,
        Some(a) => Rect {
            left: a.left.min(r.left),
            top: a.top.min(r.top),
            right: a.right.max(r.right),
            bottom: a.bottom.max(r.bottom),
        },
    })
}

/// SetBoundsRect(hdc, lprect, flags) : état précédent, 0 si le DC n'existe pas.
pub unsafe extern "win64" fn set_bounds_rect(hdc: usize, rect: *const Rect, flags: u32) -> u32 {
    if gui::dc_window(hdc).is_none() {
        return 0;
    }
    let mut map = crate::dlls::lock(bounds());
    let b = map.entry(hdc).or_default();
    let prev = state(b);
    if flags & DCB_RESET != 0 {
        b.rect = None;
    }
    if flags & DCB_ACCUMULATE != 0 && !rect.is_null() {
        b.rect = union(b.rect, *rect);
    }
    if flags & DCB_ENABLE != 0 {
        b.enabled = true;
    }
    if flags & DCB_DISABLE != 0 {
        b.enabled = false;
    }
    prev
}

/// GetBoundsRect(hdc, lprect, flags) : rectangle courant (vide s'il n'y en a
/// pas), vidé ensuite si `flags` porte DCB_RESET ; 0 si le DC n'existe pas.
pub unsafe extern "win64" fn get_bounds_rect(hdc: usize, rect: *mut Rect, flags: u32) -> u32 {
    if gui::dc_window(hdc).is_none() {
        return 0;
    }
    let mut map = crate::dlls::lock(bounds());
    let b = map.entry(hdc).or_default();
    let st = state(b);
    if !rect.is_null() {
        *rect = b.rect.unwrap_or_default();
    }
    if flags & DCB_RESET != 0 {
        b.rect = None;
    }
    st
}

/// Oublie l'état d'un DC détruit, pour qu'un handle réutilisé reparte de
/// l'état par défaut.
pub fn forget_dc(hdc: usize) {
    crate::dlls::lock(bounds()).remove(&hdc);
}

pub fn exports() -> crate::dlls::Exports {
    crate::exports! {
        "SetBoundsRect" => fn set_bounds_rect,
        "GetBoundsRect" => fn get_bounds_rect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rectangle_d_accumulation_3216() {
        unsafe {
            let dc = crate::dlls::gdi32::create_compatible_dc(0);
            assert_ne!(dc, 0);
            let mut r = Rect::default();
            // Par défaut : désactivé et vide.
            assert_eq!(get_bounds_rect(dc, &mut r, 0), DCB_DISABLE | DCB_RESET);
            assert_eq!(r, Rect::default());
            // Activation : l'état précédent est rendu.
            assert_eq!(
                set_bounds_rect(dc, std::ptr::null(), DCB_ENABLE | DCB_RESET),
                DCB_DISABLE | DCB_RESET
            );
            let a = Rect {
                left: 10,
                top: 20,
                right: 30,
                bottom: 40,
            };
            let b = Rect {
                left: 0,
                top: 30,
                right: 15,
                bottom: 50,
            };
            assert_eq!(
                set_bounds_rect(dc, &a, DCB_ACCUMULATE),
                DCB_ENABLE | DCB_RESET
            );
            assert_eq!(
                set_bounds_rect(dc, &b, DCB_ACCUMULATE),
                DCB_ENABLE | DCB_SET
            );
            assert_eq!(get_bounds_rect(dc, &mut r, 0), DCB_ENABLE | DCB_SET);
            assert_eq!(
                r,
                Rect {
                    left: 0,
                    top: 20,
                    right: 30,
                    bottom: 50
                }
            );
            // DCB_ACCUMULATE | DCB_RESET (= DCB_SET) remplace le rectangle.
            set_bounds_rect(dc, &a, DCB_SET);
            get_bounds_rect(dc, &mut r, 0);
            assert_eq!(r, a);
            // GetBoundsRect(DCB_RESET) rend puis vide.
            assert_eq!(get_bounds_rect(dc, &mut r, DCB_RESET), DCB_ENABLE | DCB_SET);
            assert_eq!(r, a);
            assert_eq!(get_bounds_rect(dc, &mut r, 0), DCB_ENABLE | DCB_RESET);
            assert_eq!(
                set_bounds_rect(dc, std::ptr::null(), DCB_DISABLE),
                DCB_ENABLE | DCB_RESET
            );
            assert_eq!(get_bounds_rect(dc, &mut r, 0), DCB_DISABLE | DCB_RESET);
            crate::dlls::gdi32::delete_dc(dc);
            // DC détruit : échec, valeur 0.
            assert_eq!(set_bounds_rect(dc, &a, DCB_ACCUMULATE), 0);
            assert_eq!(get_bounds_rect(dc, &mut r, 0), 0);
        }
    }
}
