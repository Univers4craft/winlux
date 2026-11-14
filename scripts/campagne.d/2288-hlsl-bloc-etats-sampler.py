# HLSL : « SamplerState s { état = valeur; }; » accepté, pixel shaders d'OBS Studio (#2288).
# Le comportement est vérifié par le test hlsl::tests::sampler_avec_bloc_d_etats.
_p = (ROOT / "runtime/src/dlls/hlsl/parse.rs").read_text()
_t = (ROOT / "runtime/src/dlls/hlsl/tests.rs").read_text()
ok("fn skip_state_block" in _p and "fn sampler_avec_bloc_d_etats" in _t,
   "le compilateur HLSL accepte le bloc d'états d'un SamplerState (#2288)")
