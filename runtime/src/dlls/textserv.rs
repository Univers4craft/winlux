//! Text Services sans fenêtre de msftedit.dll (#3268) : CreateTextServices
//! et l'interface ITextServices, servies pour de vrai sur un texte en
//! mémoire. Acrobat la demande au démarrage (huit fois dans la passe « PDF
//! direct ») pour ses champs dessinés sans fenêtre.
//!
//! Sources publiques :
//! - Microsoft Learn, « About Windowless Rich Edit Controls » : msftedit.dll
//!   exporte IID_ITextServices et IID_ITextHost, qu'on obtient par
//!   GetProcAddress ; l'hôte fournit ITextHost, le contrôle ITextServices.
//! - Microsoft Learn, « CreateTextServices function » : punkOuter (agrégation),
//!   pITextHost, ppUnk reçoit l'IUnknown privé de l'objet.
//! - Microsoft Learn, « ITextServices::TxSendMessage » : S_OK si le message
//!   est traité, S_FALSE sinon (l'appelant le traite lui-même).
//! - mingw-w64 lib64/msftedit.def : noms exportés.
//! - windows-rs 0.58 (MIT), Win32/UI/Controls/RichEdit : ordre des tables
//!   virtuelles d'ITextServices et d'ITextHost.
//!
//! Inférences nommées :
//! - la valeur des IID n'est publiée nulle part (windows-rs les laisse à 0) ;
//!   le programme les lit dans la DLL, donc nos propres GUID suffisent tant
//!   que QueryInterface compare avec ces mêmes valeurs ;
//! - l'objet ne prend pas de référence sur l'hôte : l'hôte possède en général
//!   le contrôle, une référence en retour formerait un cycle ;
//! - le texte est dessiné avec la police du DC et les couleurs système de
//!   l'hôte, sans lire TxGetCharFormat (tranche suivante de #3268).
//!
//! Ce qui n'est pas encore servi rend E_NOTIMPL et l'écrit une fois sur
//! stderr avec son fichier et sa ligne.

use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::comobj::{guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, E_NOTIMPL};
use super::comobj::{IID_IUNKNOWN, S_FALSE, S_OK};
use super::gui::Rect;

const E_POINTER: i32 = 0x8000_4003u32 as i32;
const E_OUTOFMEMORY: i32 = 0x8007_000Eu32 as i32;

/// IID exportés par msftedit (valeurs propres à Winlux, voir l'en-tête).
pub static IID_ITEXT_SERVICES: Guid = guid(
    0xe15c_a0d2,
    0xef67,
    0x4fe1,
    [0x92, 0x09, 0x35, 0x1c, 0x5b, 0x67, 0x17, 0x0b],
);
pub static IID_ITEXT_HOST: Guid = guid(
    0xe0f7_df69,
    0xa0ba,
    0x40c9,
    [0xb2, 0x9e, 0xa8, 0xb4, 0x27, 0x94, 0x48, 0x46],
);
pub static IID_ITEXT_HOST2: Guid = guid(
    0xc566_c506,
    0xa262,
    0x43b9,
    [0xbb, 0x9f, 0xb0, 0x3d, 0x9c, 0x5d, 0x3a, 0x99],
);

// Messages traités par TxSendMessage.
const WM_SETTEXT: u32 = 0x000C;
const WM_GETTEXT: u32 = 0x000D;
const WM_GETTEXTLENGTH: u32 = 0x000E;
const EM_LIMITTEXT: u32 = 0x00C5;
const EM_GETEVENTMASK: u32 = 0x043B;
const EM_EXLIMITTEXT: u32 = 0x0435;
const EM_SETEVENTMASK: u32 = 0x0445;
const EM_GETTEXTLENGTHEX: u32 = 0x045F;
const EM_SETTEXTEX: u32 = 0x0461;

const ENM_CHANGE: u32 = 0x0000_0001;
const EN_CHANGE: u32 = 0x0300;
const CP_UNICODE: u32 = 1200;

// Bits de propriété (TXTBIT_*) et styles d'arrière-plan.
const TXTBIT_MULTILINE: u32 = 0x2;
const TXTBACK_OPAQUE: i32 = 1;

const COLOR_WINDOW: i32 = 5;
const COLOR_WINDOWTEXT: i32 = 8;

const DT_WORDBREAK: u32 = 0x10;
const DT_SINGLELINE: u32 = 0x20;
const DT_CALCRECT: u32 = 0x400;
const DT_NOPREFIX: u32 = 0x800;
const TRANSPARENT: u32 = 1;

