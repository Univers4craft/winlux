//! Classe Windows Runtime `Windows.UI.ColorHelper` (#1659) : fabrique
//! statique sans état, interfaces IColorHelperStatics (FromArgb) et
//! IColorHelperStatics2 (ToDisplayName), d'après la documentation publique
//! de Microsoft ; identifiants d'interface tels que les publie windows-rs
//! (licence MIT). L'installateur en ligne d'iCUE 5 l'active au démarrage.

use super::comobj::{
    guid, read_guid, Guid, VTable, E_INVALIDARG, E_NOINTERFACE, IID_IUNKNOWN, S_OK,
};

const E_POINTER: i32 = 0x8000_4003u32 as i32;

const IID_IINSPECTABLE: Guid = guid(
    0xAF86_E2E0,
    0xB12D,
    0x4C6A,
    [0x9C, 0x5A, 0xD7, 0xAA, 0x65, 0x10, 0x1E, 0x90],
);
const IID_IAGILEOBJECT: Guid = guid(
    0x94EA_2B94,
    0xE9CC,
    0x49E0,
    [0xC0, 0xFF, 0xEE, 0x64, 0xCA, 0x8F, 0x5B, 0x90],
);
/// IColorHelperStatics {8504DBEA-FB6A-4144-A6C2-33499C9284F5}.
const IID_STATICS: Guid = guid(
    0x8504_DBEA,
    0xFB6A,
    0x4144,
    [0xA6, 0xC2, 0x33, 0x49, 0x9C, 0x92, 0x84, 0xF5],
);
/// IColorHelperStatics2 {24D9AF02-6EB0-4B94-855C-FCF0818D9A16}.
const IID_STATICS2: Guid = guid(
    0x24D9_AF02,
    0x6EB0,
    0x4B94,
    [0x85, 0x5C, 0xFC, 0xF0, 0x81, 0x8D, 0x9A, 0x16],
);

const CLASS: &str = "Windows.UI.ColorHelper";

#[repr(C)]
struct Statics {
    vtbl: &'static VTable<7>,
}

static STATICS: Statics = Statics {
    vtbl: &STATICS_VTBL,
};
static STATICS2: Statics = Statics {
    vtbl: &STATICS2_VTBL,
};

static STATICS_VTBL: VTable<7> = VTable([
    qi as *const (),
    add_ref as *const (),
    release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    from_argb as *const (),
]);

static STATICS2_VTBL: VTable<7> = VTable([
    qi as *const (),
    add_ref as *const (),
    release as *const (),
    get_iids as *const (),
    class_name as *const (),
    trust_level as *const (),
    to_display_name as *const (),
]);

unsafe extern "win64" fn qi(_this: usize, iid: *const u8, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let Some(iid) = read_guid(iid) else {
        *out = 0;
        return E_INVALIDARG;
    };
    *out = if iid == IID_STATICS2 {
        std::ptr::addr_of!(STATICS2) as usize
    } else if iid == IID_STATICS
        || iid == IID_IUNKNOWN
        || iid == IID_IINSPECTABLE
        || iid == IID_IAGILEOBJECT
    {
        std::ptr::addr_of!(STATICS) as usize
    } else {
        0
    };
    if *out == 0 {
        E_NOINTERFACE
    } else {
        S_OK
    }
}

unsafe extern "win64" fn add_ref(_this: usize) -> u32 {
    1
}

unsafe extern "win64" fn release(_this: usize) -> u32 {
    1
}

unsafe extern "win64" fn get_iids(_this: usize, count: *mut u32, iids: *mut usize) -> i32 {
    if !count.is_null() {
        *count = 0;
    }
    if !iids.is_null() {
        *iids = 0;
    }
    S_OK
}

unsafe extern "win64" fn class_name(_this: usize, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let w: Vec<u16> = CLASS.encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

unsafe extern "win64" fn trust_level(_this: usize, level: *mut u32) -> i32 {
    if level.is_null() {
        return E_POINTER;
    }
    *level = 0; // BaseTrust
    S_OK
}

/// FromArgb(a, r, g, b, Color*) : Color = { A, R, G, B }, un octet chacun.
unsafe extern "win64" fn from_argb(_this: usize, a: u8, r: u8, g: u8, b: u8, out: *mut u8) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    std::ptr::copy_nonoverlapping([a, r, g, b].as_ptr(), out, 4);
    S_OK
}

