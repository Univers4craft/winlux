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

## Prochaine version : v0.2.14

- Sécurité : sur un système installé, le bureau et les .exe tournent sous le compte de l'utilisateur, plus en root (#258).
- Démarrage plus rapide : menu GRUB caché 1 s, noyau silencieux, matériel et services hors du chemin critique (#254, #259, #260).
- Claude Code (claude.exe officiel) tourne par la couche PE maison, avec une entrée au menu Démarrer (#255, #257).
- Applications Ubuntu traduites en quatre langues (#253) et correctifs de la campagne de tests en VM : raccourcis Windows, menu clic droit du bureau, glisser-déposer et Corbeille de l'Explorateur, copier-coller, icônes de la barre des tâches (#252, #263, #264).

## En cours (pull requests ouvertes)

Aucune pull request ouverte pour la v0.2.14.

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

### Idées proposées, pas encore commencées (02/11/2026)

- Petits plaisirs : éclairage nocturne (#274), emojis Win+. (#275), historique Win+V natif (#276), Snap Layouts, Win+Tab et bureaux virtuels natifs (#277), Horloge (#278), pipette et petits outils (#279), copier le texte d'une image (#280), thème automatique et diaporama (#281).
- Gros chantiers : widgets Win+W (#282), réinitialiser le PC et clé de secours (#283), lien avec le téléphone (#284), coin jeux puis Steam (#285), assistant intégré (#286), site web et guide utilisateur (#287).

Voir aussi : [Limites et problèmes connus](Limites-et-problemes-connus)