// Rangs des méthodes d'ITextHost appelées (ordre de windows-rs).
const HOST_GET_DC: usize = 3;
const HOST_RELEASE_DC: usize = 4;
const HOST_INVALIDATE_RECT: usize = 9;
const HOST_VIEW_CHANGE: usize = 10;
const HOST_GET_CLIENT_RECT: usize = 24;
const HOST_GET_SYS_COLOR: usize = 28;
const HOST_GET_BACK_STYLE: usize = 29;
const HOST_GET_PROPERTY_BITS: usize = 37;
const HOST_NOTIFY: usize = 38;

/// Écrit une fois sur stderr qu'une méthode n'est pas servie, avec son
/// emplacement, et rend E_NOTIMPL.
macro_rules! non_servie {
    ($nom:literal) => {{
        static VU: std::sync::Once = std::sync::Once::new();
        VU.call_once(|| {
            eprintln!(
                "winlux[textserv] ITextServices::{} : E_NOTIMPL ({}:{})",
                $nom,
                file!(),
                line!()
            )
        });
        E_NOTIMPL
    }};
}

#[derive(Default)]
struct Etat {
    texte: Vec<u16>,
    masque_evenements: u32,
    bits: u32,
    bits_connus: u32,
    limite: u32,
    actif: bool,
    ui_actif: bool,
    taille: (u32, u32),
}

/// L'objet Text Services : l'IUnknown privé à l'adresse de l'objet,
/// ITextServices juste après.
#[repr(C)]
struct TextServices {
    vtbl_prive: &'static VTable<3>,
    vtbl_services: &'static VTable<21>,
    refs: AtomicU32,
    externe: usize,
    hote: usize,
    etat: Mutex<Etat>,
}

const DECALAGE_SERVICES: usize = std::mem::size_of::<usize>();

unsafe fn de_prive(this: usize) -> &'static TextServices {
    &*(this as *const TextServices)
}
unsafe fn de_services(this: usize) -> &'static TextServices {
    &*((this - DECALAGE_SERVICES) as *const TextServices)
}

impl TextServices {
    fn base(&self) -> usize {
        self as *const Self as usize
    }
    fn services(&self) -> usize {
        self.base() + DECALAGE_SERVICES
    }
    fn etat(&self) -> std::sync::MutexGuard<'_, Etat> {
        self.etat.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Méthode `rang` de la table virtuelle de l'hôte.
    unsafe fn hote_fn(&self, rang: usize) -> *const () {
        let vtbl = *(self.hote as *const *const *const ());
        *vtbl.add(rang)
    }
    unsafe fn hote_couleur(&self, index: i32) -> u32 {
        let f: unsafe extern "win64" fn(usize, i32) -> u32 =
            std::mem::transmute(self.hote_fn(HOST_GET_SYS_COLOR));
        f(self.hote, index)
    }
    unsafe fn hote_opaque(&self) -> bool {
        let f: unsafe extern "win64" fn(usize, *mut i32) -> i32 =
            std::mem::transmute(self.hote_fn(HOST_GET_BACK_STYLE));
        let mut style = TXTBACK_OPAQUE;
        f(self.hote, &mut style) < 0 || style == TXTBACK_OPAQUE
    }
    unsafe fn hote_rect_client(&self) -> Option<Rect> {
        let f: unsafe extern "win64" fn(usize, *mut Rect) -> i32 =
            std::mem::transmute(self.hote_fn(HOST_GET_CLIENT_RECT));
        let mut r = Rect::default();
        (f(self.hote, &mut r) >= 0).then_some(r)
    }
    /// Bits de propriété : ceux que l'hôte a signalés, sinon ceux qu'il rend.
    unsafe fn bits(&self) -> u32 {
        let (bits, connus) = {
            let e = self.etat();
            (e.bits, e.bits_connus)
        };
        if connus == u32::MAX {
            return bits;
        }
        let f: unsafe extern "win64" fn(usize, u32, *mut u32) -> i32 =
            std::mem::transmute(self.hote_fn(HOST_GET_PROPERTY_BITS));
        let mut lus = 0u32;
        if f(self.hote, !connus, &mut lus) < 0 {
            return bits;
        }
        (bits & connus) | (lus & !connus)
    }

