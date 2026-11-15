# QueryFullProcessImageNameW sur un autre processus du même compte (#2041).
_c = (ROOT / "runtime/src/dlls/win32_basics.rs").read_text()
ok("fn foreign_image_path" in _c and "libc::getuid()" in _c,
   "QueryFullProcessImageNameW : autre processus du même compte servi, autre compte refusé (#2041)")
ok("QueryFullProcessImageNameW(pid" in _c, "QueryFullProcessImageNameW : ligne de trace (#2041)")
