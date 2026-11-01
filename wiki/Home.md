# Wiki de Winlux

> **Winlux est un système d'exploitation sur noyau Linux où les programmes Windows (`.exe`) se lancent d'un double-clic, grâce à une couche de compatibilité écrite de zéro en Rust, sans Wine et sans machine virtuelle Windows cachée. Son bureau reprend l'ergonomie de Windows 11, avec sa propre identité visuelle, « Prisme ».**

Ce wiki explique ce qu'est Winlux, pourquoi il existe, comment il est construit, ce qui fonctionne aujourd'hui et ce qui reste à faire. Il décrit l'état réel du code sur la branche `master` : quand une fonction est partielle ou en cours, c'est écrit.

| | |
|---|---|
| **Dernière version publiée** | [v0.2.12](https://github.com/Univers4craft/winlux/releases/tag/v0.2.12) (1er novembre 2026) |
| **En préparation** | v0.2.13 |
| **Statut** | Préversion technique : à tester en machine virtuelle d'abord |
| **Licence** | GPL 3 ou ultérieure |
| **Dépôt** | [Univers4craft/winlux](https://github.com/Univers4craft/winlux) |

## Sommaire

### Comprendre Winlux

1. [But et principes](But-et-principes) : pourquoi Winlux existe, ce qu'il veut être et les règles qu'il s'impose.
2. [Architecture](Architecture) : les couches du système, du noyau Linux jusqu'au bureau, et le trajet d'un `.exe` double-cliqué.
3. [Bureau et applications](Bureau-et-applications) : la session de bureau habillée Windows 11 et le shell PE interne, avec la liste des applications intégrées.

### Ce que Winlux sait faire

4. [Compatibilité des applications](Compatibilite-des-applications) : DLL réimplémentées, graphismes, son, .NET, installateurs, ce qui marche et ce qui ne marche pas.
5. [Registre Windows](Registre-Windows) : le registre partagé, son format, `reg.exe` et `regedit`.
6. [Chemins et droits](Chemins-et-droits) : `C:\` ↔ `/home`, lettres de lecteur, droits des fichiers, UAC et autorisations des applications.
7. [Réseau et disques](Reseau-et-disques) : réseau, pare-feu, partage de dossiers, lecteurs réseau, clés USB et partitions Windows.
8. [Paramètres](Parametres) : les 29 pages de Paramètres et la règle « masqué plutôt que grisé ».

### Installer et maintenir

9. [Installation](Installation) : machine virtuelle, clé USB, vrai PC, double démarrage, premier démarrage, récupération.
10. [Publication et mises à jour](Publication-et-mises-a-jour) : comment une version est publiée et comment un système installé se met à jour.

### Développer

11. [Construire l'ISO](Construire-l-ISO) : préparation de la base Ubuntu, assemblage de l'image, Secure Boot, VM de test.
12. [Tests et CI](Tests-et-CI) : les workflows GitHub Actions et les tests à lancer en local.
13. [Comment contribuer](Comment-contribuer) : environnement, conventions, règles de pull request.

### Suivre le projet

14. [Feuille de route](Feuille-de-route) : ce qui reste à faire, par priorité.
15. [Limites et problèmes connus](Limites-et-problemes-connus) : ce qui ne fonctionne pas encore et les pièges déjà rencontrés.
16. [FAQ](FAQ) : les questions qui reviennent.

## Liens utiles

- [Télécharger la dernière ISO](https://github.com/Univers4craft/winlux/releases/latest)
- [Suivi.md](https://github.com/Univers4craft/winlux/blob/master/Suivi.md), la source unique de vérité des tâches
- [Issues ouvertes](https://github.com/Univers4craft/winlux/issues) · [Pull requests](https://github.com/Univers4craft/winlux/pulls) · [Actions](https://github.com/Univers4craft/winlux/actions)
- [Documentation technique `docs/`](https://github.com/Univers4craft/winlux/tree/master/docs)

> Les pages de ce wiki sont versionnées dans le dossier [`wiki/`](https://github.com/Univers4craft/winlux/tree/master/wiki) du dépôt et publiées automatiquement sur le wiki GitHub à chaque fusion sur `master`. Pour corriger une page, modifiez le fichier dans `wiki/` par une pull request : une modification faite directement sur le wiki GitHub serait écrasée à la publication suivante.
