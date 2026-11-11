# #1701 : System.Lazy<T> écrit en C# dans la mscorlib Winlux
# (Corsair.Service.exe d'iCUE 4.18 lisait Lazy<T>.Value).
# #1702 : un délégué de notre mscorlib a la place de ses champs moteur.
# #1703 : une BCL absente est signalée.
bcl_racine = ROOT / "bcl"
bcl_lazy = (bcl_racine / "mscorlib" / "Paresseux.cs").read_text(encoding="utf-8")
ok("public class Lazy<T>" in bcl_lazy and "public delegate TResult Func<TResult>();" in bcl_lazy
   and "Code de sortie attendu : 63." in (bcl_racine / "temoins" / "Paresseux.cs").read_text(encoding="utf-8"),
   "BCL C# : Lazy<T>, Func<T> et leur témoin Paresseux (#1701)")
clr_dir = runtime_dlls.parent / "clr"
ok("is_delegate_root" in (clr_dir / "typesys.rs").read_text(encoding="utf-8")
   and "pub(super) const OBJECT_SIZE: usize = LIST + 8;" in (clr_dir / "delegates.rs").read_text(encoding="utf-8"),
   "CLR : System.Delegate réserve cible, méthode et liste d'invocation (#1702)")
ok("BCL Winlux absente" in (clr_dir / "domain.rs").read_text(encoding="utf-8"),
   "CLR : une BCL C# absente est signalée dans le journal (#1703)")
