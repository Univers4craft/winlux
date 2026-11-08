# #1206 : Paint.NET demande ID2D1EffectContext2 dans Initialize de ses effets.
ok("IID_EFFECT_CONTEXT2" in (runtime_dlls / "d2d1_effects.rs").read_text(encoding="utf-8")
   and "initialize_obtient_le_contexte_d_effet_2" in (SCRIPTS / "test-all.sh").read_text(encoding="utf-8"),
   "Direct2D : effets personnalisés, contexte d'effet ID2D1EffectContext1/2 (#1206)")
