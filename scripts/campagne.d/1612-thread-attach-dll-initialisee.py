# DLL_THREAD_ATTACH/DETACH seulement pour les DLL déjà initialisées : plus de rappel TLS de SDL3.dll en cours de chargement (#1612).
ok(".filter(|m| m.initialized)" in (ROOT / "runtime/src/process.rs").read_text(encoding="utf-8"),
   "rappels de thread réservés aux DLL initialisées (#1612)")
