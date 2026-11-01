# Publication et mises à jour

## Les versions publiées

| Version | Date | Points marquants |
|---|---|---|
| **v0.2.13** | en préparation | Changement rapide d'utilisateur corrigé, impression, son PipeWire, lecteurs réseau SMB/NFS par le noyau, outil de mise à jour réparé, manifeste publié avec chaque version et repli sur l'API des releases GitHub. |
| **[v0.2.12](https://github.com/Univers4craft/winlux/releases/tag/v0.2.12)** | 1er novembre 2026 | **Première version sur la base Ubuntu 26.04 avec APT natif.** Installateur graphique complet, Secure Boot, mode récupération, verrouillage, autorisations, quatre langues, lecteur multimédia FFmpeg, Direct3D 9/11/12, CLR maison, msiexec… ISO de 1,92 Gio. Remplace les tags v0.2.7 à v0.2.11, qui n'ont produit aucune release. |
| [v0.2.6](https://github.com/Univers4craft/winlux/releases/tag/v0.2.6) | 30 octobre 2026 | Dernière version sur l'ancienne base (sans APT) : applications natives, raccourcis globaux. |
| v0.2.3 à v0.2.5 | octobre 2026 | Premières releases système complètes, bureau et Paramètres. |
| v0.2.0-preview.2 à .15 | octobre 2026 | Préversions du runtime et du bureau. |

Les notes de chaque version sont dans [`docs/releases/`](https://github.com/Univers4craft/winlux/tree/master/docs/releases).

## Ce qu'il faut savoir avant de mettre à jour

> **Une seule réinstallation, selon votre version.**
>
> - **v0.2.6 ou plus ancienne** : la v0.2.12 change toute la base du système (passage à Ubuntu 26.04 avec APT). Une installation ancienne ne peut pas être convertie en place ; son outil de mise à jour le dit et propose de réinstaller avec la nouvelle ISO.
> - **v0.2.12** : son outil de mise à jour transmet à APT un fichier temporaire que celui-ci refuse, si bien qu'il ne peut installer aucune mise à jour. Le défaut est corrigé dans la v0.2.13, mais un outil installé ne peut pas se corriger lui-même : il faudra **réinstaller une fois avec l'ISO v0.2.13**.
> - **À partir de la v0.2.13**, les mises à jour se font depuis Paramètres, sans réinstaller.
>
> Les notes publiées de la v0.2.12 promettent à tort une mise à jour sans réinstallation ; elles seront corrigées à la publication de la v0.2.13.

## Mettre à jour un système installé

**Depuis le bureau** : Paramètres › Mises à jour › « Rechercher des mises à jour », puis « Installer ». La page affiche la progression et le journal. Les mises à jour automatiques s'activent sur la même page ; elles ne forcent jamais de redémarrage.

**En ligne de commande** :

```sh
winlux-update --check      # une nouvelle version est-elle publiée ?
winlux-update --list       # versions disponibles
winlux-update --apply      # télécharger, vérifier et installer
winlux-update --rollback   # revenir à la version précédente
winlux-update --auto enable   # (ou disable)
```

Les mises à jour des paquets Ubuntu eux-mêmes passent par Paramètres › Mises à jour système (ou `apt`).

### Comment ça marche

1. L'outil interroge la **dernière release publique** du dépôt GitHub (jamais les commits de `master`, jamais les brouillons ni les préversions), par son URL publique, **sans jeton ni compte**.
2. Il lit le **manifeste** (format 3, `backend=apt`, `base=ubuntu-resolute`) : nom, version, taille et SHA-256 du paquet `winlux-desktop-<version>.deb`. À partir de la v0.2.13, si le manifeste manque, l'outil se rabat sur l'API des releases GitHub (nom du fichier, taille, somme de contrôle).
3. Il vérifie l'espace disque, télécharge, contrôle la taille et le SHA-256.
4. Il crée un **point de restauration** (`winlux-restore`, les trois derniers sont gardés), puis installe le paquet avec APT (`--no-remove`, configurations locales conservées).
5. Il vérifie les fichiers installés (`dpkg --verify`), répare ou revient en arrière en cas d'écart.
6. **Validation au démarrage** : après la mise à jour, si le bureau ne tient pas 60 secondes ou si Xorg ne démarre pas, Winlux revient automatiquement à la version précédente (au plus tard au troisième démarrage sans validation), et cette version n'est plus réinstallée automatiquement.

Vos documents, vos applications installées et la base `dpkg` sont conservés. Les arrêts normaux sont différés pendant l'installation.

### Ce qui manque encore

- Les mises à jour sont vérifiées par SHA-256 mais **pas encore signées** (signature Ed25519 prévue).
- Il n'y a pas encore de **partitions système A/B** : une coupure de courant au mauvais moment reste un risque, couvert seulement par APT (`dpkg --configure -a`) et le mode récupération.

Conception cible : [docs/AB_UPDATES.md](https://github.com/Univers4craft/winlux/blob/master/docs/AB_UPDATES.md).

## Publier une nouvelle version

Le processus est entièrement automatisé par [`.github/workflows/release.yml`](https://github.com/Univers4craft/winlux/blob/master/.github/workflows/release.yml), déclenché par un tag.

1. **Préparer** sur une branche puis fusionner dans `master` :
   - mettre le numéro dans [`VERSION`](https://github.com/Univers4craft/winlux/blob/master/VERSION) (par exemple `v0.2.13`) ;
   - écrire les notes dans `docs/releases/v0.2.13.md` (24 Kio au plus : elles deviennent aussi les notes affichées dans Paramètres).
2. **Pousser le tag** `v0.2.13` sur le commit de `master`. Le tag doit correspondre à `VERSION`. Les tags `v*-preview.*` sont exclus.
3. **Le workflow** (runner Ubuntu 26.04, KVM activé) :
   1. vérifie la version ;
   2. libère de l'espace disque sur le runner ;
   3. prépare les navigateurs et le bureau natif hors ligne ;
   4. compile et teste le runtime ;
   5. prépare la base Ubuntu dans une VM jetable et **construit l'ISO** (squashfs xz, taille contrôlée sous la limite de 2 Gio des fichiers de release GitHub) ;
   6. **valide en VM** : navigateurs, bureau natif (fenêtres, édition, menu, captures), ISO en BIOS et UEFI, installation sur disque, **mise à jour depuis la version précédente** ;
   7. construit le paquet de mise à jour (`build-update-bundle.sh --system`) et le manifeste ;
   8. calcule les sommes SHA-256 et **publie la release** : ISO, `.deb`, manifeste, sommes.
4. Les journaux de validation restent disponibles comme artefacts du workflow.

Une version ratée **ne réutilise jamais son tag** : on passe au numéro suivant (c'est pourquoi v0.2.7 à v0.2.11 n'ont pas de release).

### Pièges déjà rencontrés

| Échec | Cause | Correction |
|---|---|---|
| v0.2.7 | Ubuntu 26.04 a renommé `libqt6gui6t64` en `libqt6gui6` et retiré `libgl1-mesa-glx` | Noms de paquets corrigés (#128) ; vérifier les paquets de Resolute avant de taguer |
| v0.2.8 | Le BusyBox du runner utilise AVX, absent du CPU QEMU par défaut | `-cpu max` et KVM dans les tests de VM (#132) |
| v0.2.9 | Bureau bloqué au démarrage sur un PC sans Bluetooth | #142 |
| v0.2.10 | ISO de plus de 2 Gio, refusée par GitHub | Squashfs en xz avec contrôle de taille (#192) |
| v0.2.11 | Disque du runner plein pendant la construction | Nettoyage du runner avant la construction |

Le workflow [`apt-base.yml`](https://github.com/Univers4craft/winlux/blob/master/.github/workflows/apt-base.yml) reconstruit la base Ubuntu à chaque pull request qui touche sa recette, pour qu'une base cassée soit vue avant de taguer.

## Identifier la version installée

`/usr/share/winlux/BUILD-INFO` (affiché dans Paramètres › À propos) contient la version, le commit, la date UTC de construction, l'état des sources et le périmètre du paquet.

Voir aussi : [Construire l'ISO](Construire-l-ISO) · [Tests et CI](Tests-et-CI)
