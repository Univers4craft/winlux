# Une fenêtre HWND_MESSAGE n'a pas de fenêtre X (pygame 32 bits en créait 27 par seconde) (#1586).
ok("fn fenetre_message_seulement" in (ROOT / "runtime/src/dlls/user32.rs").read_text(encoding="utf-8"),
   "fenêtres HWND_MESSAGE sans fenêtre X (#1586)")
