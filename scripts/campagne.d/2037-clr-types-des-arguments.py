# #2037 : une primitive du moteur ne lit comme chaîne qu'une System.String ;
# MethodInfo.Invoke, ConstructorInfo.Invoke et FieldInfo.GetValue/SetValue
# vérifient le type des arguments et de la cible ; Array.SetValue, IList et
# Array.Copy vérifient l'objet rangé dans un tableau de valeurs.
clr = runtime_dlls.parent / "clr"
ok("fn string_units" in (clr / "bcl.rs").read_text(encoding="utf-8")
   and "gc::type_of(ptr) != Some(TYPE_STRING)" in (clr / "bcl.rs").read_text(encoding="utf-8"),
   "CLR : seule une System.String se lit comme une chaîne (#2037)")
ok("fn argument_fits" in (clr / "membres.rs").read_text(encoding="utf-8")
   and "fn target_fits" in (clr / "membres.rs").read_text(encoding="utf-8")
   and "La 14 passe à Invoke" in (ROOT / "bcl" / "temoins" / "Membres.cs").read_text(encoding="utf-8"),
   "CLR : Invoke et FieldInfo vérifient le type des arguments et de la cible (#2037)")
ok("fn unbox_elem" in (clr / "tableaux.rs").read_text(encoding="utf-8")
   and "ArrayTypeMismatchException" in (ROOT / "bcl" / "mscorlib" / "Conversion.cs").read_text(encoding="utf-8"),
   "CLR : un tableau de valeurs refuse un objet d'un autre type (#2037)")
