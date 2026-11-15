# #2007 : IObserver, IObservable, IProgress et WeakReference dans notre
# mscorlib.
# #2008 : réflexion sur les membres (GetMethod, Invoke, propriétés, champs,
# attributs personnalisés), Activator et Enum ; témoin Membres.
# #2009 : les méthodes de System.Enum reçoivent l'objet encadré.
# #2010 : Enum.ToString rend le nom de la valeur.
bcl = ROOT / "bcl" / "mscorlib"
ok("public interface IObserver<in T>" in (bcl / "Observateurs.cs").read_text(encoding="utf-8"),
   "CLR : IObserver, IObservable, IProgress et WeakReference dans notre mscorlib (#2007)")
membres = (bcl / "Membres.cs").read_text(encoding="utf-8")
ok("internal sealed class RuntimeMethodInfo : MethodInfo" in membres and "public static class Activator" in membres
   and "fn custom_attributes" in (runtime_dlls.parent / "clr" / "membres.rs").read_text(encoding="utf-8")
   and "Code de sortie attendu : 200." in (ROOT / "bcl" / "temoins" / "Membres.cs").read_text(encoding="utf-8"),
   "CLR : réflexion sur les membres, attributs personnalisés, Activator et Enum, témoin Membres (#2008)")
ok("o != types::TYPE_ENUM" in (runtime_dlls.parent / "clr" / "il.rs").read_text(encoding="utf-8"),
   "CLR : une méthode de System.Enum reçoit l'objet encadré (#2009)")
ok("public override string ToString() { return Texte(GetType()" in membres,
   "CLR : Enum.ToString rend le nom de la valeur (#2010)")
