# X509Chain.Build de .NET (SslStream) remplit un magasin par
# CertAddCertificateLinkToStore avant CertGetCertificateChain.
ok('"CertAddCertificateLinkToStore" => fn CertAddCertificateLinkToStore' in (runtime_dlls / "crypt32.rs").read_text()
   and "lien_vers_un_certificat_dans_un_magasin_memoire" in (runtime_dlls / "crypt32.rs").read_text()
   and "CertAddCertificateLinkToStore pour X509Chain (#1207)" in (SCRIPTS / "test-all.sh").read_text(),
   "HTTPS .NET : CertAddCertificateLinkToStore pour la vérification du certificat (#1207)")
