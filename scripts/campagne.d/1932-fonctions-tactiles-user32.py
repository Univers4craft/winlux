# Les 17 fonctions tactiles de user32 absentes arrêtaient un .exe en
# 0xC0000139 ; elles sont servies (tactile.rs) et répondent comme Windows
# sans message PT_TOUCH, l'injection passant par l'autorisation « input » (#1932).
_t = (runtime_dlls / "tactile.rs").read_text(encoding="utf-8")
_m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
_noms = ("InitializeTouchInjection", "InjectTouchInput", "GetPointerTouchInfo",
         "GetPointerTouchInfoHistory", "RegisterPointerInputTarget",
         "UnregisterPointerInputTarget", "RegisterTouchHitTestingWindow",
         "EvaluateProximityToRect", "EvaluateProximityToPolygon",
         "GetCurrentInputMessageSource", "GetCIMSSM", "GetGestureConfig",
         "GetGestureExtraArgs", "CreateSyntheticPointerDevice",
         "InjectSyntheticPointerInput", "DestroySyntheticPointerDevice",
         "GetPointerInputTransform")
ok(all(f'"{n}" => fn ' in _t for n in _noms) and "e.extend(tactile::user32_exports())" in _m,
   "user32 exporte les 17 fonctions tactiles (#1932)")
ok("saisie::inject(" in _t,
   "l'injection tactile passe par la saisie simulée et son autorisation « input » (#1932)")
ok(all(f"fn {t}" in _t for t in ("aucun_pointeur_tactile_1932", "source_du_message_courant_1932",
                                  "contact_principal_en_souris_1932",
                                  "proximite_d_un_rectangle_et_d_un_polygone_1932",
                                  "reglages_de_gestes_et_test_de_toucher_par_fenetre_1932")),
   "tests : erreurs sans toucher, source du message, injection, proximité, gestes (#1932)")
