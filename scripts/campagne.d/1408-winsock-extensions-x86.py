# Winsock x86 : AcceptEx, ConnectEx, GetAcceptExSockaddrs et DisconnectEx
# rendus par WSAIoctl, OVERLAPPED x86 (#1408).
ok("fn extension_address" in (ROOT / "runtime/src/dlls/ws2_32/overlapped.rs").read_text(encoding="utf-8")
   and "pub(crate) unsafe fn ov_set" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and (ROOT / "testc/wow32/net32.c").is_file(),
   "fonctions d'extension Winsock en x86 (#1408)")
