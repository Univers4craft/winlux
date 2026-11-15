# virt-manager de la Boutique se connecte à qemu:///session (#2190).
_o = (ROOT / "integration/native/glib-schemas/90_winlux-virt-manager.gschema.override").read_text(encoding="utf-8")
_st = (ROOT / "scripts/stage-desktop-components.sh").read_text(encoding="utf-8")
ok("[org.virt-manager.virt-manager.connections]" in _o and "uris=['qemu:///session']" in _o
   and "autoconnect=['qemu:///session']" in _o and "qemu:///system" not in _o.split("]", 1)[1],
   "virt-manager : session de l'utilisateur par défaut, jamais qemu:///system (#2190)")
ok("integration/native/glib-schemas/*.gschema.override" in _st
   and _st.index("glib-schemas/*.gschema.override") < _st.index('glib-compile-schemas" "$stage/usr/share/glib-2.0/schemas"'),
   "surcharges GSettings installées avant la compilation des schémas (#2190)")
