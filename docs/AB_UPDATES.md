# Architecture des mises à jour signées et système A/B

> **État : en service (#31).** Manifestes signés en Ed25519 et deux
> emplacements système sur la partition racine, avec une seule version
> précédente gardée en réserve, bascule et retour automatique au démarrage.
> Le retour par `previous.deb` reste en place pour les mises à jour installées
> par APT sur place (voir « Repli »).

## Mise en œuvre de l'A/B

| Élément | Rôle |
|---------|------|
| `scripts/winlux-ab.py` (`/usr/bin/winlux-ab`) | `prepare` installe la mise à jour dans `/.winlux-ab/b` ; `confirm`, `rollback`, `discard`, `status` |
| `runtime/src/abslots.rs` (`winlux --ab-boot`) | appelé par `/init` avant tout autre programme : bascule, compte les essais, retour |
| `scripts/winlux-update-apt.py` | prépare en A/B, sinon APT sur place ; validation du bureau, échec du bureau et retour manuel passent par `winlux-ab` |

**Emplacements.** Le système actif est la racine ; l'emplacement B
(`/.winlux-ab/b`, dossier 0700 sur la même partition) contient soit la mise à
jour préparée, soit la version précédente. Sont échangés `/usr`,
`/var/lib/dpkg`, `/opt/winlux` et `/init`, plus les fichiers du paquet sous
`/etc`. `/home`, les comptes, le registre, `/var/lib/winlux` et le noyau
restent communs. Aucun repartitionnement : un PC déjà installé passe en A/B
dès que la version qui contient `winlux-ab` est installée (cette mise à
jour-là se fait encore sur place, par l'ancien client).

**Préparation** (système en marche, jamais modifié) : `/usr` et `/opt/winlux`
sont recopiés en liens physiques, `/var/lib/dpkg` et `/init` en vraie copie,
puis `dpkg --root=/.winlux-ab/b --force-script-chrootless` installe le paquet
(ses scripts respectent `DPKG_ROOT`). dpkg remplace chaque fichier par
renommage, donc les fichiers partagés avec l'emplacement actif ne bougent
pas. Coût disque : la taille décompressée du paquet plus `/var/lib/dpkg`,
environ 200 Mo pour la 0.2.20 ; le système complet (6,6 Go mesurés sur un
portable) n'est jamais recopié. Caches du bureau régénérés, puis
`dpkg --verify`.

**Bascule** (`/init`, au démarrage suivant) : `renameat2(RENAME_EXCHANGE)`
échange chaque paire de façon atomique. Chaque échange se reconnaît au numéro
d'inode noté à la préparation, donc une coupure de courant au milieu reprend
au démarrage suivant. `/init` se relance ensuite (code 10). Si des paquets ont
été installés entre la préparation et le redémarrage, la préparation est
abandonnée (`state=stale`) et la mise à jour sera refaite.

**Essai et retour.** La nouvelle version reste à l'essai jusqu'à ce que le
bureau tienne une minute (`--confirm-boot`, puis `winlux-ab confirm`). Au 3e
démarrage interactif sans validation, ou tout de suite si le bureau échoue,
`/init` échange de nouveau : la réserve revient et la version quittée est
bloquée pour la mise à jour automatique. « Revenir à la version précédente »
dans Paramètres demande le même échange au prochain démarrage.

**Repli.** Un paquet qui demande un paquet Ubuntu absent ne peut pas
s'installer dans l'emplacement B, faute de réseau pour dpkg. Le client
l'installe alors avec APT sur place, avec point de restauration et
`previous.deb` comme avant.

## Conception d'origine

## Objectif

Garantir qu'une coupure de courant à n'importe quel moment d'une mise à jour
laisse le système dans un état démarrable et fonctionnel, sans intervention
manuelle.

## Architecture A/B

### Principe

Le système maintient deux jeux de fichiers racine (« slots ») :

```
/winlux/slot-a/    ← slot actif (exemple)
/winlux/slot-b/    ← slot inactif, cible de la mise à jour
/winlux/shared/    ← données utilisateur, configuration, registre
```

Un indicateur atomique dans le chargeur de démarrage (GRUB) désigne le slot
actif. La bascule ne se produit qu'après vérification complète du nouveau slot.

### Cycle de mise à jour

1. **Téléchargement** : le paquet est récupéré et vérifié (SHA-256 + signature).
2. **Écriture** : le contenu est écrit dans le slot inactif. Le slot actif
   n'est jamais modifié.
3. **Vérification** : le slot mis à jour est vérifié par empreinte.
4. **Bascule** : l'indicateur GRUB est mis à jour atomiquement pour pointer
   vers le nouveau slot.
5. **Redémarrage** : le système démarre sur le nouveau slot.
6. **Validation** : si le démarrage réussit, le slot est marqué « bon ».
   Sinon, le chargeur de démarrage rebascule automatiquement sur l'ancien slot.

### Résistance aux pannes

| Moment de la coupure | Résultat |
|----------------------|----------|
| Pendant le téléchargement | Reprise au prochain cycle |
| Pendant l'écriture du slot inactif | Le slot actif est intact ; l'écriture reprend |
| Après la bascule GRUB | Le nouveau slot démarre ; si échec, retour automatique |
| Pendant la validation | Retour à l'ancien slot au prochain démarrage |

## Mises à jour signées

### Chaîne de confiance

```
Clé racine Winlux (Ed25519, hors ligne)
  └─ Clé de release (Ed25519, CI)
       └─ Signature du manifeste de mise à jour
            └─ SHA-256 de chaque fichier du paquet
```

### Format du manifeste signé

```
WINLUX-UPDATE-MANIFEST v2
slot: b
version: 0.2.0
timestamp: 2026-11-01T12:00:00Z
files:
  usr/bin/winlux           sha256:<hash>
  usr/share/winlux/...     sha256:<hash>
signature: <signature Ed25519 du bloc ci-dessus>
```

### Vérification

L'outil `winlux-update` :

1. Vérifie la signature Ed25519 du manifeste avec la clé publique embarquée.
2. Vérifie le SHA-256 de chaque fichier téléchargé.
3. Refuse toute mise à jour dont la signature ou une empreinte est invalide.

## État actuel et prochaines étapes

### Ce qui existe

- [x] Paquet de mise à jour avec manifeste SHA-256 (`build-update-bundle.sh`)
- [x] Vérification des empreintes à l'application (`winlux-update --apply`)
- [x] Retour arrière manuel (`winlux-update --rollback`)
- [x] Mise à jour automatique (boucle de vérification, pas de redémarrage forcé)
- [x] Verrouillage des opérations concurrentes
- [x] Point de restauration `winlux-restore` avant chaque installation APT (3 derniers gardés)
- [x] Vérification `dpkg --verify` des fichiers installés, réparation puis retour arrière
- [x] Validation du démarrage (base APT) : `/var/lib/winlux/update-trial` est écrit après
      l'installation ; l'init appelle `winlux-update --confirm-boot` quand le bureau tient
      60 s, `--boot-failed` si Xorg ou la session ne démarrent pas (retour immédiat), et
      `--boot` revient à `previous.deb` hors ligne (`dpkg --install`) au 3e démarrage
      interactif sans validation. La version quittée est notée dans `update-blocked` :
      `--auto-run` ne la réinstalle pas, `--apply` manuel reste possible.

- [x] Signature Ed25519 du manifeste (#31) : `scripts/sign-update-manifest.sh` signe
      `winlux-update-manifest.txt` en CI avec le secret `WINLUX_UPDATE_SIGNING_KEY`
      (graine de 32 octets en hexadécimal) et publie `winlux-update-manifest.txt.sig`.
      `winlux-update-apt` vérifie la signature avec `/usr/share/winlux/update-signing.pub`
      (copie de `integration/native/keys/update-signing.pub`) grâce à
      `winlux-signature`, une implémentation d'Ed25519 écrite d'après la RFC 8032 ;
      le manifeste couvre le SHA-256 du paquet. Dès que la clé publique est dans
      l'image, un manifeste absent, non signé ou falsifié est refusé, et le repli
      sur l'API GitHub aussi. Tests : `scripts/test-update-signature.py`.

Audit du 25 octobre 2026 : `scripts/test-updater.sh` couvre les transactions et
retours arrière ; `scripts/test-ab-update.py` couvre l'A/B.

### Mise en service de la clé

1. Créer la graine sur un PC de confiance : `winlux-signature new graine.hex`
   (affiche la clé publique ; le fichier est en 0600).
2. Mettre le contenu de `graine.hex` dans le secret GitHub `WINLUX_UPDATE_SIGNING_KEY`.
3. Committer la clé publique dans `integration/native/keys/update-signing.pub`.
   À partir de là, `release.yml` échoue si le secret manque ou ne correspond pas.

### A/B (P1)

- [x] Deux emplacements sur la partition racine (dossiers, sans repartitionnement)
- [x] Bascule atomique au démarrage (`renameat2` RENAME_EXCHANGE, reprise après coupure)
- [x] Validation automatique après un démarrage réussi (bureau tenu une minute)
- [x] Retour automatique si le démarrage échoue (3e démarrage ou bureau en échec)

### À implémenter (P2)

- [ ] Rotation des clés de release
- [ ] Mises à jour delta (seuls les fichiers modifiés sont téléchargés)
- [ ] Canal de mise à jour bêta/stable
- [ ] Journal d'audit des mises à jour appliquées
