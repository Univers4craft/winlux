# Mode récupération et vérification du média

## Winlux Recovery

Entrée du menu GRUB **« Winlux Recovery (secours) »** (touche `r`) sur la clé
USB live, et **« Winlux secours »** (touche `s`) sur un système installé. Elle
démarre la racine installée sans bureau et affiche :

1. **Réparer le démarrage** : réinstalle GRUB en BIOS et en UEFI sur le disque
   de la racine, puis la chaîne Secure Boot si elle est disponible
   ([SECURE-BOOT.md](SECURE-BOOT.md)).
2. **Restaurer le point de restauration précédent** : `winlux-update --rollback`
   (sauvegarde créée avant chaque mise à jour, voir [AB_UPDATES.md](AB_UPDATES.md)).
3. **Terminal de secours** : shell root dans le système installé.
4. **Vérifier le système de fichiers** : passe la racine en lecture seule puis
   lance `e2fsck -f -y`.
5. **Redémarrer** / 6. **Éteindre**.

Si l'entrée de la clé USB ne trouve aucun disque `WINLUX_ROOT`, elle l'indique
et revient au menu. Le menu est dans `scripts/winlux-recovery.sh`, installé en
`/usr/share/winlux/winlux-recovery.sh` ; l'initramfs garde un menu minimal de
repli si ce fichier manque.

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
