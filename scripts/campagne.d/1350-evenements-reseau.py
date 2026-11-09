# WSAEnumNetworkEvents : FD_ACCEPT seulement sur une socket d'écoute, erreur
# de connexion dans iErrorCode[FD_CONNECT_BIT] ; WinSCP appelait accept() sur
# sa connexion SSH (#1350).
ok("fn socket_listening" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8")
   and "fn evenements_reseau_selon_le_role_de_la_socket" in (ROOT / "runtime/src/dlls/ws2_32.rs").read_text(encoding="utf-8"),
   "WSAEnumNetworkEvents selon le rôle de la socket (#1350)")
