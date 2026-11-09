# Une socket n'est transmise à un enfant que marquée héritable : socket(),
# WSASocket et accept() posent SOCK_CLOEXEC (#1347).
ok("libc::SOCK_CLOEXEC, protocol" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8")
   and "libc::accept4(" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8")
   and "fn sockets_non_heritables_par_defaut_et_fd_accept_vu_par_kernel32" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8"),
   "sockets non héritables par défaut (#1347)")
