//! ID3D11Device::CheckFormatSupport : capacités de chaque format DXGI.
//!
//! ANGLE construit ses tables de formats OpenGL ES (rendu, échantillonnage,
//! mélange) à partir de ces réponses. Les drapeaux fixes rendus jusqu'ici
//! (tampon, texture 2D) ne disaient d'aucun format qu'il était une cible de
//! rendu : Skia recevait un format GL invalide et le processus GPU de
//! Chromium s'arrêtait (#1304, #996).
//!
//! Valeurs D3D11_FORMAT_SUPPORT : d3d11.h de MinGW-w64 (domaine public).
//! Ne sont annoncés que les formats que `dxgi_format_to_gl_bind` sait
//! traduire en OpenGL.

pub(super) const BUFFER: u32 = 0x1;
pub(super) const IA_VERTEX_BUFFER: u32 = 0x2;
pub(super) const IA_INDEX_BUFFER: u32 = 0x4;
pub(super) const TEXTURE1D: u32 = 0x10;
pub(super) const TEXTURE2D: u32 = 0x20;
pub(super) const TEXTURE3D: u32 = 0x40;
pub(super) const TEXTURECUBE: u32 = 0x80;
pub(super) const SHADER_LOAD: u32 = 0x100;
pub(super) const SHADER_SAMPLE: u32 = 0x200;
pub(super) const SHADER_SAMPLE_COMPARISON: u32 = 0x400;
pub(super) const MIP: u32 = 0x1000;
pub(super) const MIP_AUTOGEN: u32 = 0x2000;
pub(super) const RENDER_TARGET: u32 = 0x4000;
pub(super) const BLENDABLE: u32 = 0x8000;
pub(super) const DEPTH_STENCIL: u32 = 0x1_0000;
pub(super) const CPU_LOCKABLE: u32 = 0x2_0000;
pub(super) const MULTISAMPLE_RESOLVE: u32 = 0x4_0000;
pub(super) const DISPLAY: u32 = 0x8_0000;
pub(super) const CAST_WITHIN_BIT_LAYOUT: u32 = 0x10_0000;
pub(super) const MULTISAMPLE_RENDERTARGET: u32 = 0x20_0000;
pub(super) const MULTISAMPLE_LOAD: u32 = 0x40_0000;
pub(super) const SHADER_GATHER: u32 = 0x80_0000;
pub(super) const BACK_BUFFER_CAST: u32 = 0x100_0000;
pub(super) const SHADER_GATHER_COMPARISON: u32 = 0x400_0000;

const TEX_ALL: u32 = TEXTURE1D | TEXTURE2D | TEXTURE3D | TEXTURECUBE | MIP;

/// Format sans type : textures et conversions dans sa famille.
const TYPELESS: u32 = TEX_ALL | CAST_WITHIN_BIT_LAYOUT;

/// Couleur flottante ou normalisée : échantillonnée, rendue, mélangée.
const COLOR: u32 = BUFFER
    | IA_VERTEX_BUFFER
    | TEX_ALL
    | SHADER_LOAD
    | SHADER_SAMPLE
    | SHADER_GATHER
    | MIP_AUTOGEN
    | RENDER_TARGET
    | BLENDABLE
    | CPU_LOCKABLE
    | MULTISAMPLE_RESOLVE
    | MULTISAMPLE_RENDERTARGET
    | MULTISAMPLE_LOAD
    | CAST_WITHIN_BIT_LAYOUT;

/// Formats d'une chaîne d'échange.
const DISPLAYABLE: u32 = COLOR | DISPLAY | BACK_BUFFER_CAST;

/// Entier : lu et rendu, ni échantillonné ni mélangé.
const INTEGER: u32 = BUFFER
    | IA_VERTEX_BUFFER
    | TEX_ALL
    | SHADER_LOAD
    | RENDER_TARGET
    | CPU_LOCKABLE
    | MULTISAMPLE_RENDERTARGET
    | MULTISAMPLE_LOAD
    | CAST_WITHIN_BIT_LAYOUT;

/// Profondeur et gabarit.
const DEPTH: u32 =
    TEXTURE1D | TEXTURE2D | TEXTURECUBE | MIP | DEPTH_STENCIL | MULTISAMPLE_RENDERTARGET;

/// Profondeur relue comme couleur (R24_UNORM_X8, R32_FLOAT_X8X24).
const DEPTH_READ: u32 = TEXTURE1D
    | TEXTURE2D
    | TEXTURECUBE
    | MIP
    | SHADER_LOAD
    | SHADER_SAMPLE
    | SHADER_SAMPLE_COMPARISON
    | SHADER_GATHER
    | SHADER_GATHER_COMPARISON
    | CAST_WITHIN_BIT_LAYOUT;

/// Gabarit relu (X24_G8_UINT, X32_G8X24_UINT).
const STENCIL_READ: u32 =
    TEXTURE1D | TEXTURE2D | TEXTURECUBE | MIP | SHADER_LOAD | CAST_WITHIN_BIT_LAYOUT;

/// Bloc compressé BC1 à BC7.
const BLOCK: u32 = TEXTURE2D
    | TEXTURE3D
    | TEXTURECUBE
    | MIP
    | SHADER_LOAD
    | SHADER_SAMPLE
    | CAST_WITHIN_BIT_LAYOUT;

