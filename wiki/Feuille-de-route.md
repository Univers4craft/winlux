# Feuille de route

> Le détail tâche par tâche est dans les [issues](https://github.com/Univers4craft/winlux/issues), chacune avec un niveau d'urgence (`urgence:1` bloque la prochaine version, `urgence:2` gêne au quotidien, `urgence:3` peut attendre). Cette page donne la vue d'ensemble au 22 novembre 2026.

## Où en est le projet

| | |
|---|---|
| **Dernière version publiée** | [v0.2.22](https://github.com/Univers4craft/winlux/releases/tag/v0.2.22), le 23 novembre 2026 |
| **Prochaine version** | **v0.3** |
| **Bloquent la v0.3** | Les issues `urgence:1` encore ouvertes : #2784 (le contrôle WebBrowser n'exécute aucun script), #2747 (tablette en UEFI 32 bits), #2851 (installeur de OneDrive) |

## Déjà fait (grandes étapes)

| Étape | Contenu |
|---|---|
| Runtime PE | Chargeur PE32+ et PE32 (programmes 64 et 32 bits), ~100 DLL réimplémentées, SEH et exceptions C++, registre, chemins, UAC, WinRT. |
| Bureau | Session XFCE habillée du thème Winlux, menu Démarrer, Paramètres (plus de quarante pages), Explorateur et applications maison, ancrage des fenêtres, bureaux virtuels, widgets, historique du presse-papiers. |
| ISO installable | Live, installateur graphique (automatique, manuel, LUKS, double démarrage), Secure Boot, récupération, BIOS, UEFI 64 et 32 bits. |
| Base native | Ubuntu 26.04 avec APT ; mises à jour signées en Ed25519 avec deux emplacements système et retour automatique (#31). |
| Sécurité | Bureau sous le compte de l'utilisateur (#13), autorisations par application, pare-feu par application, contrat de licence toujours montré avant d'installer, signatures Authenticode vérifiées, contrôle parental, connexion par code PIN et empreinte. |
| Multimédia et jeux | Lecteur FFmpeg, son PipeWire, Direct2D/DirectWrite/WIC maison, Direct3D 9/11/12 vers OpenGL/Vulkan, anciens DirectX, manettes. |
| Intégration | Clés USB et partitions Windows avec lettres, lecteurs réseau, partage Samba, impression CUPS, presse-papiers et glisser-déposer communs. |
| Identité | Logo Prisme, fonds Aurore, Nuit et Aube, sons, icônes, aucune marque Microsoft. |
| Langues | Bureau et applications en français, anglais, allemand et espagnol ; installateur en six langues. |

## Prochaine version : v0.3

Le but : pour chaque logiciel testé, l'installation réussit et la première fenêtre s'ouvre, jusqu'à l'écran de connexion quand il en a un. Un installeur web officiel doit fonctionner tel quel.

- **Office** jusqu'au bout de son installation (#2740).
- **Teams** jusqu'à son écran de connexion (#2511, #2726).
- **Acrobat Reader** et les applications Chromium, Electron et WebView2 (#236).
- **Creative Cloud** : scripts dans le contrôle WebBrowser (#2784).
- **Installeur web de Paint.NET** (#2752).
- **Tablette en UEFI 32 bits** démarrant sur son disque interne (#2747).

## Ensuite

- Autorisations par application plus fines et revue de sécurité.
- Écran tactile (#2749).
- WPF complet (#268) et .NET de bureau (#22).
- Mandataires COM pour les programmes 32 bits (#2428).
- Validation sur une plus large gamme de vrais PC et de GPU.

Voir aussi : [Limites et problèmes connus](Limites-et-problemes-connus)
