# État du projet

> Mis à jour le 30 octobre 2026

## Progression globale

| Métrique | Valeur |
|----------|--------|
| Tâches réalisées | **124** |
| Tâches restantes (prioritaires) | **26** |
| Tâches restantes (toutes sources) | **~234** |
| Progression prioritaire | **82.7%** |
| Version | v0.2.11 |
| Dernière release | v0.2.0-preview.15 |
| Tests unitaires | 84+ (1 ignoré sans serveur X) |
| Tests PE | 10 exécutables validés |
| Tests MinGW-w64 | 40 contrôles |

## Jalons atteints

### Jalon 1 : Runtime PE fonctionnel
- Chargement PE32+ x86-64 avec relocations et imports
- 10 PE de test exécutés en séquentiel et parallèle
- Structures PEB/TEB, TLS, imports différés
- Politique d'imports manquants

### Jalon 2 : API Win32 de base
- kernel32 : fichiers, mémoire, processus, threads, synchronisation
- user32/gdi32 : fenêtres, messages, dessin complet
- ws2_32 : réseau TCP/UDP
- msvcrt : CRT complète
- COM/OLE fondation

### Jalon 3 : Bureau Windows 11
- Bureau X11 style Windows 11
- Menu Démarrer, barre des tâches, notifications
- Explorer avec onglets et double volet
- Paramètres complets
- Applications natives (calculatrice, capture, visionneuse, PDF, audio, post-it)

### Jalon 4 : ISO bootable
- ISO BIOS + UEFI via GRUB
- Tests automatisés QEMU
- Installateur disque expérimental
- Mises à jour GitHub avec rollback

### Jalon 5 : Intégration Linux
- binfmt_misc, MIME, .desktop
- Installation/désinstallation propre
- Lanceur intelligent console/graphique

### Jalon 6 : Applications natives (octobre 2026)
- Hotkeys globales
- Lecteur PDF et audio natifs
- Visionneuse, Post-it, Infos système
- Calculatrice et Capture d'écran

## Ce qui reste pour la v1.0

### Bloquants
1. **Sécurité** : bureau en utilisateur non-privilégié (#13)
2. **Signature** : mises à jour Ed25519 + A/B (#31)
3. **Release** : workflow validé sur GitHub Actions (#32)
4. **Licences** : notices complètes (#33)

### Importants
5. Corbeille fonctionnelle (#10)
6. Copie avec progression (#11)
7. Compléter kernel32/msvcrt/ntdll (#14)
8. COM/OLE complet (#16)
9. C++ MinGW et MSVC (#25)
10. Accessibilité (#28)

### Objectifs long terme
11. Runtime CLR/WPF/WinForms (#22)
12. Codecs vidéo (#23)
13. Tests matériel réel
14. Installateur SSD généraliste

## Historique des versions

| Version | Date | Contenu principal |
|---------|------|-------------------|
| v0.2.6 | Oct 2026 | Applications natives, hotkeys globales |
| v0.2.5 | Oct 2026 | Améliorations bureau et paramètres |
| v0.2.4 | Oct 2026 | Corrections services desktop, file manager |
| v0.2.3 | Oct 2026 | Release système complète validée |
| v0.2.2 | Oct 2026 | Registre NT, API mémoire PE |
| v0.2.1 | Oct 2026 | Menu Démarrer, système de mise à jour |
| v0.2.0 | Oct 2026 | Série de previews (1-15) |
