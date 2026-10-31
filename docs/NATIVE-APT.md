# APT natif dans Winlux

Cette intégration utilise Ubuntu Resolute amd64 comme racine du système, avec
les vrais outils `apt-get` et `dpkg`, leurs bases de paquets et leurs scripts
de configuration. Ce n’est pas un environnement Ubuntu séparé.

Les sources de cette intégration ne constituent pas une nouvelle release.
La construction 0.2.6 déjà lancée reste basée sur le système précédent.

## Construire une image native

Les outils habituels de construction de l’ISO sont requis, ainsi que
`python3`, `curl`, `dpkg-deb`, `fakeroot`, `mkfs.ext4`, `debugfs` et QEMU.
L’hôte de compilation doit être Ubuntu Resolute amd64 ou un dérivé compatible,
afin de conserver la même ABI que la base. Le workflow utilise Ubuntu 26.04.

```sh
python3 scripts/prepare-apt-base.py \
  --kernel /chemin/vers/vmlinuz --kernel-release VERSION_DU_NOYAU
WINLUX_KERNEL=/chemin/vers/vmlinuz WINLUX_KERNEL_RELEASE=VERSION_DU_NOYAU \
  sh scripts/build-live-iso.sh build/winlux-native-apt.iso
```

La préparation télécharge l’image Ubuntu Base officielle 26.04.1, vérifie
son SHA-256 fixé dans le script et installe les composants du bureau dans
une VM jetable. APT vérifie les signatures des dépôts Ubuntu. Aucun script
de maintenance Ubuntu ne s’exécute sur l’hôte. Les journaux sont conservés
dans `build/apt-base/bootstrap.log`. Un dossier de préparation préexistant
est conservé en cas d’échec ; le déplacer avant une nouvelle tentative.

Le système Linux configuré garde ses fichiers et son inventaire `dpkg`.
Les ajouts propres à Winlux deviennent le paquet `winlux-desktop`, marqué
Essential et Protected et dépendant des composants essentiels du bureau.
Les bibliothèques et commandes Ubuntu restent gérées par leurs paquets
d’origine : les anciens raccourcis BusyBox et copies de bibliothèques du
constructeur ne sont pas ajoutés au paquet du bureau. Le noyau et ses
pilotes sont enregistrés dans un paquet protégé `winlux-kernel`, conservé
lors des mises à jour de l’interface. L’activation d’un autre noyau via APT
n’est pas automatisée par le chargeur Winlux.
Le worker XScreenSaver personnalisé utilise une diversion dpkg, afin que
les mises à jour du paquet Ubuntu restent cohérentes.

La création des images se fait sans privilèges sur l’hôte : les propriétaires
des comptes système et les permissions spéciales de la base sont rétablis
une seule fois au premier démarrage, avant le lancement des services.

`WINLUX_LEGACY_BASE=1` permet uniquement de reconstruire l’ancien système
pour les diagnostics. Ce mode ne fournit pas APT.

## Installer des applications

Dans le terminal du système natif :

```sh
apt-get update
apt-get install nom-du-paquet
apt-get remove nom-du-paquet
```

La session actuelle utilise root. Le panneau Applications propose aussi
l’installation et la désinstallation par nom de paquet. Les fichiers
`.desktop` installés dans les répertoires XDG sont reconnus par Démarrer ;
un hook APT actualise le catalogue Winlux après les opérations dpkg.
Les associations de fichiers restent celles choisies par l’utilisateur.

Une suppression ordinaire avec `apt-get -y` ne peut pas retirer le paquet
protégé du bureau. Un administrateur qui utilise explicitement les options
de suppression des paquets essentiels peut néanmoins casser son système.

## Mises à jour Winlux

Le constructeur produit un paquet Debian et un manifeste de format 3,
identifié `backend=apt` et `base=ubuntu-resolute`. Le moteur natif vérifie la
taille, le SHA-256, le nom et la version du paquet, puis passe par APT avec
`--no-remove` et conservation des configurations locales. Il ne remplace
pas les bibliothèques Linux par un arbre de fichiers hors de dpkg.

Les applications supplémentaires, la base dpkg et les documents sont
conservés. Le paquet Winlux précédent est conservé pour le retour à cette
version ; les dépendances Ubuntu ne sont pas restaurées par cette action.
APT garde ses propres mécanismes de réparation (`dpkg --configure -a`,
`apt-get -f install`) en cas d’installation interrompue. Le bureau bloque
son lancement si dpkg signale une configuration inachevée au démarrage.

Les verrous APT sont pris en compte pour différer l’arrêt et éviter une
mise à jour Winlux pendant une installation manuelle.

Une ancienne installation Winlux sans base dpkg ne peut pas recevoir ce
nouveau système par simple remplacement de fichiers : une nouvelle
installation de l’image native est nécessaire. Le moteur natif refuse les
anciens manifestes au lieu d’appliquer un paquet incompatible. La migration
automatique des anciennes installations n’est pas implémentée.

## Limites

Winlux conserve son init et sa session. Les services installés par les
paquets ne sont pas démarrés automatiquement par systemd ; les applications
qui exigent une session systemd/logind ou un utilisateur non privilégié
peuvent nécessiter une adaptation. La disponibilité dans les dépôts ne
garantit donc pas le fonctionnement de tous les logiciels Linux.

La validation native installe un paquet de diagnostic avec un lanceur XDG,
réinstalle Winlux en conservant ce paquet et un document, vérifie la
protection du bureau puis désinstalle le paquet de diagnostic.
