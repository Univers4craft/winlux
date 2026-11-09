# WaitForSingleObject/WaitForMultipleObjects voient l'événement d'une socket
# WSAEventSelect : WinSCP (PuTTY) attendait FD_READ indéfiniment (#1340).
ok("ws2_32::has_socket_assoc" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "fn attente_kernel32_sur_l_evenement_d_une_socket" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8"),
   "attentes kernel32 sur les sockets WSAEventSelect (#1340)")