    /// Le texte a changé : l'hôte redessine, et reçoit EN_CHANGE si son
    /// masque d'événements le demande. Appelé sans le verrou de l'état.
    unsafe fn texte_change(&self) {
        let inval: unsafe extern "win64" fn(usize, *const Rect, i32) =
            std::mem::transmute(self.hote_fn(HOST_INVALIDATE_RECT));
        inval(self.hote, std::ptr::null(), 1);
        let vue: unsafe extern "win64" fn(usize, i32) =
            std::mem::transmute(self.hote_fn(HOST_VIEW_CHANGE));
        vue(self.hote, 1);
        if self.etat().masque_evenements & ENM_CHANGE != 0 {
            let notif: unsafe extern "win64" fn(usize, u32, *mut c_void) -> i32 =
                std::mem::transmute(self.hote_fn(HOST_NOTIFY));
            notif(self.hote, EN_CHANGE, std::ptr::null_mut());
        }
    }

    fn poser_texte(&self, texte: Vec<u16>) {
        let mut e = self.etat();
        let mut t = texte;
        if e.limite != 0 && t.len() > e.limite as usize {
            t.truncate(e.limite as usize);
        }
        e.texte = t;
    }
}

unsafe fn chaine_large(p: *const u16) -> Vec<u16> {
    let mut v = Vec::new();
    if p.is_null() {
        return v;
    }
    let mut i = 0;
    while *p.add(i) != 0 {
        v.push(*p.add(i));
        i += 1;
    }
    v
}

unsafe fn chaine_ansi(p: *const u8) -> Vec<u16> {
    if p.is_null() {
        return Vec::new();
    }
    let s = std::ffi::CStr::from_ptr(p as *const std::ffi::c_char);
    super::decode_ansi(s.to_bytes()).encode_utf16().collect()
}

/// CreateTextServices(punkOuter, pITextHost, ppUnk).
pub unsafe extern "win64" fn create_text_services(
    externe: usize,
    hote: usize,
    out: *mut usize,
) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    if hote == 0 {
        return E_INVALIDARG;
    }
    let objet = Box::new(TextServices {
        vtbl_prive: &VTBL_PRIVE,
        vtbl_services: &VTBL_SERVICES,
        refs: AtomicU32::new(1),
        externe,
        hote,
        etat: Mutex::new(Etat::default()),
    });
    let p = Box::into_raw(objet);
    if p.is_null() {
        return E_OUTOFMEMORY;
    }
    *out = p as usize;
    S_OK
}

// --- IUnknown privé ---------------------------------------------------------

unsafe extern "win64" fn prive_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    *out = 0;
    let Some(g) = read_guid(iid) else {
        return E_INVALIDARG;
    };
    let o = de_prive(this);
    if g == IID_IUNKNOWN {
        o.refs.fetch_add(1, Ordering::SeqCst);
        *out = o.base();
        S_OK
    } else if g == IID_ITEXT_SERVICES {
        services_add_ref(o.services());
        *out = o.services();
        S_OK
    } else {
        super::comobj::log_no_interface("TextServices", iid);
        E_NOINTERFACE
    }
}
unsafe extern "win64" fn prive_add_ref(this: usize) -> u32 {
    de_prive(this).refs.fetch_add(1, Ordering::SeqCst) + 1
}
unsafe extern "win64" fn prive_release(this: usize) -> u32 {
    let n = de_prive(this).refs.fetch_sub(1, Ordering::SeqCst) - 1;
    if n == 0 {
        drop(Box::from_raw(this as *mut TextServices));
    }
    n
}

static VTBL_PRIVE: VTable<3> = VTable([
    prive_qi as *const (),
    prive_add_ref as *const (),
    prive_release as *const (),
]);

// --- ITextServices : IUnknown délégué --------------------------------------

unsafe fn externe_fn(externe: usize, rang: usize) -> *const () {
    *(*(externe as *const *const *const ())).add(rang)
}

unsafe extern "win64" fn services_qi(this: usize, iid: *const u8, out: *mut usize) -> i32 {
    let o = de_services(this);
    if o.externe != 0 {
        let f: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
            std::mem::transmute(externe_fn(o.externe, 0));
        return f(o.externe, iid, out);
    }
    prive_qi(o.base(), iid, out)
}
unsafe extern "win64" fn services_add_ref(this: usize) -> u32 {
    let o = de_services(this);
    if o.externe != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 =
            std::mem::transmute(externe_fn(o.externe, 1));
        return f(o.externe);
    }
    prive_add_ref(o.base())
}
unsafe extern "win64" fn services_release(this: usize) -> u32 {
    let o = de_services(this);
    if o.externe != 0 {
        let f: unsafe extern "win64" fn(usize) -> u32 =
            std::mem::transmute(externe_fn(o.externe, 2));
        return f(o.externe);
    }
    prive_release(o.base())
}

// --- ITextServices ----------------------------------------------------------

