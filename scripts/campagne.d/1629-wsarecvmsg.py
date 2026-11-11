# #1629 : WSARecvMsg et WSASendMsg servis comme fonctions d'extension (Qt Network, iCUE).
ov = (runtime_dlls / "ws2_32" / "overlapped.rs").read_text(encoding="utf-8")
ok("WSAID_WSARECVMSG" in ov and "WSAID_WSASENDMSG" in ov and "msg::wsa_recv_msg" in ov,
   "WSAIoctl rend WSARecvMsg et WSASendMsg (#1629)")
