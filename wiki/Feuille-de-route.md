# Feuille de route

> Le détail tâche par tâche est dans [Suivi.md](https://github.com/Univers4craft/winlux/blob/master/Suivi.md), la source unique de vérité, et dans les [issues](https://github.com/Univers4craft/winlux/issues). Cette page donne la vue d'ensemble au 1er novembre 2026.

## Déjà fait (grandes étapes)

| Étape | Contenu |
|---|---|
| Runtime PE | Chargeur PE32+ x86-64, ~100 DLL réimplémentées, SEH et exceptions C++, registre, chemins, UAC. |
| Bureau Windows 11 | Session XFCE habillée, menu Démarrer, Paramètres (30 pages), Explorateur et applications maison. |
| ISO installable | Live, installateur graphique (automatique, manuel, LUKS, double démarrage), Secure Boot, récupération. |
| Base native | Ubuntu 26.04 avec APT, mises à jour par paquet `.deb` avec retour arrière (v0.2.12). |
| Multimédia et jeux | Lecteur FFmpeg, son PipeWire, Direct2D/DirectWrite/WIC maison, Direct3D 9/11/12 vers OpenGL/Vulkan, manettes. |
| Intégration | Clés USB et partitions Windows avec lettres, lecteurs réseau, partage Samba, impression CUPS, presse-papiers et glisser-déposer communs. |
| Identité | Logo Prisme, fonds, son de démarrage, aucune marque Microsoft. |
| Récupération | Réinitialiser ce PC (conserver mes fichiers ou tout supprimer), clé de secours, démarrage avancé, réparation du démarrage, menu de secours en quatre langues (#283). |
| Langues | Bureau en quatre langues, installateur en six. |

## Prochaine version : v0.2.15

- Aucun navigateur dans l'image : l'installateur, le premier démarrage et Paramètres > Navigateurs proposent Chrome, Edge et Firefox par ordre alphabétique ; Winlux télécharge le choix chez son éditeur et l'installe. La licence d'Edge interdit de le redistribuer : les ISO v0.2.14 et antérieures, qui le contenaient, sont retirées.
- Système au nom de Winlux (« construit à partir de paquets Ubuntu »), liste des sources GPL publiée, mention de non-affiliation à Microsoft dans l'installateur, le premier démarrage et le créateur USB.
- Contrôles et boîtes de dialogue Win32, OpenGL et Vulkan, manettes, MSIX, PuTTY, premiers jeux (#301 à #332).
- « Redémarrer maintenant » de l'installateur réparé en Live (#303), taille de mise à jour affichée seulement quand une mise à jour existe.

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
- Gros chantiers : widgets Win+W (#282), lien avec le téléphone (#284), coin jeux puis Steam (#285), assistant intégré (#286), site web et guide utilisateur (#287).

Voir aussi : [Limites et problèmes connus](Limites-et-problemes-connus)