unsafe extern "win64" fn tx_send_message(
    this: usize,
    msg: u32,
    wparam: usize,
    lparam: isize,
    resultat: *mut isize,
) -> i32 {
    let o = de_services(this);
    let mut r: isize = 0;
    let hr = match msg {
        WM_SETTEXT => {
            o.poser_texte(chaine_large(lparam as *const u16));
            o.texte_change();
            r = 1;
            S_OK
        }
        EM_SETTEXTEX => {
            // SETTEXTEX { flags, codepage } : 1200 = UTF-16, sinon ANSI.
            let page = if wparam == 0 {
                CP_UNICODE
            } else {
                *((wparam as *const u32).add(1))
            };
            let t = if page == CP_UNICODE {
                chaine_large(lparam as *const u16)
            } else {
                chaine_ansi(lparam as *const u8)
            };
            o.poser_texte(t);
            o.texte_change();
            r = 1;
            S_OK
        }
        WM_GETTEXT => {
            let e = o.etat();
            let dst = lparam as *mut u16;
            if wparam > 0 && !dst.is_null() {
                let n = e.texte.len().min(wparam - 1);
                std::ptr::copy_nonoverlapping(e.texte.as_ptr(), dst, n);
                *dst.add(n) = 0;
                r = n as isize;
            }
            S_OK
        }
        WM_GETTEXTLENGTH | EM_GETTEXTLENGTHEX => {
            r = o.etat().texte.len() as isize;
            S_OK
        }
        EM_SETEVENTMASK => {
            let mut e = o.etat();
            r = e.masque_evenements as isize;
            e.masque_evenements = lparam as u32;
            S_OK
        }
        EM_GETEVENTMASK => {
            r = o.etat().masque_evenements as isize;
            S_OK
        }
        EM_LIMITTEXT => {
            o.etat().limite = wparam as u32;
            S_OK
        }
        EM_EXLIMITTEXT => {
            o.etat().limite = lparam as u32;
            S_OK
        }
        _ => {
            note_message_non_traite(msg);
            S_FALSE
        }
    };
    if !resultat.is_null() {
        *resultat = r;
    }
    hr
}

/// Premier passage de chaque message non traité, sur stderr.
fn note_message_non_traite(msg: u32) {
    static VUS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
    let mut v = VUS.lock().unwrap_or_else(|e| e.into_inner());
    if !v.contains(&msg) {
        v.push(msg);
        eprintln!("winlux[textserv] TxSendMessage 0x{msg:x} non traité : S_FALSE");
    }
}

/// Texte à dessiner : un retour chariot seul (paragraphe RichEdit) devient
/// CR LF pour DrawTextW.
fn texte_a_dessiner(t: &[u16]) -> Vec<u16> {
    let mut v = Vec::with_capacity(t.len());
    for (i, &c) in t.iter().enumerate() {
        v.push(c);
        if c == 13 && t.get(i + 1) != Some(&10) {
            v.push(10);
        }
    }
    v
}

#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn tx_draw(
    this: usize,
    _aspect: u32,
    _lindex: i32,
    _pv_aspect: *mut c_void,
    _ptd: *mut c_void,
    hdc: usize,
    _hic_target: usize,
    bornes: *const Rect,
    _bornes_meta: *const Rect,
    _maj: *mut Rect,
    _continuer: usize,
    _continuer_param: u32,
    _vue: i32,
) -> i32 {
    let o = de_services(this);
    // Sans bornes, le contrôle actif dessine dans le rectangle client.
    let mut r = if bornes.is_null() {
        match o.hote_rect_client() {
            Some(r) => r,
            None => return E_INVALIDARG,
        }
    } else {
        *bornes
    };
    let (dc, rendre) = if hdc != 0 {
        (hdc, false)
    } else {
        let f: unsafe extern "win64" fn(usize) -> usize =
            std::mem::transmute(o.hote_fn(HOST_GET_DC));
        (f(o.hote), true)
    };
    if dc == 0 {
        return E_INVALIDARG;
    }
    if o.hote_opaque() {
        let pinceau = super::gdi32::create_solid_brush(o.hote_couleur(COLOR_WINDOW));
        super::gdi32_ext::fill_rect(dc, &r, pinceau);
        super::gdi32::delete_object(pinceau);
    }
    let format = if o.bits() & TXTBIT_MULTILINE != 0 {
        DT_WORDBREAK | DT_NOPREFIX
    } else {
        DT_SINGLELINE | DT_NOPREFIX
    };
    let texte = texte_a_dessiner(&o.etat().texte);
    let couleur = super::gdi32::set_text_color(dc, o.hote_couleur(COLOR_WINDOWTEXT));
    let mode = super::gdi32::set_bk_mode(dc, TRANSPARENT);
    if !texte.is_empty() {
        super::gdi32::draw_text_w(dc, texte.as_ptr(), texte.len() as i32, &mut r, format);
    }
    super::gdi32::set_bk_mode(dc, mode);
    super::gdi32::set_text_color(dc, couleur);
    o.etat().taille = (
        (r.right - r.left).max(0) as u32,
        (r.bottom - r.top).max(0) as u32,
    );
    if rendre {
        let f: unsafe extern "win64" fn(usize, usize) -> i32 =
            std::mem::transmute(o.hote_fn(HOST_RELEASE_DC));
        f(o.hote, dc);
    }
    S_OK
}

