# Archives et images disque

## Archives (comme des dossiers)

Double-clic sur une archive dans l'Explorer : elle s'ouvre comme un dossier.
On peut y copier, coller, renommer, supprimer ou créer des fichiers ; l'archive
est reconstruite automatiquement après chaque modification.

Clic droit :

- sur une archive : **Extraire tout…** (dossier du même nom, à côté de
  l'archive, puis ouverture), **Extraire ici** ;
- sur une sélection : **Compresser en fichier ZIP / 7z / TAR.GZ**,
  **Compresser en ZIP avec mot de passe…**.

Une archive chiffrée ouvre une boîte de saisie du mot de passe.

| Format | Lecture | Écriture | Moteur |
| --- | --- | --- | --- |
| zip | oui | oui | Winlux (DEFLATE, ZIP64, ZipCrypto) |
| tar, tar.gz / tgz, gz | oui | oui | Winlux |
| tar.xz, tar.bz2, tar.zst, xz, bz2, zst | si `xz` / `bzip2` / `zstd` | idem | Winlux + outil de compression |
| zip AES, LZMA | si 7-Zip ou bsdtar | ZipCrypto | outil |
| 7z | si 7-Zip ou bsdtar | si 7-Zip | outil |
| rar | si unrar, 7-Zip, unar ou bsdtar | non (lecture seule) | outil |
| cab, deb, rpm, lzh, arj | si 7-Zip ou bsdtar | non | outil |

Un format dont l'outil manque n'est pas proposé : le double-clic revient alors
au comportement normal (application associée).

Fonctionnement : `WinluxArchiveOpen` déploie l'archive dans
`~/.cache/winlux/archives/<empreinte>/root`, puis `paths::to_linux` résout tout
chemin `…\photos.zip\…` vers ce cache. `WinluxArchiveCommit` compare le
contenu au manifeste de déploiement et reconstruit l'archive (fichier
temporaire puis renommage atomique) si quelque chose a changé.

## Images disque (comme un lecteur)

Double-clic sur `.iso`, `.img`, `.vhd`, `.vhdx`, `.qcow2`, `.vmdk`, `.vdi` ou
`.dmg` : chaque partition reconnue est montée et reçoit une lettre (D:, E:…)
dans « Ce PC », avec **Éjecter** au clic droit.

| Format | Mécanisme | Écriture |
| --- | --- | --- |
| iso | périphérique boucle (`losetup`) | non |
| img, raw | périphérique boucle | si le fichier est inscriptible |
| vhd, vhdx, qcow2, vmdk, vdi | `qemu-nbd` + module `nbd` | si le fichier est inscriptible |
| dmg | `qemu-nbd` (pilote dmg) ou `dmg2img` | non |

Sans `root`, seules les images brutes passent par `udisksctl` s'il est
installé. Si un système de fichiers refuse l'écriture, il est remonté en
lecture seule.

Les lecteurs dynamiques (images et lecteurs réseau) partagent le registre
`/run/winlux/mounts.tsv` (`runtime/src/mounts.rs`).

L'ISO live embarque ces outils et modules s'ils sont présents sur la machine
de construction (`scripts/stage-storage-tools.sh`).
