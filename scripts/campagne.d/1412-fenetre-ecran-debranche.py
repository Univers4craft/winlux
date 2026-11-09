# #1412 : quand un écran est débranché, les fenêtres qui n'y sont plus
# visibles reviennent sur l'écran restant (sans toucher aux fenêtres agrandies).
rescue_rs = (ROOT / "runtime" / "src" / "display" / "rescue.rs").read_text(encoding="utf-8")
display_rs = (ROOT / "runtime" / "src" / "display" / "mod.rs").read_text(encoding="utf-8")
ok("fn rescue_frame" in rescue_rs and "_NET_MOVERESIZE_WINDOW" in rescue_rs
   and "rescue::bring_back" in display_rs and (SCRIPTS / "test-display-bring-back.sh").is_file(),
   "Fenêtre restée sur un écran débranché ramenée sur l'écran restant (#1412)")