/// Pas de barre de défilement : bornes et position nulles, désactivée.
unsafe fn defilement(
    min: *mut i32,
    max: *mut i32,
    pos: *mut i32,
    page: *mut i32,
    actif: *mut i32,
) -> i32 {
    for p in [min, max, pos, page, actif] {
        if !p.is_null() {
            *p = 0;
        }
    }
    S_OK
}
unsafe extern "win64" fn tx_get_hscroll(
    _this: usize,
    min: *mut i32,
    max: *mut i32,
    pos: *mut i32,
    page: *mut i32,
    actif: *mut i32,
) -> i32 {
    defilement(min, max, pos, page, actif)
}
unsafe extern "win64" fn tx_get_vscroll(
    _this: usize,
    min: *mut i32,
    max: *mut i32,
    pos: *mut i32,
    page: *mut i32,
    actif: *mut i32,
) -> i32 {
    defilement(min, max, pos, page, actif)
}

unsafe extern "win64" fn on_tx_set_cursor(_this: usize) -> i32 {
    non_servie!("OnTxSetCursor")
}
unsafe extern "win64" fn tx_query_hit_point(_this: usize) -> i32 {
    non_servie!("TxQueryHitPoint")
}
unsafe extern "win64" fn on_tx_in_place_activate(this: usize, _rect: *const Rect) -> i32 {
    de_services(this).etat().actif = true;
    S_OK
}
unsafe extern "win64" fn on_tx_in_place_deactivate(this: usize) -> i32 {
    let mut e = de_services(this).etat();
    e.actif = false;
    e.ui_actif = false;
    S_OK
}
unsafe extern "win64" fn on_tx_ui_activate(this: usize) -> i32 {
    de_services(this).etat().ui_actif = true;
    S_OK
}
unsafe extern "win64" fn on_tx_ui_deactivate(this: usize) -> i32 {
    de_services(this).etat().ui_actif = false;
    S_OK
}

unsafe extern "win64" fn tx_get_text(this: usize, out: *mut *mut u16) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let mut t = de_services(this).etat().texte.clone();
    t.push(0);
    *out = super::oleaut32::sys_alloc_string(t.as_ptr());
    if (*out).is_null() {
        E_OUTOFMEMORY
    } else {
        S_OK
    }
}
unsafe extern "win64" fn tx_set_text(this: usize, texte: *const u16) -> i32 {
    let o = de_services(this);
    o.poser_texte(chaine_large(texte));
    o.texte_change();
    S_OK
}

unsafe extern "win64" fn tx_get_cur_target_x(_this: usize, _x: *mut i32) -> i32 {
    non_servie!("TxGetCurTargetX")
}
unsafe extern "win64" fn tx_get_base_line_pos(_this: usize, _pos: *mut i32) -> i32 {
    non_servie!("TxGetBaseLinePos")
}

/// TxGetNaturalSize : hauteur du texte pour la largeur proposée, mesurée
/// par DrawTextW(DT_CALCRECT) dans le DC donné.
#[allow(clippy::too_many_arguments)]
unsafe extern "win64" fn tx_get_natural_size(
    this: usize,
    _aspect: u32,
    hdc: usize,
    _hic_target: usize,
    _ptd: *mut c_void,
    _mode: u32,
    _etendue: *const c_void,
    largeur: *mut i32,
    hauteur: *mut i32,
) -> i32 {
    if largeur.is_null() || hauteur.is_null() {
        return E_INVALIDARG;
    }
    if hdc == 0 {
        return E_INVALIDARG;
    }
    let o = de_services(this);
    let format = if o.bits() & TXTBIT_MULTILINE != 0 {
        DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT
    } else {
        DT_SINGLELINE | DT_NOPREFIX | DT_CALCRECT
    };
    let mut texte = texte_a_dessiner(&o.etat().texte);
    if texte.is_empty() {
        // Un contrôle vide garde la hauteur d'une ligne.
        texte.push(u16::from(b' '));
    }
    let mut r = Rect {
        left: 0,
        top: 0,
        right: (*largeur).max(1),
        bottom: 0,
    };
    super::gdi32::draw_text_w(hdc, texte.as_ptr(), texte.len() as i32, &mut r, format);
    if format & DT_WORDBREAK == 0 {
        *largeur = r.right - r.left;
    }
    *hauteur = r.bottom - r.top;
    o.etat().taille = ((*largeur).max(0) as u32, (*hauteur).max(0) as u32);
    S_OK
}

