# Installation sur un Linux ordinaire (#27) : install.sh sur l'hôte, binfmt,
# ./hello.exe direct, association, désinstallation. Demande root (sudo sans
# mot de passe) et les programmes PE de test ; jamais en mode rapide.
if test "$quick" = 1; then
    skip "scripts/test-host-install.sh (#27)" "mode rapide"
elif ! test -f target/x86_64-pc-windows-msvc/release/hello.exe || ! test -x target/release/winlux; then
    skip "scripts/test-host-install.sh (#27)" "runtime ou programmes PE non compilés"
elif ! sudo -n true 2>/dev/null; then
    skip "scripts/test-host-install.sh (#27)" "demande sudo"
else
    step "scripts/test-host-install.sh (#27, install.sh sur l'hôte)" sudo -n sh scripts/test-host-install.sh
fi
