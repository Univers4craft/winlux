# GetFileInformationByHandle sur un handle standard redirigé (Python 3.12 x86, #1436).
ok("handle_to_fd(handle)" in (ROOT / "runtime/src/dlls/kernel32.rs").read_text(encoding="utf-8")
   and "(#1436)" in (ROOT / "testc/wow32/crt32.c").read_text(encoding="utf-8"),
   "GetFileInformationByHandle lit le descripteur du handle (#1436)")