/// Couleurs nommées (RVB) et leur nom en français, anglais, allemand et
/// espagnol.
const NAMED: &[([u8; 3], [&str; 4])] = &[
    ([0, 0, 0], ["Noir", "Black", "Schwarz", "Negro"]),
    ([255, 255, 255], ["Blanc", "White", "Weiß", "Blanco"]),
    ([128, 128, 128], ["Gris", "Gray", "Grau", "Gris"]),
    (
        [192, 192, 192],
        ["Gris clair", "Light gray", "Hellgrau", "Gris claro"],
    ),
    (
        [64, 64, 64],
        ["Gris foncé", "Dark gray", "Dunkelgrau", "Gris oscuro"],
    ),
    ([255, 0, 0], ["Rouge", "Red", "Rot", "Rojo"]),
    (
        [139, 0, 0],
        ["Rouge foncé", "Dark red", "Dunkelrot", "Rojo oscuro"],
    ),
    ([255, 165, 0], ["Orange", "Orange", "Orange", "Naranja"]),
    ([255, 255, 0], ["Jaune", "Yellow", "Gelb", "Amarillo"]),
    ([0, 128, 0], ["Vert", "Green", "Grün", "Verde"]),
    (
        [144, 238, 144],
        ["Vert clair", "Light green", "Hellgrün", "Verde claro"],
    ),
    (
        [0, 100, 0],
        ["Vert foncé", "Dark green", "Dunkelgrün", "Verde oscuro"],
    ),
    ([0, 255, 255], ["Cyan", "Cyan", "Cyan", "Cian"]),
    ([0, 0, 255], ["Bleu", "Blue", "Blau", "Azul"]),
    (
        [173, 216, 230],
        ["Bleu clair", "Light blue", "Hellblau", "Azul claro"],
    ),
    (
        [0, 0, 139],
        ["Bleu foncé", "Dark blue", "Dunkelblau", "Azul oscuro"],
    ),
    ([128, 0, 128], ["Violet", "Purple", "Lila", "Morado"]),
    ([255, 0, 255], ["Magenta", "Magenta", "Magenta", "Magenta"]),
    ([255, 192, 203], ["Rose", "Pink", "Rosa", "Rosa"]),
    ([165, 42, 42], ["Marron", "Brown", "Braun", "Marrón"]),
];

/// Nom de la couleur nommée la plus proche, dans la langue de l'interface.
fn display_name(r: u8, g: u8, b: u8) -> &'static str {
    let lang = crate::intl::locale::ui_language_tag().unwrap_or_default();
    let i = match lang.get(..2) {
        Some("en") => 1,
        Some("de") => 2,
        Some("es") => 3,
        _ => 0,
    };
    let d = |c: &[u8; 3]| {
        let sq = |x: u8, y: u8| (i32::from(x) - i32::from(y)).pow(2);
        sq(c[0], r) + sq(c[1], g) + sq(c[2], b)
    };
    NAMED
        .iter()
        .min_by_key(|(c, _)| d(c))
        .map_or("", |(_, names)| names[i])
}

/// ToDisplayName(Color, HSTRING*) : la couleur arrive par valeur (4 octets
/// dans un registre).
unsafe extern "win64" fn to_display_name(_this: usize, color: u32, out: *mut usize) -> i32 {
    if out.is_null() {
        return E_POINTER;
    }
    let [_a, r, g, b] = color.to_le_bytes();
    let w: Vec<u16> = display_name(r, g, b).encode_utf16().collect();
    super::electron_win32::WindowsCreateString(w.as_ptr(), w.len() as u32, out)
}

/// `RoGetActivationFactory` pour `Windows.UI.ColorHelper`, `None` pour une
/// autre classe.
pub unsafe fn activation_factory(class: &str, iid: *const u8, out: *mut usize) -> Option<i32> {
    if class != CLASS {
        return None;
    }
    Some(qi(0, iid, out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_helper() {
        unsafe {
            let mut f = 0usize;
            assert_eq!(
                activation_factory(CLASS, IID_STATICS.as_ptr(), &mut f),
                Some(S_OK)
            );
            assert_ne!(f, 0);
            assert!(
                activation_factory("Windows.UI.Colors", IID_STATICS.as_ptr(), &mut f).is_none()
            );
            let mut c = [0u8; 4];
            assert_eq!(from_argb(f, 255, 1, 2, 3, c.as_mut_ptr()), S_OK);
            assert_eq!(c, [255, 1, 2, 3]);
            let mut f2 = 0usize;
            assert_eq!(qi(f, IID_STATICS2.as_ptr(), &mut f2), S_OK);
            assert_ne!(f2, f);
            let mut h = 0usize;
            assert_eq!(
                to_display_name(f2, u32::from_le_bytes([255, 250, 5, 5]), &mut h),
                S_OK
            );
            let name = String::from_utf16_lossy(super::super::electron_win32::hstring_slice(h));
            assert!(
                ["Rouge", "Red", "Rot", "Rojo"].contains(&name.as_str()),
                "{name}"
            );
            let mut cls = 0usize;
            assert_eq!(class_name(f, &mut cls), S_OK);
            assert_eq!(
                String::from_utf16_lossy(super::super::electron_win32::hstring_slice(cls)),
                CLASS
            );
            let mut none = 1usize;
            assert_eq!(qi(f, [0u8; 16].as_ptr(), &mut none), E_NOINTERFACE);
            assert_eq!(none, 0);
        }
    }
}
