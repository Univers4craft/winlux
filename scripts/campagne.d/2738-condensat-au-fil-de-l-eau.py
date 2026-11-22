# #2738 : BCryptHashData et CryptHashData condensent au fil des appels, avec
# un état de taille fixe (l'outil de déploiement d'Office condense un fichier
# de 2,9 Go et mourait sur une allocation de 1 Gio).
c = (runtime_dlls / "condensat.rs").read_text(encoding="utf-8")
b = (runtime_dlls / "bcrypt.rs").read_text(encoding="utf-8")
a = (runtime_dlls / "advapi32_crypto.rs").read_text(encoding="utf-8")
ok("fn condensat_au_fil_de_l_eau_2738" in c
   and "type HashState = crate::dlls::condensat::Condensat;" in b
   and "flux: Option<super::condensat::Condensat>" in a,
   "Condensats BCrypt et CryptoAPI au fil de l'eau, sans garder les données (#2738)")
