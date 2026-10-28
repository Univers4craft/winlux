# Architecture des mises à jour signées et système A/B

> **État : conception préliminaire.** Ce document décrit l'architecture cible
> pour les mises à jour résistantes aux coupures d'alimentation. L'implémentation
> actuelle utilise une mise à jour transactionnelle avec retour arrière, mais ne
> possède pas encore de véritable schéma A/B ni de signature cryptographique.

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

Audit du 25 octobre 2026 : `scripts/test-updater.sh` couvre les transactions et
retours arrière. Aucune case A/B ou signature n'est cochée : le schéma ci-dessus
est une conception cible, pas le comportement du client actuel.

### À implémenter (P1)

- [ ] Signature Ed25519 du manifeste (génération en CI, vérification côté client)
- [ ] Partitionnement A/B réel avec deux slots sur le disque
- [ ] Bascule atomique dans la configuration GRUB
- [ ] Validation automatique du slot après démarrage réussi
- [ ] Retour arrière automatique par le chargeur si le démarrage échoue

### À implémenter (P2)

- [ ] Rotation des clés de release
- [ ] Mises à jour delta (seuls les fichiers modifiés sont téléchargés)
- [ ] Canal de mise à jour bêta/stable
- [ ] Journal d'audit des mises à jour appliquées
