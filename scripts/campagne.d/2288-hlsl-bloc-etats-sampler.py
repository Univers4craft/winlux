# HLSL : « SamplerState s { état = valeur; }; » accepté, pixel shaders d'OBS Studio (#2288).
_p = (ROOT / "runtime/src/dlls/hlsl/parse.rs").read_text()
ok("fn skip_state_block" in _p and "matches!(base, Ty::Sampler(_)) && self.is(\"{\")" in _p,
   "le compilateur HLSL accepte le bloc d'états d'un SamplerState (#2288)")
