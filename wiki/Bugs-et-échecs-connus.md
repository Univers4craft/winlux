# Bugs et échecs connus

> Problèmes identifiés, limitations et travaux échoués à résoudre.

## Bugs actifs

### Bureau lancé en root
- **Issue** : [#13](https://github.com/Univers4craft/winlux/issues/13)
- **Description** : Le script Xsession existe, mais l'init lance encore le bureau en root dans l'ISO
- **Impact** : Sécurité — le bureau devrait tourner en utilisateur non-privilégié
- **Contournement** : Aucun actuellement

### test-live-iso-qemu.sh cassé
- **Description** : Le script de test ISO est cassé depuis que GRUB utilise par défaut l'entrée bureau au lieu de l'entrée diagnostic
- **Impact** : Les tests automatisés de l'ISO ne sont pas fiables
- **Source** : Mémoire projet (octobre 2026)

### Mises à jour non signées
- **Issue** : [#31](https://github.com/Univers4craft/winlux/issues/31)
- **Description** : Les mises à jour vérifient SHA-256 mais ne sont pas signées cryptographiquement (Ed25519)
- **Impact** : Sécurité — un attaquant qui compromet le transport peut substituer le paquet
- **Statut** : Design documenté dans `docs/AB_UPDATES.md`, implémentation à faire

## Limitations connues

### Runtime
- **64 bits uniquement** : pas de prise en charge PE32 (32 bits)
- **Pas de noyau Windows** : les appels système NT ne sont pas émulés au niveau noyau
- **DLL restent mappées** : `FreeLibrary` ne décharge pas réellement les DLL (#24)
- **Exceptions C++ non supportées** : `RtlUnwindEx`, `RtlVirtualUnwind` manquants (#25)
- **WPF/WinForms incompatibles** : le pont dotnet/Mono ne suffit pas (#22)
- **DirectX non supporté** : pas d'accélération GPU

### Bureau
- **Corbeille non fonctionnelle** : la suppression existe mais pas la restauration (#10)
- **Pas de progression de copie** : les copies de fichiers n'ont pas de barre de progression (#11)
- **Accessibilité incomplète** : contraste, taille du pointeur, navigation clavier (#28)

### ISO / Installation
- **Installateur expérimental** : ne pas utiliser sur un SSD avec des données
- **Pas de dual boot** : l'installation efface le disque entier
- **Workflow release non validé** : les tests sur GitHub Actions n'ont pas été exécutés bout en bout (#32)
- **Pas de système A/B** : une coupure pendant la mise à jour peut laisser le système inutilisable

### Matériel
- **VM uniquement** : pas de validation sur matériel réel
- **Réseau limité** : Ethernet virtuel seulement, pas de Wi-Fi/Bluetooth réel
- **Audio non validé sur machine physique** : ALSA testé en sortie null uniquement

## Threads de projet échoués

### Thread "Vérifier GitHub, MD et continuer le travail"
- **État** : `failed`
- **Date** : Octobre 2026
- **Description** : Un thread de travail qui devait vérifier l'état GitHub, les fichiers Markdown et continuer le développement a échoué
- **Impact** : Travail planifié non complété, à reprendre manuellement

## Logiciels Windows non compatibles (connus)

| Logiciel | Raison | Issue liée |
|----------|--------|------------|
| paint.net | Nécessite CLR/WPF complet | #22 |
| Photoshop | DirectX, COM avancé, DLL complexes | #26 |
| Applications WinForms | Pas de runtime WinForms | #22 |
| Programmes C++ MSVC | Exceptions non supportées | #25 |
| Installateurs MSIX | Pas d'activation MSIX | - |
| Services Windows | StartServiceCtrlDispatcher manquant | #17 |

## Comment signaler un bug

1. Vérifier que le bug n'est pas déjà listé ci-dessus ou dans les [issues ouvertes](https://github.com/Univers4craft/winlux/issues)
2. Créer une issue avec :
   - Version/commit (`cat /usr/share/winlux/BUILD-INFO`)
   - Logiciel concerné et sa version
   - Étapes de reproduction
   - Journal des imports manquants (`WINLUX_TRACE=1 winlux programme.exe 2>&1`)
