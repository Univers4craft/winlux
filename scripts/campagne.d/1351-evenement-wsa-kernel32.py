# WaitForSingleObject/WaitForMultipleObjects acceptent un événement de
# WSACreateEvent : serveur DevTools de Chromium sur le port 9222 (#1351).
ok("fn wait_wsa_events" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "fn evenement_wsa_attendu_par_kernel32" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8"),
   "événements WSACreateEvent attendus par kernel32 (#1351)")
