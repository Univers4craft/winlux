# Feuille de route

> Le détail tâche par tâche est dans [Suivi.md](https://github.com/Univers4craft/winlux/blob/master/Suivi.md), la source unique de vérité, et dans les [issues](https://github.com/Univers4craft/winlux/issues). Cette page donne la vue d'ensemble au 1er novembre 2026.

## Déjà fait (grandes étapes)

| Étape | Contenu |
|---|---|
| Runtime PE | Chargeur PE32+ x86-64, ~100 DLL réimplémentées, SEH et exceptions C++, registre, chemins, UAC. |
| Bureau Windows 11 | Session XFCE habillée, menu Démarrer, Paramètres (29 pages), Explorateur et applications maison. |
| ISO installable | Live, installateur graphique (automatique, manuel, LUKS, double démarrage), Secure Boot, récupération. |
| Base native | Ubuntu 26.04 avec APT, mises à jour par paquet `.deb` avec retour arrière (v0.2.12). |
| Multimédia et jeux | Lecteur FFmpeg, son PipeWire, Direct2D/DirectWrite/WIC maison, Direct3D 9/11/12 vers OpenGL/Vulkan, manettes. |
| Intégration | Clés USB et partitions Windows avec lettres, lecteurs réseau, partage Samba, impression CUPS, presse-papiers et glisser-déposer communs. |
| Identité | Logo Prisme, fonds, son de démarrage, aucune marque Microsoft. |
| Langues | Bureau en quatre langues, installateur en six. |

## Prochaine version : v0.2.13

- Outil de mise à jour réparé (le dernier passage obligé par une réinstallation), manifeste publié avec chaque version, repli sur l'API des releases GitHub.
- Changement rapide d'utilisateur, impression, son PipeWire, lecteurs SMB/NFS par le noyau.
- Correctifs de l'audit du code et de l'interface du 1er novembre (Éditeur, boucles de messages, plantage du Terminal, menu Démarrer, barre des tâches…).

## En cours (pull requests ouvertes)

| Sujet | Pull request |
|---|---|
| Installateur, DNS dans les VM, mise à jour sans manifeste (v0.2.13) | #200 |
| Session native : menu Démarrer fiable, barre des tâches, compositeur, finition Windows 11 | #203 |
| Corrections de l'audit (runtime, shell, scripts, décodeurs, Winsock IPv6) | #206, #218, #220, #221, #222 |
| Paint.NET jusqu'à son propre code (`Program.Start`) | #201 |
| Réseau local partagé Windows ↔ Linux, ports ouverts, proxy commun | #186 |
| Identité visuelle, suite : écran du noyau, sons, icônes | #187 |
| Démarrage plus rapide (menu GRUB de 2 s) | #188 |
| Portable : veille, luminosité, écrans multiples, pavé tactile | #149 |

## Priorités suivantes

### Sécurité et fiabilité du système

- Bureau de l'ISO en **utilisateur non privilégié**, avec autorisation ciblée des actions système (#13).
- **Mises à jour signées** (Ed25519) et **partitions A/B** résistantes aux coupures (#31, [docs/AB_UPDATES.md](https://github.com/Univers4craft/winlux/blob/master/docs/AB_UPDATES.md)).
- Validation sur de vrais PC : veille prolongée, capot, Wi-Fi, son, GPU variés.
- Notices de licences complètes pour une distribution publique.

### Compatibilité Windows

- **Paint.NET** : `__CxxFrameHandler4`, puis **WPF** (#138).
- **WinForms**, puis WPF pour les applications .NET de bureau ; génériques, délégués et LINQ dans le CLR maison.
- **COM** complet : activation depuis le registre, serveurs hors processus, automatisation.
- **Media Foundation / DirectShow** branchés sur le moteur multimédia, pour que les `.exe` lisent la vidéo.
- **Direct3D** : shaders DXBC/DXIL, D3D12 complet, autres codecs DXVA2, validation sur vrais GPU (#137).
- Processus : héritage des handles, objets nommés partagés, `FreeLibrary` réel, `StartServiceCtrlDispatcher`.
- Programmes **32 bits**.
- Clients Windows de ChatGPT et Claude (applications Electron).

### Bureau

- Corbeille complète (#10), copie avec progression, pause et annulation (#11), réglages rapides complets (#12).
- Navigation au clavier dans toutes les fenêtres du shell ; rendu revalidé en 1080p, 1440p et 4K.
- Bureau vide par défaut, icônes système facultatives.
- Traduction des dernières applications (#139) et de tout l'installateur (#140).
- « Ouvrir avec… » dans l'Explorateur (#152).

### Idées proposées, pas encore commencées

- Jeux : Direct3D 11 complet, Steam, overlay FPS.
- Suite bureautique et lecteur PDF avancé, navigateur intégré.
- Finition Windows 11 de la session native : Snap Layouts, animations, bureaux virtuels, widgets, panneau de notifications.
- Menu de récupération étendu, clé USB de secours, réinitialisation du PC.
- Site web et documentation utilisateur.

Voir aussi : [Limites et problèmes connus](Limites-et-problemes-connus)