/// Réponse d'avant pour les formats hors table (NV12 et formats vidéo,
/// SNORM à un ou deux canaux…) : tampon et texture seulement, ni rendu ni
/// échantillonnage. Les garder évite de retirer à une application un chemin
/// qu'elle empruntait déjà (VLC, décodage vidéo).
const LEGACY: u32 = BUFFER | IA_VERTEX_BUFFER | IA_INDEX_BUFFER | TEXTURE1D | TEXTURE2D;

/// Capacités D3D11_FORMAT_SUPPORT d'un format DXGI ; 0 pour
/// DXGI_FORMAT_UNKNOWN (CheckFormatSupport rend alors E_FAIL).
pub(super) fn format_support(format: u32) -> u32 {
    match format {
        // Sans type.
        1 | 5 | 9 | 15 | 23 | 27 | 33 | 39 | 48 | 53 | 60 | 90 | 92 => TYPELESS,
        19 | 44 => TEXTURE1D | TEXTURE2D | TEXTURECUBE | MIP | CAST_WITHIN_BIT_LAYOUT,
        70 | 73 | 76 | 79 | 82 | 94 | 97 => {
            TEXTURE2D | TEXTURE3D | TEXTURECUBE | MIP | CAST_WITHIN_BIT_LAYOUT
        }
        // R32G32B32 : ni cible de rendu ni mélange (facultatifs en 11.0).
        6 => BUFFER | IA_VERTEX_BUFFER | TEX_ALL | SHADER_LOAD | SHADER_SAMPLE | CPU_LOCKABLE,
        7 | 8 => BUFFER | IA_VERTEX_BUFFER | TEX_ALL | SHADER_LOAD | CPU_LOCKABLE,
        // Couleurs de chaîne d'échange.
        10 | 24 | 28 | 29 | 87 | 91 => DISPLAYABLE,
        // Autres couleurs flottantes ou normalisées.
        2 | 11 | 13 | 16 | 26 | 31 | 34 | 35 | 49 | 54 | 56 | 61 | 65 | 85 | 86 | 88 | 93 => COLOR,
        // R32_FLOAT : aussi comparé (ombres).
        41 => COLOR | SHADER_SAMPLE_COMPARISON | SHADER_GATHER_COMPARISON,
        // Entiers ; R16_UINT et R32_UINT servent aussi d'index.
        42 | 57 => INTEGER | IA_INDEX_BUFFER,
        3 | 4 | 12 | 14 | 17 | 18 | 25 | 30 | 32 | 36 | 43 | 50 | 62 => INTEGER,
        // Profondeur.
        20 | 40 | 45 | 55 => DEPTH,
        21 | 46 => DEPTH_READ,
        22 | 47 => STENCIL_READ,
        // Compressés.
        71 | 72 | 74 | 75 | 77 | 78 | 80 | 81 | 83 | 84 | 95 | 96 | 98 | 99 => BLOCK,
        0 => 0,
        // Le reste n'a pas de traduction OpenGL fidèle : réponse d'avant.
        _ => LEGACY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_de_rendu_annonces() {
        // R8G8B8A8_UNORM et B8G8R8A8_UNORM : rendus, échantillonnés,
        // mélangés, affichables (ANGLE, Skia).
        for f in [28, 87] {
            let s = format_support(f);
            for flag in [
                TEXTURE2D,
                SHADER_SAMPLE,
                RENDER_TARGET,
                BLENDABLE,
                DISPLAY,
                MIP_AUTOGEN,
            ] {
                assert_ne!(s & flag, 0, "format {f}, drapeau {flag:#x}");
            }
        }
        // Index 16 et 32 bits.
        assert_ne!(format_support(42) & IA_INDEX_BUFFER, 0);
        assert_ne!(format_support(57) & IA_INDEX_BUFFER, 0);
        assert_eq!(format_support(28) & IA_INDEX_BUFFER, 0);
        // Entiers : pas d'échantillonnage filtré ni de mélange.
        assert_eq!(format_support(30) & (SHADER_SAMPLE | BLENDABLE), 0);
        assert_ne!(format_support(30) & RENDER_TARGET, 0);
        // Profondeur.
        assert_ne!(format_support(45) & DEPTH_STENCIL, 0);
        assert_eq!(format_support(45) & RENDER_TARGET, 0);
        // R32G32B32_FLOAT : pas de cible de rendu.
        assert_eq!(format_support(6) & RENDER_TARGET, 0);
        // NV12 : réponse d'avant, sans rendu ni échantillonnage ;
        // DXGI_FORMAT_UNKNOWN refusé.
        assert_eq!(format_support(103), LEGACY);
        assert_eq!(format_support(103) & (RENDER_TARGET | SHADER_SAMPLE), 0);
        assert_eq!(format_support(0), 0);
    }

    #[test]
    fn formats_annonces_ont_une_traduction_opengl() {
        // Tout format annoncé échantillonnable ou rendu doit avoir sa propre
        // entrée OpenGL, pas la valeur par défaut RGBA8.
        for f in 1..=200u32 {
            let s = format_support(f);
            if s & (RENDER_TARGET | SHADER_SAMPLE | DEPTH_STENCIL) == 0 {
                continue;
            }
            let bind = if s & DEPTH_STENCIL != 0 { 0x40 } else { 0 };
            let (internal, _, _) = super::super::dxgi_format_to_gl_bind(f, bind);
            let rgba8 = matches!(f, 27 | 28 | 87 | 88 | 90 | 92);
            if !rgba8 {
                assert_ne!(
                    internal,
                    crate::dlls::gl_bindings::GL_RGBA8,
                    "format {f} annoncé sans traduction OpenGL"
                );
            }
        }
    }
}
