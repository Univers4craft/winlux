# #2694 : CertFindCertificateInStore(CERT_FIND_SUBJECT_CERT) retrouve un
# certificat par l'émetteur et le numéro de série d'un CERT_INFO.
# Gardé par le test unitaire nommé.
t = (ROOT / "runtime/src/dlls/crypt32.rs").read_text(encoding="utf-8")
ok("fn certificat_retrouve_par_emetteur_et_numero_de_serie" in t, "crypt32 : certificat retrouvé par émetteur et numéro de série (#2694)")
