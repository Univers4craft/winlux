# Ressources de manifeste : le moteur lit la table ManifestResource et le
# répertoire Resources, notre mscorlib sert GetManifestResourceStream et
# GetManifestResourceNames ; témoin Ressources (code 200, #2824).
_m = (ROOT / "runtime/src/clr/metadata.rs").read_text(encoding="utf-8")
_r = (ROOT / "bcl/mscorlib/Reflexion.cs").read_text(encoding="utf-8")
_t = (ROOT / "bcl/temoins/Ressources.cs").read_text(encoding="utf-8")
ok("pub fn manifest_resource(&self, name: &str)" in _m
   and "public virtual System.IO.Stream GetManifestResourceStream(Type type, string name)" in _r
   and "Ressource incorporée : " in _t and "Code de sortie attendu : 200." in _t,
   "Ressources de manifeste : GetManifestResourceStream servi par le moteur, témoin (#2824)")
