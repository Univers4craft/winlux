# #1244 : IDXGISwapChain1 sur la chaîne d'échange Direct3D 11 (Claude pour Windows, #996).
ok("IID_IDXGI_SWAP_CHAIN1" in (runtime_dlls / "dxgi.rs").read_text(encoding="utf-8")
   and "present1: sc_present1" in (runtime_dlls / "dxgi.rs").read_text(encoding="utf-8")
   and "chaine_d_echange_direct3d11_en_idxgiswapchain1" in (SCRIPTS / "test-all.d" / "1244-idxgiswapchain1.sh").read_text(encoding="utf-8"),
   "La chaîne d'échange Direct3D 11 répond en IDXGISwapChain1 (#1244)")
