# Signaler un problème depuis Winlux

Winlux propose à l'utilisateur de signaler un bug sur
[les issues GitHub du projet](https://github.com/Univers4craft/winlux/issues),
avec tout ce qu'il faut pour le diagnostiquer. Rien ne part sans action
volontaire : pas de télémétrie, pas d'envoi silencieux, aucun compte imposé.

## Où l'utilisateur le trouve

| Point d'entrée | Ce qui se passe |
|---|---|
| Plantage d'un `.exe` dans le bureau Winlux | Après l'écran bleu, la boîte « Signaler un problème » s'ouvre avec la cause probable. |
| Fonction Windows absente, erreur interne du runtime | Pas d'écran bleu : le bureau ouvre directement la même boîte (`/run/winlux/crash-offer`). |
| Double-clic hors du bureau Winlux (GNOME, KDE, XFCE…) | `winlux-open` lance `winlux-report --crash --since <début>` si un plantage a été enregistré. |
| Paramètres > Signaler un problème | Nouveau signalement, dernier plantage, rapports enregistrés, et les réglages ci-dessous. |
| Observateur d'événements | Bouton « Signaler » : le plantage sélectionné, sinon un signalement libre. |
| Paramètres natifs (session XFCE) | Système > « Signaler un problème ». |

La proposition après plantage se désactive dans Paramètres (« Ne rien
proposer ») : le plantage reste dans le journal et l'Observateur.

## Contenu du rapport

- **Plantage** : exception ou signal, code, stop code, module fautif avec
  décalage (`setup.exe+0x1a2b`, `d3d9.dll+0x…` ou `winlux+0x…` pour le runtime),
  registres, pile d'appels (chaîne des cadres, sinon balayage de pile marqué
  `?`), fonction Windows manquante le cas échéant, dernières lignes du journal
  Winlux et de la sortie du programme.
- **Cause probable en clair** : pointeur nul après une fonction incomplète,
  fonction non implémentée, faute dans Winlux lui-même, instruction CPU absente
  (AVX2/AVX-512), débordement de pile, point d'arrêt, division par zéro.
- **Programme** : nom, version de fichier, produit, éditeur, architecture,
  type (console/graphique/.NET), taille, empreinte SHA-256 tronquée.
- **Machine** : version et commit de Winlux, noyau, base, processeur et
  extensions, GPU et pilote, rendu OpenGL, mémoire, type de session (bureau
  Winlux ou autre, X11/Wayland, Live ou installé), définition, virtualisation,
  langue.

Côté runtime, `runtime/src/crash.rs` écrit ces données dans
`/run/winlux/journal/crash.log`. La pile est lue par sondage sûr : chaque mot
passe par un tube, une adresse illisible renvoie `EFAULT` au lieu de refaire
planter le gestionnaire. `record_failure` couvre les échecs sans signal
(fonction absente, panique Rust).

## Vie privée

- Le rapport complet s'affiche **avant** l'envoi, modifiable (titre et corps).
- Masquage activé par défaut (réglable dans Paramètres, et case à cocher dans
  la boîte) : dossier personnel et chemins de l'utilisateur (`~/…/<fichier>.pdf`,
  y compris `C:\Users\…` et les noms avec espaces), nom d'utilisateur, nom de
  machine, SSID Wi-Fi connus, numéros de série et UUID DMI, `machine-id`,
  adresses e-mail, MAC et IP, jetons. Le nom du `.exe` concerné reste visible
  car il sert au diagnostic.
- Avant l'envoi, seule la signature du plantage (`wlx-` + 12 caractères
  hexadécimaux, sans donnée personnelle) ou les mots du résumé servent à
  chercher un doublon, au moment où l'utilisateur clique « Envoyer ».
- Chaque rapport est aussi enregistré localement dans
  `~/.local/state/winlux/reports/`.

## Déduplication

La signature combine le nom du programme, l'exception et l'emplacement fautif
(arrondi à 16 octets), ou la fonction manquante. Elle figure dans le corps
(`Signature : \`wlx-…\``). Si une issue la contient déjà, la boîte propose
« Ajouter à #N » : le rapport devient un commentaire « Nouvelle occurrence ».
Pour un signalement libre, les issues ouvertes au titre semblable sont
proposées. Le relais refait cette recherche de son côté.

## Envoi sans compte GitHub : le choix

GitHub n'accepte pas de création d'issue anonyme. Trois options ont été
considérées :

1. **Jeton embarqué dans l'ISO** : rejeté. N'importe qui l'extrait de l'image
   en quelques minutes et peut spammer ou vandaliser le dépôt ; GitHub révoque
   d'ailleurs automatiquement les jetons qu'il détecte publiés.
2. **Relais Winlux** (retenu comme chemin principal) : un petit service
   (`integration/report-relay/winlux-report-relay.py`, Python standard, aucune
   dépendance) détient le jeton côté serveur. Il n'accepte que des rapports au
   format `winlux-report`, n'autorise que les étiquettes connues, neutralise
   les `@mentions`, ne commente que des issues `signalement-utilisateur`, limite
   le débit (5 rapports par heure et par adresse, 60 au total, réglables) et ne
   conserve ni rapport ni adresse.
3. **Navigateur** (repli automatique) : formulaire d'issue prérempli via le
   modèle `.github/ISSUE_TEMPLATE/rapport-winlux.md`. Il exige un compte GitHub
   au moment de valider. Un rapport trop long pour une URL (environ 7 000
   caractères) est tronqué dans le lien et copié en entier dans le
   presse-papiers.

Limites : tant que le relais n'est pas déployé, le chemin sans compte n'existe
pas et Winlux utilise le navigateur. Paramètres l'indique, et au clic sur
« Envoyer » une fenêtre explique qu'un compte GitHub gratuit est nécessaire,
avec trois choix : ouvrir GitHub, créer un compte, ou seulement enregistrer. Le relais est un
service en ligne à héberger et surveiller ; la limitation de débit freine le
spam sans l'empêcher totalement. Un mode « Fichier » enregistre seulement le
rapport, pour l'envoyer autrement.

## Déployer le relais

Voir [RELAIS-SIGNALEMENT.md](RELAIS-SIGNALEMENT.md) : jeton, service systemd, HTTPS et
branchement de Winlux, en une page.

Réglages reconnus : `OFFER=on|off`, `REDACT=on|off`, `MODE=auto|browser|file`,
`RELAY=https://…`, et `TOKEN_FILE=…` pour une machine de développement qui
envoie directement avec son propre jeton.

## Étiquettes

Les rapports utilisent `bug`, `signalement-utilisateur`, `plantage`,
`fonction-manquante`, `à-trier`, `compatibilité-windows`, `runtime` et
`bureau` ; elles existent toutes sur le dépôt (créées le 31/10/2026).

## Tests

`python3 -B scripts/test-report.py` (lancé par la CI) vérifie la lecture du
journal, la cause probable, le masquage, la signature, le modèle d'issue, le
lien navigateur, les réglages et la validation du relais, sans réseau.
