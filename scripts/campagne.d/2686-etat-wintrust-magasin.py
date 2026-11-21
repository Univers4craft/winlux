# #2686 : les certificats de l'état de WinVerifyTrust partagent un magasin,
# où CertGetCertificateChain trouve les autorités intermédiaires jointes.
f = (ROOT / "runtime/src/dlls/wintrust.rs").read_text(encoding="utf-8")
ok("fn certificats_de_l_etat_dans_un_seul_magasin_2686" in f,
   "Signature : certificats de l'état dans un seul magasin (#2686)")
