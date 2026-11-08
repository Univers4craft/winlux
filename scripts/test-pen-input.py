#!/usr/bin/env python3
"""#1199 : stylet et tablette graphique (pression, inclinaison).

Vérifie sans compiler que le runtime :
- relève les valuateurs XInput 2 du stylet (événements bruts sur la racine) ;
- fournit l'API WM_POINTER de user32 (GetPointerInfo, GetPointerPenInfo,
  EnableMouseInPointer…) et GetMessageExtraInfo ;
- retient le message souris promu quand la fenêtre a traité le WM_POINTER* ;
- fournit une wintab32.dll maison (WTInfo, WTOpen, WTPacketsGet…).
"""
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DLLS = ROOT / "runtime/src/dlls"
fails = []


def need(cond, msg):
    if not cond:
        fails.append(msg)


pointer = (DLLS / "pointer.rs").read_text(encoding="utf-8")
for name in ("GetPointerInfo", "GetPointerPenInfo", "GetPointerType", "GetPointerFrameInfo",
             "GetPointerPenInfoHistory", "EnableMouseInPointer", "IsMouseInPointerEnabled",
             "GetPointerDevices", "GetMessageExtraInfo"):
    need(f'"{name}" =>' in pointer, f"user32 : {name} absent")
need("0xFF51_5700" in pointer, "signature MI_WP_SIGNATURE absente")

wintab = (DLLS / "wintab32.rs").read_text(encoding="utf-8")
for name in ("WTInfoA", "WTInfoW", "WTOpenA", "WTOpenW", "WTClose", "WTPacketsGet",
             "WTPacket", "WTEnable", "WTOverlap", "WTQueueSizeSet"):
    need(f'"{name}" =>' in wintab, f"wintab32 : {name} absent")

mod = (DLLS / "mod.rs").read_text(encoding="utf-8")
need('name: "wintab32"' in mod, "wintab32.dll non déclarée")
need("e.extend(pointer::user32_exports())" in mod, "API pointeur non ajoutée à user32")

x11 = (DLLS / "gui_x11.rs").read_text(encoding="utf-8")
need("XIEventMask::RAW_MOTION" in x11 and "b\"Abs Pressure\"" in x11,
     "événements bruts XInput du stylet non demandés")
need("Event::XinputRawButtonPress" in x11, "boutons bruts du stylet ignorés")

gui = (DLLS / "gui.rs").read_text(encoding="utf-8")
need("pointer::pointer_for_mouse" in gui, "WM_POINTER* non produit avant le message souris")
need("pointer::on_take(&m)" in gui and "pointer::on_default(message)" in gui,
     "promotion souris de Windows non reproduite")

games = (DLLS / "games_win32.rs").read_text(encoding="utf-8")
need('"GetMessageExtraInfo" => fn zero' not in games, "GetMessageExtraInfo encore à zéro")

if fails:
    for f in fails:
        print("ÉCHEC :", f)
    raise SystemExit(1)
print("OK : stylet (WM_POINTER, pression, inclinaison) et wintab32 (#1199).")
