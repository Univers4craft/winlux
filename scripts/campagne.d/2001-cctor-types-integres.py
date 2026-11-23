# #2001 : le constructeur statique qu'un type intégré (String, Boolean…) reçoit
# de notre mscorlib fait attendre les autres fils jusqu'à sa fin, et son échec
# est relevé à chaque accès sous forme de TypeInitializationException
# (ECMA-335 §II.10.5.3.3). Le comportement est gardé par les tests nommés.
f = ROOT / "runtime" / "src" / "clr" / "initialisations.rs"
t = f.read_text(encoding="utf-8") if f.exists() else ""
for nom in ("le_second_fil_attend_la_fin_du_constructeur_statique_2001",
            "l_echec_est_releve_a_chaque_acces_2001",
            "deux_initialisations_croisees_ne_se_bloquent_pas_2001"):
    ok(f"fn {nom}(" in t, f"constructeur statique des types intégrés : {nom} (#2001)")
m = (ROOT / "bcl" / "mscorlib" / "Temps.cs").read_text(encoding="utf-8")
ok("class TypeInitializationException" in m, "notre mscorlib déclare TypeInitializationException (#2001)")
