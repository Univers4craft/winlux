# #1606 : imports d'iCUE 4.18 (wtsapi32, kernel32, winmm, iphlpapi, ws2_32, shlwapi, shell32, crypt32, divers).
mod_rs = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
wts = (runtime_dlls / "sandbox_dlls.rs").read_text(encoding="utf-8")
ns = (runtime_dlls / "kernel32_namespace.rs").read_text(encoding="utf-8")
tp = (runtime_dlls / "threadpool.rs").read_text(encoding="utf-8")
ic = (runtime_dlls / "icue_win32.rs").read_text(encoding="utf-8")
ok('"WTSOpenServerA" => fn WTSOpenServerA' in wts and '"WTSQuerySessionInformationA"' in wts and '"WTSCloseServer"' in wts,
   "wtsapi32 : WTSOpenServerA, WTSQuerySessionInformationA, WTSCloseServer (#1606)")
ok("kernel32_namespace::exports()" in mod_rs and '"CreatePrivateNamespaceW"' in ns and '"CreateThreadpool" => fn create_threadpool' in tp,
   "kernel32 : pools privés et espaces de noms privés (#1606)")
ok("icue_win32::all()" in mod_rs and '"mixerOpen" => fn mixerOpen' in ic and '"WSAAccept" => fn WSAAccept' in ic
   and '"#487" => fn SHLoadIndirectString' in ic and '"CertGetIssuerCertificateFromStore"' in ic,
   "winmm, ws2_32, shlwapi, crypt32 : imports d'iCUE (#1606)")
