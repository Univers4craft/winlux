# #1247 : D3DReflect (d3dcompiler_47) décrit un shader DXBC
# (ID3D11ShaderReflection), pour les effets Direct2D de Paint.NET.
step "D3DReflect (#1247)" cargo test -q -p winlux -- d3dreflect_decrit_un_pixel_shader_d2d d3dreflect_refuse_un_bloc_qui_n_est_pas_dxbc
