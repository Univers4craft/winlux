# #1404 : les sections d'une même version d'un .exe restent ensemble dans
# le cache ; seules celles d'une version précédente sont effacées.
step "Sections d'un même .exe conservées dans le cache (#1404)" cargo test -q -p winlux -- section_alignee_creee_puis_reprise_et_ancienne_version_effacee
