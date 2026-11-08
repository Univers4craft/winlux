#!/usr/bin/python3
"""Paint.NET 5.1 et les programmes à Direct2D 1.1 + effets personnalisés (#561).

Vérifie, sans compiler, que les pièces dont Paint.NET a besoin au démarrage
sont en place et gardent la taille de table de méthodes publiée par Windows :
Direct3D 11.1 à 11.4, Direct2D 1.1 à 1.3 (fabrique 7, contexte de dessin),
effets personnalisés (ID2D1EffectContext, ID2D1TransformGraph,
ID2D1DrawInfo, pixel shaders), gestionnaire d'animations Windows,
GdipImageForceValidation, et DirectWrite 3 à 7 (#984 : jeux de polices,
collections 1 à 3, faces 1 à 6, paramètres de rendu 3, repli
personnalisé). Les tests unitaires Rust
(cargo test) contrôlent le comportement de chaque pièce.
"""
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
DLLS = ROOT / "runtime/src/dlls"
failures = 0


def ok(condition, label):
    global failures
    print(("  OK     " if condition else "ÉCHEC : ") + label)
    if not condition:
        failures += 1


def source(name):
    return (DLLS / name).read_text(encoding="utf-8")


def vtable(text, name):
    m = re.search(rf"static {name}: VTable<(\d+)>", text)
    return int(m.group(1)) if m else None


d2d1 = source("d2d1.rs")
d2d1_1 = source("d2d1_1.rs")
effects = source("d2d1_effects.rs")
shader = source("d2d1_shader.rs")
anim = source("uianimation.rs")

ok(vtable(d2d1, "FACTORY_VT") == 33, "ID2D1Factory7 : 33 méthodes")
ok("DC_VT: VTable<120>" in d2d1_1, "ID2D1DeviceContext6 : 120 méthodes")
ok(vtable(effects, "CONTEXT_VT") == 24, "ID2D1EffectContext : 24 méthodes")
ok(vtable(effects, "GRAPH_VT") == 12, "ID2D1TransformGraph : 12 méthodes")
ok(vtable(effects, "DRAW_INFO_VT") == 12, "ID2D1DrawInfo : 12 méthodes")
ok("factory_effect_properties" in d2d1 and "unsupported!(\"GetEffectProperties\")" not in d2d1,
   "ID2D1Factory1::GetEffectProperties fourni")
ok("D2DERR_EFFECT_IS_NOT_REGISTERED" in d2d1_1 and "fn is_builtin_effect" in d2d1_1,
   "CreateEffect refuse un effet non enregistré, comme Direct2D")
ok("custom::instantiate" in d2d1_1 and "fn bind(" in effects and "fn parse(" in effects,
   "effet personnalisé : description XML, liaisons de propriétés, code de l'application appelé")
ok("SCENE_POSITION" in shader and "TEXCOORD" in shader and "dxbc::translate" in shader,
   "pixel shaders d'effet : conventions de d2d1effecthelpers, traduction DXBC → GLSL")

ok(vtable(anim, "MANAGER_VT") == 22 and vtable(anim, "TIMER_VT") == 10
   and vtable(anim, "LIBRARY_VT") == 15 and vtable(anim, "FACTORY_VT") == 4,
   "gestionnaire d'animations : tables de UIAnimation.dll (v1)")
ok("uianimation::create_builtin" in source("ole32.rs"),
   "CoCreateInstance connaît UIAnimationManager, Timer, TransitionLibrary et TransitionFactory")

ok("ext: ext::DeviceExt" in source("d3d11.rs") and "ext: ext::ContextExt" in source("d3d11.rs"),
   "ID3D11Device5 et ID3D11DeviceContext4")
ok("IID_ANNOTATION" in source("d3d11_ext.rs") and "ext::annotation_for" in source("d3d11.rs"),
   "ID3DUserDefinedAnnotation servi par le contexte (ANGLE, Chromium)")
ok("DXGI_ADAPTER_DESC2_SIZE: usize = 320" in source("dxgi.rs"),
   "IDXGIAdapter4::GetDesc3 écrit 320 octets, pas 324")

ok("v11::context_view(this, out)" in d2d1 and "winlux_d2d_context_view_vt" in d2d1_1
   and ".rept 120" in d2d1_1,
   "anciennes cibles : QueryInterface vers ID2D1DeviceContext6 rend une vue à 120 relais (#1085)")

brush = source("d2d1_brush.rs")
ok('unsupported!("CreateBitmapBrush")' not in d2d1 and 'unsupported!("CreateBitmapBrush")' not in d2d1_1
   and 'unsupported!("CreateImageBrush")' not in d2d1_1
   and 'unsupported!("CreateLinearGradientBrush")' not in d2d1
   and "fn bitmap_and_gradient_brushes_paint" in d2d1 and "BITMAP_BRUSH_VT: VTable<18>" in brush,
   "pinceaux bitmap, image et dégradés réels, transformation des pinceaux (#1124, Notepad++)")

dwrite = source("dwrite.rs")
sets = source("dwrite_sets.rs")
fallback = source("dwrite_fallback.rs")
custom = source("dwrite_custom.rs")
ok(vtable(dwrite, "FACTORY_VT") == 57 and "IID_DWRITE_FACTORY7" in sets,
   "IDWriteFactory7 : 57 méthodes, demandé par Paint.NET 5.1")
ok(vtable(dwrite, "COLLECTION_VT") == 14 and vtable(dwrite, "FAMILY_VT") == 14
   and vtable(dwrite, "FONT_VT") == 24 and vtable(dwrite, "FACE_VT") == 60,
   "IDWriteFontCollection3, IDWriteFontFamily2, IDWriteFont3, IDWriteFontFace6")
ok(vtable(sets, "SET_VT") == 32 and vtable(sets, "REF_VT") == 20
   and vtable(sets, "BUILDER_VT") == 10 and vtable(sets, "LIST_VT") == 10,
   "IDWriteFontSet4, IDWriteFontFaceReference1, IDWriteFontSetBuilder2, IDWriteFontList2")
ok(vtable(fallback, "PARAMS_VT") == 11 and "IID_RENDERING_PARAMS3" in fallback
   and "E_NOTIMPL" not in fallback and vtable(fallback, "BUILDER_VT") == 6,
   "IDWriteRenderingParams3, CreateGlyphRunAnalysis et CreateFontFallbackBuilder réels")
ok("fn raw_table" in source("sfnt.rs") and "raw_table(&tag.to_le_bytes())" in dwrite,
   "TryGetFontTable rend les tables OpenType")

ok("// CreateCustomFontCollection" in dwrite and "not_implemented as *const (), // CreateCustom" not in dwrite
   and "fn custom_file_reference(" in custom and "fn read_stream(" in custom,
   "DirectWrite : collections et fichiers de polices fournis par l'appli (#1075)")

text = source("dwrite_text.rs")
ok(vtable(dwrite, "FORMAT_VT") == 43 and vtable(dwrite, "LAYOUT_VT") == 89
   and "TEXT_FORMAT3" in text and "TEXT_LAYOUT4" in text,
   "IDWriteTextFormat1-3 et IDWriteTextLayout1-4 servis par CreateTextFormat et CreateTextLayout (#1118)")

gdiplus = source("gdiplus.rs")
ok("\"GdipImageForceValidation\" => fn image_force_validation" in gdiplus,
   "GdipImageForceValidation (Image.FromStream avec validation) ne répond plus NotImplemented")

if failures:
    sys.exit(1)
print("Paint.NET / Direct2D 1.1 : tout est en place.")
