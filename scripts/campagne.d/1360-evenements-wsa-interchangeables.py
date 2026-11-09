# SetEvent/ResetEvent/CloseHandle agissent sur un événement de
# WSACreateEvent (hEvent d'un WSASend de Chromium), WSASetEvent et
# WSAResetEvent sur un événement de CreateEvent ; les handles sont des
# multiples de 4, le bit 0 de hEvent gardant son sens (#1360).
ok("ws2_32::wsa_set_event(h)" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "NEXT_HANDLE.fetch_add(4" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "fn evenements_wsa_et_kernel32_interchangeables" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8"),
   "événements WSA et kernel32 interchangeables, handles multiples de 4 (#1360)")