unsafe extern "win64" fn tx_get_drop_target(_this: usize, out: *mut usize) -> i32 {
    if !out.is_null() {
        *out = 0;
    }
    non_servie!("TxGetDropTarget")
}

unsafe extern "win64" fn on_tx_property_bits_change(this: usize, masque: u32, bits: u32) -> i32 {
    let o = de_services(this);
    {
        let mut e = o.etat();
        e.bits = (e.bits & !masque) | (bits & masque);
        e.bits_connus |= masque;
    }
    o.texte_change();
    S_OK
}

unsafe extern "win64" fn tx_get_cached_size(
    this: usize,
    largeur: *mut u32,
    hauteur: *mut u32,
) -> i32 {
    let (l, h) = de_services(this).etat().taille;
    if !largeur.is_null() {
        *largeur = l;
    }
    if !hauteur.is_null() {
        *hauteur = h;
    }
    S_OK
}

static VTBL_SERVICES: VTable<21> = VTable([
    services_qi as *const (),
    services_add_ref as *const (),
    services_release as *const (),
    tx_send_message as *const (),
    tx_draw as *const (),
    tx_get_hscroll as *const (),
    tx_get_vscroll as *const (),
    on_tx_set_cursor as *const (),
    tx_query_hit_point as *const (),
    on_tx_in_place_activate as *const (),
    on_tx_in_place_deactivate as *const (),
    on_tx_ui_activate as *const (),
    on_tx_ui_deactivate as *const (),
    tx_get_text as *const (),
    tx_set_text as *const (),
    tx_get_cur_target_x as *const (),
    tx_get_base_line_pos as *const (),
    tx_get_natural_size as *const (),
    tx_get_drop_target as *const (),
    on_tx_property_bits_change as *const (),
    tx_get_cached_size as *const (),
]);

