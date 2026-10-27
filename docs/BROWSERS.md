# Navigateurs Linux dans Winlux

Le catalogue contient les paquets AMD64 officiels de Microsoft Edge, Google Chrome
et Mozilla Firefox. Edge est installé depuis ce catalogue au premier démarrage,
avant l’affichage du bureau. Les démarrages suivants conservent l’installation.

Dans **Paramètres > Navigateurs**, chaque navigateur dispose de trois actions :
installer, choisir par défaut (qui l’installe au besoin), ouvrir. Le bouton
Navigateur du bureau utilise ce choix. Si le navigateur manque, il ouvre cette
page des paramètres plutôt que de rester silencieux.

Les archives sont extraites dans `/opt/winlux/browsers`. Winlux n’étant pas une
distribution Debian complète, cette opération n’exécute pas les scripts des
paquets et ne crée pas de base APT/dpkg. Les bibliothèques nécessaires sont
préparées dans l’image système. Les profils et téléchargements des navigateurs
appartiennent au compte dédié `winlux-web`, dans `/home/winlux-web`.
Les navigateurs démarrent sous ce compte, avec leur sandbox activée.

## Construire une image avec le catalogue

```sh
python3 scripts/prepare-browser-packages.py build/browser-packages
WINLUX_KERNEL=/chemin/vmlinuz WINLUX_KERNEL_RELEASE=version-du-noyau \
  sh scripts/build-live-iso.sh build/winlux-live.iso
```

Le préparateur télécharge les paquets depuis les dépôts HTTPS officiels,
vérifie leurs SHA-256 contre les index de ces dépôts et contrôle le nom du
paquet. Il ne réalise pas de vérification de signature GPG. Les archives
restent dans `build/` ; elles ne sont pas ajoutées au dépôt Git.
La construction des publications prépare ce catalogue automatiquement.
Une construction locale sans catalogue ne fournit aucun navigateur et la
page indique que les paquets sont indisponibles.

Les mises à jour système peuvent renouveler le catalogue. Edge est alors
actualisé au démarrage ; les autres navigateurs affichent leur mise à jour
disponible dans cette page. Fermer un navigateur avant de remplacer sa version.

## Vérifications

`sh scripts/test-browsers.sh` couvre les installations isolées, le choix par
défaut, les arguments URL, le refus des archives altérées et le nettoyage après
échec. `scripts/test-browsers-guest.sh` est un diagnostic réservé à une VM jetable :
il installe les trois vrais paquets et vérifie le rendu local en mode headless,
sans désactiver leur sandbox. Le paramètre noyau `winlux.browser-smoke=1`
déclenche ce diagnostic puis éteint la VM.
