# #1390 : mswsock.dll servie par ws2_32, avec TransmitFile et WSARecvEx.
dlls_mod = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ws2 = (runtime_dlls / "ws2_32.rs").read_text(encoding="utf-8")
ov = (runtime_dlls / "ws2_32" / "overlapped.rs").read_text(encoding="utf-8")
ok('base == "mswsock"' in dlls_mod, "mswsock.dll renvoyée vers ws2_32 (#1390)")
ok('"TransmitFile"=>fn overlapped::transmit_file' in ws2 and '"WSARecvEx"=>fn wsa_recv_ex' in ws2
   and '"AcceptEx"=>fn overlapped::accept_ex' in ws2,
   "mswsock : AcceptEx, TransmitFile et WSARecvEx exportées (#1390)")
ok("WSAID_TRANSMITFILE" in ov, "TransmitFile rendue aussi par WSAIoctl (#1390)")
