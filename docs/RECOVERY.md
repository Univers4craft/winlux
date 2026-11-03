# Récupération : réinitialiser ce PC, clé de secours, menu de secours

Tout est dans **Paramètres › Système › Récupération** (page 31), comme sous
Windows 11 ; les opérations passent par `winlux-reset` (`scripts/winlux-reset.py`),
en root via le relais `winlux-root` et le Contrôle de compte d'utilisateur.
Une option impossible sur la machine n'apparaît pas : pas de réinitialisation
ni de démarrage avancé dans le Live, pas de clé de secours sans `parted`,
`mkfs.fat` et `grub-install`.

## Réinitialiser ce PC

Deux choix, puis la source de la réinstallation et un résumé à confirmer :

| | Conserver mes fichiers | Tout supprimer |
|---|---|---|
| Comptes et mots de passe | gardés (UID, groupes, droits administrateur) | supprimés ; un nouveau compte est demandé |
| Dossiers personnels (`/home`, y compris une partition séparée) | gardés, jamais déplacés | effacés |
| Programmes Windows installés dans le dossier personnel | gardés | effacés |
| Applications de la Boutique (APT) | retirées, liste « Applications supprimées.txt » sur le bureau | retirées |
| Réglages | remis d'origine | remis d'origine |
| Wi-Fi enregistrés, sauvegardes de fichiers | gardés | effacés |
| Nom du PC, langue, clavier, fuseau, proxy, clés Secure Boot | gardés | gardés |
| Nettoyer le lecteur (zéros sur l'espace libre) | — | en option |

Sources : la **clé de secours** ou un support Winlux branché, une **image ISO**,
ou **Internet** (dernière version publiée, environ 2 Go, somme SHA-256
vérifiée, rien de lourd dans l'ISO).

Deux temps, et rien n'est effacé avant le redémarrage :

1. Pendant que Winlux tourne, `winlux-reset prepare` décompresse le système
   d'origine dans `/.winlux-reset/new` (même partition), y reporte ce qui est
   gardé et pose le noyau et l'initramfs de la nouvelle version dans `/boot`
   (les anciens restent en `*.before-reset` ; « Annuler la réinitialisation »
   les remet).
2. Au redémarrage, l'initramfs (`scripts/winlux-reset-apply.sh`) échange les
   dossiers du système par **renommages**, avant que le système ne démarre :
   `/home` (en « Conserver »), `/boot`, `lost+found` et le fichier d'échange ne
   bougent pas. Tant que `new/NOM` existe, `/NOM` est l'ancien : une coupure de
   courant au milieu reprend l'échange au démarrage suivant. L'ancien système
   n'est supprimé qu'ensuite.

Refusé (message clair) : une partition séparée pour `/usr`, `/var`… (réinstaller
depuis la clé), une image Winlux trop ancienne pour terminer l'échange, un
manque de place (environ la taille du système décompressé).

## Démarrage avancé

« Redémarrer maintenant » ajoute une fois pour toutes un petit bloc à
`grub.cfg` (`# winlux-next-recovery`) et pose `/boot/winlux-next-recovery` : le
démarrage suivant, et lui seul, ouvre le menu de secours. Le témoin est retiré
par le menu de secours, l'initramfs et `/init`.

## Clé de secours

« Créer une clé de secours » efface la clé USB choisie (4 Go au moins ; jamais
le disque du système ni le support Live), la formate en FAT32
(`WLXSECOURS`), y copie le noyau, l'initramfs et la racine SquashFS, installe
GRUB en BIOS et en UEFI (et la chaîne Secure Boot de l'ISO quand elle existe)
et écrit `winlux.sha256`. Elle démarre directement, en 10 s, sur :

1. **Menu de secours de ce PC** (par défaut ; `root=` et disque chiffré de ce PC) ;
2. **Menu de secours (autre PC Winlux)** (`LABEL=WINLUX_ROOT`) ;
3. **Démarrer Winlux depuis la clé (réinstaller)** : le Live et son installateur ;
4. **Vérifier la clé puis démarrer**.

Le noyau et l'initramfs viennent de la clé : le menu de secours fonctionne
même quand `/boot` du disque est abîmé.

## Menu de secours

Entrée du menu GRUB **« Winlux Recovery (secours) »** (touche `r`) sur la clé
USB live, **« Winlux secours »** (touche `s`) sur un système installé, ou la
clé de secours. Elle démarre la racine installée sans bureau et affiche, dans
la langue du système (français, anglais, allemand, espagnol) :

1. **Réparer le démarrage** : `winlux-reset repair-boot` réinstalle GRUB en
   BIOS (table MBR ou partition `bios_grub`) et en UEFI (`EFI/BOOT` sur une ESP
   à Winlux, `EFI/winlux` sur une ESP partagée avec Windows), puis la chaîne
   Secure Boot, et réécrit `grub.cfg` s'il est absent ou invalide (l'ancien
   reste en `grub.cfg.before-repair`). Aussi dans Paramètres › Récupération.
2. **Restaurer le point de restauration précédent** : `winlux-update --rollback`
   (sauvegarde créée avant chaque mise à jour, voir [AB_UPDATES.md](AB_UPDATES.md)).
3. **Terminal de secours** : shell root dans le système installé.
4. **Vérifier le système de fichiers** : passe la racine en lecture seule puis
   lance `e2fsck -f -y`.
7. **Réinitialiser ce PC** : mêmes choix que Paramètres ; la source est la clé
   de secours (ou d'installation) branchée, ou une ISO ; confirmation en tapant
   OUI (YES, JA, SÍ).
5. **Redémarrer** / 6. **Éteindre**.

Si l'entrée de la clé USB ne trouve aucun disque `WINLUX_ROOT`, elle l'indique
et revient au menu. Le menu est dans `scripts/winlux-recovery.sh`, installé en
`/usr/share/winlux/winlux-recovery.sh` ; l'initramfs garde un menu minimal de
repli si ce fichier manque.

## Tests

- `python3 -B scripts/test-reset.py` (CI) : système installé factice avec
  fichiers témoins et image d'origine en SquashFS ; conserver (fichiers gardés
  octet par octet, comptes, mots de passe, groupes, Wi-Fi), tout supprimer
  (plus aucun ancien fichier, nouveau compte), coupure de courant pendant
  l'échange puis reprise, annulation, refus, menus GRUB, traductions.
- `xvfb-run -a /usr/bin/python3 -B scripts/test-reset-page.py` (CI) : la page
  et les deux assistants dans les quatre langues ; `WINLUX_RESET_CAPTURES=DOSSIER`
  enregistre les captures.

## Vérifier l'intégrité du média

Entrée **« Vérifier l'intégrité du média puis démarrer »** (touche `v`).
Avant de démarrer le live, l'initramfs recalcule la somme SHA-256 de chaque
fichier du média (noyau, initramfs, racine SquashFS, GRUB, shim…) et la compare
à `winlux.sha256`, écrit à la racine de l'ISO lors de la construction.

- Tout correspond : le live démarre normalement.
- Une différence : un avertissement bloque le démarrage jusqu'à Entrée ; il ne
  faut pas installer depuis ce média.

Une image ne peut pas contenir sa propre somme : la somme de l'ISO entière
reste dans `winlux-live-test.iso.sha256`, à côté de l'ISO, pour la vérifier
avant de l'écrire sur une clé (`sha256sum -c winlux-live-test.iso.sha256`).
