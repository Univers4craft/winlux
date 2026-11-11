# Logiciels à anti-triche noyau : installation refusée avant toute écriture, message en 4 langues, liste documentée (#1720).
_a = (ROOT / "runtime/src/anticheat.rs").read_text(encoding="utf-8")
_main = (ROOT / "runtime/src/main.rs").read_text(encoding="utf-8")
_msi = (ROOT / "runtime/src/msi/mod.rs").read_text(encoding="utf-8")
_setup = (ROOT / "runtime/src/setup/mod.rs").read_text(encoding="utf-8")
_svc = (ROOT / "runtime/src/dlls/advapi32_services.rs").read_text(encoding="utf-8")
_open = (ROOT / "integration/winlux-open").read_text(encoding="utf-8")
_doc = (ROOT / "docs/COMPATIBILITE.md").read_text(encoding="utf-8")
ok(_main.find("anticheat::check_launch(&exe)") != -1
   and _main.find("anticheat::check_launch(&exe)") < _main.find("setup::launch(&exe, &rest)"),
   "lancement d'un .exe à anti-triche noyau refusé avant l'installateur (#1720)")
ok("anticheat::check_launch(&host_path(&p))" in _msi and "anticheat::check_launch(&path)" in _setup
   and "anticheat::in_package(&host)" in _setup,
   "msiexec, --setup et l'assistant refusent aussi (#1720)")
ok("anticheat::by_driver(&command)" in _svc, "filet : service pilote d'anti-triche jamais inscrit (#1720)")
ok(all(m in _a for m in ("sécurité de votre compte", "security of your account",
                         "Sicherheit Ihres Kontos", "seguridad de su cuenta")),
   "message de refus en français, anglais, allemand et espagnol (#1720)")
ok("easyanticheat" not in _a.split("pub const LIST")[1].split("];")[0].lower()
   and "bedaisy" not in _a.split("pub const LIST")[1].split("];")[0].lower(),
   "Easy Anti-Cheat et BattlEye ne sont jamais bloqués (#1720)")
ok("refused_by_anticheat && exit" in _open, "winlux-open n'ajoute pas de boîte d'échec après un refus (#1720)")
ok("Anti-triche noyau : installation refusée (#1720)" in _doc and "vgk.sys" in _doc,
   "liste des anti-triche refusés documentée avec ses sources (#1720)")
ok("add_button(button" in _a and "buttons=Gtk.ButtonsType.NONE" in _a and '"Aceptar"' in _a,
   "bouton de la boîte de refus dans la langue de l'interface (#1776)")
