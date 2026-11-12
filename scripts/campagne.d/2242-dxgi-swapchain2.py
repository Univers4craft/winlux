# DXGI : IDXGISwapChain2 et la chaîne attendable pour OBS Studio (#2242).
_d = (ROOT / "runtime/src/dlls/dxgi.rs").read_text()
ok("IID_IDXGI_SWAP_CHAIN2" in _d and "get_frame_latency_waitable_object: sc_get_waitable" in _d,
   "la chaîne d'échange DXGI répond à IDXGISwapChain2 et rend son handle attendable (#2242)")
