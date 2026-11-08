# #1244 : la chaîne d'échange Direct3D 11 répond en IDXGISwapChain1
# (Present1, GetDesc1, GetHwnd, GetFrameStatistics), pour ANGLE et Chromium.
step "IDXGISwapChain1 (#1244)" cargo test -q -p winlux -- chaine_d_echange_direct3d11_en_idxgiswapchain1 tables_dxgi_aux_tailles_de_windows
