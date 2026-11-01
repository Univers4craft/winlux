//! Compatibilité de texte, de langue, de clavier et d'heure entre Linux et
//! les .exe : pages de code, locale, disposition clavier, fuseau horaire et
//! conversion des fichiers texte. Tout est écrit de zéro.

pub mod api;
pub mod codepage;
pub mod keyboard;
pub mod locale;
pub mod text;
pub mod tz;

/// `winlux --intl` : réglages vus par les .exe (diagnostic).
pub fn report() -> i32 {
    let l = locale::current();
    let ui = locale::ui_locale();
    let kb = keyboard::current();
    let z = tz::current();
    println!("locale\t{} (LCID {:04X})", l.name, l.lcid);
    println!("interface\t{} (LANGID {:04X})", ui.name, ui.lcid);
    println!("pages de code\tANSI {} / OEM {}", l.ansi_cp, l.oem_cp);
    println!("clavier\t{} (HKL {:08X})", kb.name(), kb.hkl());
    println!(
        "fuseau\t{} → {} (biais {} min{})",
        if z.iana.is_empty() { "?" } else { &z.iana },
        z.windows_key(),
        z.bias,
        if z.has_dst() { ", heure d'été" } else { "" }
    );
    0
}
