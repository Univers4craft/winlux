# #531 / #1175 : cibles de rendu multiples (G-buffer), OMGetRenderTargets et
# RSGetViewports rendent l'état lié.
d3d11_rs = (runtime_dlls / "d3d11.rs").read_text(encoding="utf-8")
hlsl_c = (ROOT / "testc/msvc/hlsl_msvc.c").read_text(encoding="utf-8")
ok("unsafe fn bind_mrt(" in d3d11_rs and "rs_get_viewports: ctx_rs_get_viewports" in d3d11_rs
   and "ps_mrt" in hlsl_c and "#1175" in hlsl_c
   and "#531" in (SCRIPTS / "test-mingw.sh").read_text(encoding="utf-8"),
   "Direct3D 11 : cibles multiples, OMGetRenderTargets et RSGetViewports (#531, #1175)")
