# #3322 : chaque lancement d'un programme Windows laissait un fichier vide
# /tmp/winlux-journal-<pid>.log. Le journal par processus ne naît plus qu'à
# sa première ligne.
step "Journal par processus : créé seulement à la première écriture (#3322)" cargo test -q -p winlux --bin winlux journal_par_processus_cree_seulement_a_la_premiere_ecriture_3322
if test -x target/release/winlux && test -f target/x86_64-pc-windows-msvc/release/hello.exe; then
    step "hello.exe lancé trois fois : aucun journal vide dans /tmp (#3322)" python3 -B scripts/test-journal-par-processus.py
else
    skip "hello.exe lancé trois fois : aucun journal vide dans /tmp (#3322)" "runtime ou programmes PE non compilés"
fi
