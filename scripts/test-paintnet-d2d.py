#!/usr/bin/python3
"""Paint.NET 5.1 et les programmes à Direct2D 1.1 + effets personnalisés (#561).

Vérifie, sans compiler, que les pièces dont Paint.NET a besoin au démarrage
sont en place et gardent la taille de table de méthodes publiée par Windows :
Direct3D 11.1 à 11.4, Direct2D 1.1 à 1.3 (fabrique 7, contexte de dessin),
effets personnalisés (ID2D1EffectContext, ID2D1TransformGraph,
ID2D1DrawInfo, pixel shaders), gestionnaire d'animations Windows,
et GdipImageForceValidation. Les tests unitaires Rust
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
ok("DXGI_ADAPTER_DESC2_SIZE: usize = 320" in source("dxgi.rs"),
   "IDXGIAdapter4::GetDesc3 écrit 320 octets, pas 324")

gdiplus = source("gdiplus.rs")
ok("\"GdipImageForceValidation\" => fn image_force_validation" in gdiplus,
   "GdipImageForceValidation (Image.FromStream avec validation) ne répond plus NotImplemented")

if failures:
    sys.exit(1)
print("Paint.NET / Direct2D 1.1 : tout est en place.")
