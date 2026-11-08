# #1179 : mémoire des tas D3D12, alias des ressources placées et test de profondeur.
ok("Cmd::Alias(before, after) => alias(dev, *before, *after)" in (runtime_dlls / "d3d12.rs").read_text(encoding="utf-8")
   and "Some((h, off)) => {" in (runtime_dlls / "d3d12.rs").read_text(encoding="utf-8")
   and "pDepthAttachment: depth_ptr" in (runtime_dlls / "vk_backend.rs").read_text(encoding="utf-8")
   and "profondeur_placee_" in (SCRIPTS / "test-all.d" / "1179-tas-d3d12.sh").read_text(encoding="utf-8"),
   "Direct3D 12 : tas partagés, alias et profondeur des ressources placées (#1179)")
