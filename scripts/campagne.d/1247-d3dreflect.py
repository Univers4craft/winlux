# #1247 : D3DReflect rend un ID3D11ShaderReflection (ComputeSharp.D2D1, Paint.NET).
ok('"D3DReflect"=>fn reflect::d3d_reflect' in (runtime_dlls / "d3dcompiler.rs").read_text(encoding="utf-8")
   and "fn get_min_feature_level" in (runtime_dlls / "d3dcompiler_reflect.rs").read_text(encoding="utf-8")
   and "d3dreflect_decrit_un_pixel_shader_d2d" in (SCRIPTS / "test-all.d" / "1247-d3dreflect.sh").read_text(encoding="utf-8"),
   "D3DReflect décrit un shader DXBC (#1247)")