pub fn exports() -> super::Exports {
    crate::exports! {
        "CreateTextServices" => fn create_text_services,
        "IID_ITextServices" => data IID_ITEXT_SERVICES,
        "IID_ITextHost" => data IID_ITEXT_HOST,
        "IID_ITextHost2" => data IID_ITEXT_HOST2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// Hôte minimal : 42 entrées, dont celles que l'objet appelle.
    #[repr(C)]
    struct Hote {
        vtbl: *const [*const (); 42],
        invalidations: AtomicUsize,
        notifications: AtomicUsize,
    }

    unsafe extern "win64" fn h_rien(_this: usize) -> i32 {
        E_NOTIMPL
    }
    unsafe extern "win64" fn h_invalidate(this: usize, _r: *const Rect, _mode: i32) {
        (*(this as *const Hote))
            .invalidations
            .fetch_add(1, Ordering::SeqCst);
    }
    unsafe extern "win64" fn h_view_change(_this: usize, _maj: i32) {}
    unsafe extern "win64" fn h_client(_this: usize, r: *mut Rect) -> i32 {
        *r = Rect {
            left: 0,
            top: 0,
            right: 200,
            bottom: 40,
        };
        S_OK
    }
    unsafe extern "win64" fn h_couleur(_this: usize, i: i32) -> u32 {
        if i == COLOR_WINDOW {
            0x00ff_ffff
        } else {
            0
        }
    }
    unsafe extern "win64" fn h_fond(_this: usize, s: *mut i32) -> i32 {
        *s = TXTBACK_OPAQUE;
        S_OK
    }
    unsafe extern "win64" fn h_bits(_this: usize, _m: u32, b: *mut u32) -> i32 {
        *b = 0;
        S_OK
    }
    unsafe extern "win64" fn h_notify(this: usize, n: u32, _pv: *mut c_void) -> i32 {
        if n == EN_CHANGE {
            (*(this as *const Hote))
                .notifications
                .fetch_add(1, Ordering::SeqCst);
        }
        S_OK
    }

    fn table_hote() -> Box<[*const (); 42]> {
        let mut t = Box::new([h_rien as *const (); 42]);
        t[HOST_INVALIDATE_RECT] = h_invalidate as *const ();
        t[HOST_VIEW_CHANGE] = h_view_change as *const ();
        t[HOST_GET_CLIENT_RECT] = h_client as *const ();
        t[HOST_GET_SYS_COLOR] = h_couleur as *const ();
        t[HOST_GET_BACK_STYLE] = h_fond as *const ();
        t[HOST_GET_PROPERTY_BITS] = h_bits as *const ();
        t[HOST_NOTIFY] = h_notify as *const ();
        t
    }

    unsafe fn methode(obj: usize, rang: usize) -> *const () {
        *(*(obj as *const *const *const ())).add(rang)
    }

    #[test]
    fn text_services_sert_le_texte_3268() {
        let table = table_hote();
        let hote = Hote {
            vtbl: &*table,
            invalidations: AtomicUsize::new(0),
            notifications: AtomicUsize::new(0),
        };
        let hote_p = &hote as *const Hote as usize;
        let ex = exports();
        let creer: unsafe extern "win64" fn(usize, usize, *mut usize) -> i32 =
            unsafe { std::mem::transmute(ex["CreateTextServices"]) };
        let iid = ex["IID_ITextServices"] as *const u8;
        unsafe {
            let mut unk = 0usize;
            assert_eq!(creer(0, 0, &mut unk), E_INVALIDARG, "hôte nul");
            assert_eq!(creer(0, hote_p, &mut unk), S_OK);
            assert_ne!(unk, 0);

            // QueryInterface avec l'IID lu dans la DLL.
            let qi: unsafe extern "win64" fn(usize, *const u8, *mut usize) -> i32 =
                std::mem::transmute(methode(unk, 0));
            let mut ts = 0usize;
            assert_eq!(qi(unk, iid, &mut ts), S_OK);
            assert_eq!(ts, unk + DECALAGE_SERVICES);
            let mut rien = 1usize;
            assert_eq!(
                qi(unk, ex["IID_ITextHost"] as *const u8, &mut rien),
                E_NOINTERFACE
            );
            assert_eq!(rien, 0);

            // TxSetText puis TxGetText.
            let set: unsafe extern "win64" fn(usize, *const u16) -> i32 =
                std::mem::transmute(methode(ts, 14));
            let get: unsafe extern "win64" fn(usize, *mut *mut u16) -> i32 =
                std::mem::transmute(methode(ts, 13));
            let texte: Vec<u16> = "Winlux 3268".encode_utf16().chain([0]).collect();
            assert_eq!(set(ts, texte.as_ptr()), S_OK);
            assert!(hote.invalidations.load(Ordering::SeqCst) >= 1);
            let mut bstr: *mut u16 = std::ptr::null_mut();
            assert_eq!(get(ts, &mut bstr), S_OK);
            assert_eq!(chaine_large(bstr), texte[..texte.len() - 1]);
            super::super::oleaut32::sys_free_string(bstr);

            // TxSendMessage : longueur, lecture, masque, message inconnu.
            let send: unsafe extern "win64" fn(usize, u32, usize, isize, *mut isize) -> i32 =
                std::mem::transmute(methode(ts, 3));
            let mut r = 0isize;
            assert_eq!(send(ts, WM_GETTEXTLENGTH, 0, 0, &mut r), S_OK);
            assert_eq!(r, 11);
            let mut tampon = [0u16; 7];
            assert_eq!(
                send(ts, WM_GETTEXT, 7, tampon.as_mut_ptr() as isize, &mut r),
                S_OK
            );
            assert_eq!(r, 6);
            assert_eq!(&tampon[..6], &texte[..6]);
            assert_eq!(
                send(ts, EM_SETEVENTMASK, 0, ENM_CHANGE as isize, &mut r),
                S_OK
            );
            assert_eq!(r, 0);
            let neuf: Vec<u16> = "abc".encode_utf16().chain([0]).collect();
            assert_eq!(
                send(ts, WM_SETTEXT, 0, neuf.as_ptr() as isize, &mut r),
                S_OK
            );
            assert_eq!(hote.notifications.load(Ordering::SeqCst), 1, "EN_CHANGE");
            assert_eq!(send(ts, WM_GETTEXTLENGTH, 0, 0, &mut r), S_OK);
            assert_eq!(r, 3);
            assert_eq!(
                send(ts, 0x0401, 0, 0, &mut r),
                S_FALSE,
                "message non traité"
            );

            // Défilement : rien, désactivé.
            let hs: unsafe extern "win64" fn(
                usize,
                *mut i32,
                *mut i32,
                *mut i32,
                *mut i32,
                *mut i32,
            ) -> i32 = std::mem::transmute(methode(ts, 5));
            let (mut a, mut b, mut c, mut d, mut e) = (9, 9, 9, 9, 9);
            assert_eq!(hs(ts, &mut a, &mut b, &mut c, &mut d, &mut e), S_OK);
            assert_eq!((a, b, c, d, e), (0, 0, 0, 0, 0));

            // Ce qui n'est pas servi le dit.
            let hit: unsafe extern "win64" fn(usize) -> i32 = std::mem::transmute(methode(ts, 8));
            assert_eq!(hit(ts), E_NOTIMPL);
            let x: unsafe extern "win64" fn(usize, *mut i32) -> i32 =
                std::mem::transmute(methode(ts, 15));
            assert_eq!(x(ts, &mut a), E_NOTIMPL);

            // Références : ITextServices et IUnknown privé partagent le compteur.
            let release_ts: unsafe extern "win64" fn(usize) -> u32 =
                std::mem::transmute(methode(ts, 2));
            assert_eq!(release_ts(ts), 1);
            let release: unsafe extern "win64" fn(usize) -> u32 =
                std::mem::transmute(methode(unk, 2));
            assert_eq!(release(unk), 0);
        }
    }

    #[test]
    fn tx_draw_peint_fond_et_texte_3268() {
        use super::super::gdi32;
        let table = table_hote();
        let hote = Hote {
            vtbl: &*table,
            invalidations: AtomicUsize::new(0),
            notifications: AtomicUsize::new(0),
        };
        unsafe {
            let mut unk = 0usize;
            assert_eq!(
                create_text_services(0, &hote as *const Hote as usize, &mut unk),
                S_OK
            );
            let ts = unk + DECALAGE_SERVICES;
            let texte: Vec<u16> = "WWWW".encode_utf16().chain([0]).collect();
            assert_eq!(tx_set_text(ts, texte.as_ptr()), S_OK);

            let mem = gdi32::create_compatible_dc(0);
            let bitmap = gdi32::create_compatible_bitmap(mem, 200, 40);
            gdi32::select_object(mem, bitmap);
            let bornes = Rect {
                left: 0,
                top: 0,
                right: 200,
                bottom: 40,
            };
            let hr = tx_draw(
                ts,
                1,
                -1,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                mem,
                0,
                &bornes,
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
                0,
                0,
            );
            assert_eq!(hr, S_OK);
            // Fond opaque à la couleur COLOR_WINDOW de l'hôte (blanc).
            assert_eq!(gdi32::get_pixel(mem, 199, 39) & 0xff_ffff, 0xff_ffff);
            // Le texte, noir, a touché des pixels.
            let encre = (0..60)
                .flat_map(|x| (0..20).map(move |y| (x, y)))
                .filter(|&(x, y)| gdi32::get_pixel(mem, x, y) & 0xff_ffff != 0xff_ffff)
                .count();
            assert!(encre > 0, "aucun pixel de texte");
            let mut l = 0u32;
            let mut h = 0u32;
            assert_eq!(tx_get_cached_size(ts, &mut l, &mut h), S_OK);
            assert!(l > 0 && h > 0);
            gdi32::delete_dc(mem);
            gdi32::delete_object(bitmap);
            prive_release(unk);
        }
    }

    /// GetProcAddress trouve les IID comme données dans msftedit et
    /// riched20, à la même adresse, donc avec la même valeur.
    #[test]
    fn iid_exportes_par_msftedit_et_riched20_3268() {
        use super::super::kernel32::get_proc_address;
        let mut vus = Vec::new();
        for dll in ["msftedit.dll", "riched20.dll"] {
            let h = super::super::module_handle(dll).expect(dll);
            let lire = |nom: &[u8]| unsafe { get_proc_address(h, nom.as_ptr()) };
            let iid = lire(b"IID_ITextServices\0");
            assert_eq!(
                iid,
                std::ptr::addr_of!(IID_ITEXT_SERVICES) as usize,
                "{dll}"
            );
            assert_eq!(unsafe { *(iid as *const Guid) }, IID_ITEXT_SERVICES);
            assert_ne!(lire(b"IID_ITextHost\0"), 0, "{dll}");
            assert_ne!(lire(b"IID_ITextHost2\0"), 0, "{dll}");
            assert_ne!(lire(b"CreateTextServices\0"), 0, "{dll}");
            vus.push(iid);
        }
        assert_eq!(vus[0], vus[1]);
    }

    #[test]
    fn texte_a_dessiner_double_le_retour_chariot_3268() {
        let t: Vec<u16> = "a\rb\r\nc".encode_utf16().collect();
        let d: Vec<u16> = "a\r\nb\r\nc".encode_utf16().collect();
        assert_eq!(texte_a_dessiner(&t), d);
    }
}
