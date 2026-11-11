# #1765 : wldap32.dll intégrée (libcurl d'OBS), client LDAP sans connexion.
w = (runtime_dlls / "wldap32.rs").read_text(encoding="utf-8")
m = (runtime_dlls / "mod.rs").read_text(encoding="utf-8")
ok('name: "wldap32"' in m and "wldap32::ORDINALS" in m,
   "wldap32.dll est intégrée et servie par numéro d'ordre (#1765)")
ok("(144, \"ldap_initA\")" in w and "LDAP_NOT_SUPPORTED" in w,
   "ldap_init rend NULL avec LDAP_NOT_SUPPORTED, sans faux succès (#1765)")
