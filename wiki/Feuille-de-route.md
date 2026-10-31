# Feuille de route

> Organisée par priorité. Chaque tâche est liée à une issue GitHub.

## Priorité IMMÉDIATE

| Tâche | Issue | Description |
|-------|-------|-------------|
| Corbeille fonctionnelle | [#10](https://github.com/Univers4craft/winlux/issues/10) | Supprimer → corbeille, restaurer, vider |
| Copie avec progression | [#11](https://github.com/Univers4craft/winlux/issues/11) | Barre de progression + vitesse, ETA, pause/annuler |
| Réglages rapides | [#12](https://github.com/Univers4craft/winlux/issues/12) | Réseau, volume, éclairage, batterie |

## Sécurité

| Tâche | Issue | Description |
|-------|-------|-------------|
| Bureau non-root | [#13](https://github.com/Univers4craft/winlux/issues/13) | L'init lance encore le bureau en root |
| Signature + A/B | [#31](https://github.com/Univers4craft/winlux/issues/31) | Ed25519, partitionnement A/B, rollback auto |

## P1 — Fiabiliser le runtime

| Tâche | Issue | Description |
|-------|-------|-------------|
| kernel32/msvcrt/ntdll | [#14](https://github.com/Univers4craft/winlux/issues/14) | Compléter les implémentations partielles |
| Stack overflow | [#15](https://github.com/Univers4craft/winlux/issues/15) | Pile de secours sûre |

## P2 — Compatibilité Windows

| Tâche | Issue | Description |
|-------|-------|-------------|
| COM/OLE complet | [#16](https://github.com/Univers4craft/winlux/issues/16) | Registre, serveurs hors processus, automatisation |
| Services Windows | [#17](https://github.com/Univers4craft/winlux/issues/17) | StartServiceCtrlDispatcher |
| Annulation I/O | [#18](https://github.com/Univers4craft/winlux/issues/18) | Tests exhaustifs buffers/handles invalides |
| Cycle processus | [#19](https://github.com/Univers4craft/winlux/issues/19) | Héritage handles, attributs étendus |
| IPC nommé | [#20](https://github.com/Univers4craft/winlux/issues/20) | Partage objets et fichiers inter-processus |
| ntdll | [#21](https://github.com/Univers4craft/winlux/issues/21) | Exceptions/unwinding, TLS statique |
| CLR/WPF/WinForms | [#22](https://github.com/Univers4craft/winlux/issues/22) | Vrai runtime CLR Windows |
| Codecs vidéo | [#23](https://github.com/Univers4craft/winlux/issues/23) | Media Foundation/DirectShow |
| FreeLibrary | [#24](https://github.com/Univers4craft/winlux/issues/24) | Vrai déchargement de DLL |
| C++ / MSVC | [#25](https://github.com/Univers4craft/winlux/issues/25) | Exceptions RtlUnwind*, exécutables MSVC |
| DLL ciblées | [#26](https://github.com/Univers4craft/winlux/issues/26) | DLL système pour logiciels ciblés |

## P3 — Intégration Linux

| Tâche | Issue | Description |
|-------|-------|-------------|
| Tests réels | [#27](https://github.com/Univers4craft/winlux/issues/27) | Installation hôte, binfmt réel, GNOME/KDE/XFCE |

## P5 — Thème Windows 11

| Tâche | Issue | Description |
|-------|-------|-------------|
| Accessibilité | [#28](https://github.com/Univers4craft/winlux/issues/28) | Contraste, pointeur, animations, clavier |
| Multi-résolution | [#29](https://github.com/Univers4craft/winlux/issues/29) | Validation 1080p, 1440p, 4K |
| Bureau vide | [#30](https://github.com/Univers4craft/winlux/issues/30) | Icônes facultatives, raccourcis explicites |

## P6 — Qualité et livraison

| Tâche | Issue | Description |
|-------|-------|-------------|
| Signature Ed25519 + A/B | [#31](https://github.com/Univers4craft/winlux/issues/31) | Mises à jour signées, slots A/B |
| Workflow release | [#32](https://github.com/Univers4craft/winlux/issues/32) | Validation complète sur GitHub Actions |
| Notices/licences | [#33](https://github.com/Univers4craft/winlux/issues/33) | Obligations de distribution |
| Version stable v1.0 | [#34](https://github.com/Univers4craft/winlux/issues/34) | Jalon final |

## Objectifs long terme (depuis ROADMAP-SSD.md et INTERFACE.md)

Ces tâches ne sont pas encore planifiées avec des issues individuelles mais sont documentées :

- Assistant graphique d'installation en français
- Installation à côté de Windows (dual boot)
- Gestion multi-écrans, fréquence et échelle par écran
- Gestion Ethernet, Wi-Fi et Bluetooth sur matériel réel
- Veille, reprise et hibernation
- Chiffrement du système et des données
- Gestionnaire de tâches complet
- Recherche globale (applications, fichiers, paramètres)
- Glisser-déposer inter-applications
- Presse-papiers cohérent Linux/Windows
- Mode de récupération au démarrage
- Profils d'alimentation et suivi thermique
- Imprimantes et scanners
- DirectX et accélération GPU
- Assistant de compatibilité .exe
- Assistant de migration depuis Windows
