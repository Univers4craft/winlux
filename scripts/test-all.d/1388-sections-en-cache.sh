# #1388 : une grande section mal alignée dans le .exe (code de claude.exe)
# est projetée depuis une copie alignée en cache, partagée par les processus.
step "Section alignée en cache, reprise et nettoyée (#1388)" cargo test -q -p winlux -- section_alignee_creee_puis_reprise_et_ancienne_version_effacee
