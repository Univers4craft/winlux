# WSARecvFrom / WSASendTo avec OVERLAPPED : E/S asynchrone, jamais de lecture bloquante (#2164).
_o = (ROOT / "runtime/src/dlls/ws2_32/overlapped.rs").read_text(encoding="utf-8")
_w = (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8")
_g = (ROOT / "runtime/src/dlls/games_win32.rs").read_text(encoding="utf-8")
ok("pub(super) unsafe fn recv_from(" in _o and "pub(super) unsafe fn send_to(" in _o, "WSARecvFrom/WSASendTo asynchrones dans overlapped.rs (#2164)")
ok("wsa_recv_from_overlapped(" in _g and "wsa_send_to_overlapped(" in _g, "les exports WSARecvFrom/WSASendTo passent par le chemin OVERLAPPED (#2164)")
ok("fn udp_avec_overlapped_jamais_bloquant" in _w, "test UDP avec OVERLAPPED (#2164)")
