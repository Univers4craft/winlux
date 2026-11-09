# #1290 : Texture2D/Buffer::GetDevice rendent l'appareil parent (Claude, #996).
_src = (runtime_dlls / "d3d11.rs").read_text(encoding="utf-8")
ok("fn get_device(_this: *mut Self, _out: *mut *mut c_void) {}" not in _src
   and "texture_et_tampon_rendent_leur_appareil" in (SCRIPTS / "test-all.d" / "1290-getdevice-texture.sh").read_text(encoding="utf-8"),
   "GetDevice des textures et tampons Direct3D 11 (#1290)")
