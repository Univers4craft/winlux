# #1296 : une texture Direct3D 11 répond à IDXGIResource (ANGLE, Claude pour Windows).
step "Texture Direct3D 11 en IDXGIResource (#1296)" cargo test -q -p winlux -- texture_rend_idxgiresource
