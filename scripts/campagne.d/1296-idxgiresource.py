# #1296 : IDXGIResource sur les textures Direct3D 11 (ANGLE, #996).
ok("fn res_get_shared_handle" in (runtime_dlls / "d3d11_dxgi_resource.rs").read_text(encoding="utf-8")
   and "texture_rend_idxgiresource" in (SCRIPTS / "test-all.d" / "1296-idxgiresource.sh").read_text(encoding="utf-8"),
   "Texture Direct3D 11 en IDXGIResource (#1296)")
