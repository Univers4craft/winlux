# Signaler un problème depuis Winlux

Winlux propose à l'utilisateur de signaler un bug sur
[les issues GitHub du projet](https://github.com/Univers4craft/winlux/issues),
avec tout ce qu'il faut pour le diagnostiquer. Pas de télémétrie, aucun
compte imposé. Seule exception, choisie à l'installation et réversible dans
Paramètres : l'envoi automatique des plantages (ci-dessous).

## Où l'utilisateur le trouve

| Point d'entrée | Ce qui se passe |
|---|---|
| Plantage d'un `.exe` dans le bureau Winlux | Après l'écran bleu, la boîte « Signaler un problème » s'ouvre avec la cause probable. |
| Fonction Windows absente, erreur interne du runtime | Pas d'écran bleu : le bureau ouvre directement la même boîte (`crash-offer` dans le journal du compte, `/run/winlux/journal-<uid>`). |
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
`/run/winlux/journal-<uid>/crash.log` (un journal par compte, 0700, #633 ; `/run/winlux/journal` pour les démons root). La pile est lue par sondage sûr : chaque mot
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

## Journaux de diagnostic (#458)

Sous la description, des cases « Réseau et Wi-Fi », « Son », « Démarrage » et
« Mises à jour » joignent les journaux correspondants, repliés en `<details>`
(80 dernières lignes chacun, raccourcis si le rapport dépasse la taille
d'une issue). Toujours décochées à l'ouverture (#1050) : aucun journal, pas même
celui du réseau, ne part sans que l'utilisateur coche la case.

- Réseau : `/run/winlux/network*.log`, `wpa-supplicant.log`,
  `network-probe.json`, `time-sync.*`, plus `nmcli` (appareils, sans nom de
  réseau), `rfkill list`, `ip -brief link`.
- Son : `pipewire.log`, `wireplumber.log`, `pactl info` et listes courtes des
  sorties et entrées (en `LC_ALL=C`), `aplay -l`.
- Démarrage : `native-early.log`, `native-session.log`, `boot-splash.log`,
  `system-services.log`, `hardware.log`, `/var/lib/winlux/native-boot-failure.log`,
  erreurs du noyau (`dmesg`, si le compte peut le lire).
- Mises à jour : `update.log`, `boot-update.log`, `sysupdate-auto.log`, listes
  des mises à jour, journaux de premier démarrage et des pilotes.

Un fichier absent n'apparaît pas ; un fichier vide est indiqué « vide » (cas
de NetworkManager et wpa_supplicant sur un système installé tant qu'ils
n'écrivent pas dans un fichier) ; un fichier réservé à root est indiqué
« illisible depuis ce compte ». Les plantages encore en file (#438) sont
listés. Le masquage est le même que pour le reste du rapport.
`winlux-report --print --attach all` affiche le tout sans interface.

## Envoi automatique des plantages (#438)

`winlux-report --background`, lancé par l'ouverture de session
(`winlux-session-startup`), attend 2 minutes, passe en priorité minimale et
n'ouvre aucune fenêtre.

- **Collecte** : `crash.log` est en mémoire vive et disparaît à l'arrêt. Chaque
  plantage entre donc dans une file persistante
  (`~/.local/state/winlux/auto-report/queue/`) dès qu'il se produit
  (`winlux-report --crash` lance `--collect`), puis toutes les 5 minutes, et
  l'arrêt anormal précédent (pstore) au démarrage. Au plus 50 rapports en file.
- **Envoi** : seulement si l'horloge est plausible (jamais avant la date de
  construction ; avec l'heure automatique, après la première synchro SNTP ou
  15 minutes), sinon `certificate is not yet valid`. Réseau absent ou refus :
  la file attend le démarrage connecté suivant. Au plus 5 envois par session.
- **Déduplication** : une issue par signature `wlx-…` (`wlx-sys-…` pour un arrêt
  du système) ; si une issue ouverte la porte, un commentaire « Nouvelle
  occurrence », au plus un par jour et par signature (les suivants sont comptés
  dans le commentaire d'après). Issue fermée : nouvelle issue qui la cite.
- **Contenu** : toujours anonymisé, quel que soit le réglage « Masquer » ; en
  français ; étiquettes `bug`, `signalement-utilisateur`, `plantage`…, plus
  `urgence:2-confort` (`urgence:1-quotidien` pour un arrêt du système).
- **Cible** : le relais si `RELAY=https://…`, sinon le jeton GitHub du
  système (Paramètres > Signaler un problème > « Jeton GitHub… »). Sans l'un ni
  l'autre, les rapports restent dans la file.
- **Réglage** : `AUTO=on|off|default` dans `/etc/winlux/report.conf`. La case de
  l'installateur (cochée par défaut, `account.crash_reports` en installation
  automatique) écrit `on` ou `off`. `default` (système installé avant #438) =
  actif seulement si un jeton personnel est présent.
- **Suivi** : `winlux-report --auto-status`, journal
  `~/.local/state/winlux/auto-report/journal.log`.

### Protection du jeton

- Stocké dans `/etc/winlux/report-token`, propriétaire root, droits 0600 : ni le
  compte ni les `.exe` qu'il lance ne peuvent le lire. Il n'est ni dans le
  dossier du compte, ni dans le registre des programmes Windows, ni dans
  l'image ISO.
- Saisi dans Paramètres, il passe à root par l'entrée standard (jamais en
  argument de commande, donc invisible dans la liste des processus) après
  confirmation d'un administrateur (`report-token-set`, niveau admin du relais).
- Les comptes remettent leurs rapports au relais root (`winlux-root control
  report-send`, sur l'entrée standard). Root vérifie la forme (titre, signature
  `wlx-…`, étiquettes connues, tailles), neutralise les `@mentions`, masque à
  nouveau les données personnelles, retire le jeton s'il apparaissait, et
  envoie au plus 10 rapports par démarrage et 30 par jour : un programme qui
  fabriquerait de faux rapports ne peut ni lire le jeton ni inonder le dépôt.
- Il ne part que vers `https://api.github.com` (adresse fixe côté root, sans
  suivre de redirection) et n'est jamais écrit dans un journal.
- Portée : jeton « fine-grained » limité au dépôt `Univers4craft/winlux`,
  permission Issues seulement, avec une date d'expiration. S'il fuitait, il ne
  permettrait que de créer, modifier ou fermer des issues de ce dépôt, et se
  révoque en un clic sur GitHub.
- Limite assumée : root peut toujours lire ce jeton. Le chiffrer sur la même
  machine ne change rien face à root, qui lit aussi la clé (même scellée dans
  le TPM, root peut demander à la desceller). Contre le vol de l'ordinateur,
  la protection réelle est le chiffrement du disque proposé par l'installateur.

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
« Envoyer » la confirmation explique qu'un compte GitHub gratuit est nécessaire,
avec trois choix : Annuler (par défaut), créer un compte, ouvrir sur GitHub. Le relais est un
service en ligne à héberger et surveiller ; la limitation de débit freine le
spam sans l'empêcher totalement. Un mode « Fichier » enregistre seulement le
rapport, pour l'envoyer autrement.

## Relier son compte GitHub (#1049 à #1052)

- **Confirmation avant tout envoi** : au clic sur « Envoyer », une fenêtre dit
  où part le rapport (dépôt public Univers4craft/winlux sous `@compte`, jeton
  de la machine, relais ou formulaire du navigateur), ce qui part (titre,
  taille, journaux joints ou non, masquage) et que la recherche de doublons
  envoie la signature ou quelques mots du résumé. Annuler est le bouton par
  défaut ; la recherche de doublons n'a lieu qu'après accord.
- **Connexion** : par code (flux « device » de GitHub) dès que
  `GITHUB_CLIENT_ID` contient l'identifiant public de l'application GitHub
  Winlux ; sinon par jeton « classic » `public_repo`. Le jeton reste dans
  `~/.config/winlux/github-account.json` (0600).
- **Se déconnecter** : après confirmation, le jeton est envoyé au service
  public de révocation de GitHub (`POST /credentials/revoke`, sans
  authentification), puis effacé de ce PC. Si GitHub ne confirme pas, la page
  où le révoquer à la main s'ouvre.
- **Mes signalements** (Paramètres > Signaler un problème) : les issues
  envoyées depuis ce PC (`~/.local/state/winlux/sent-reports.json`) et celles
  ouvertes par le compte relié ; « Actualiser depuis GitHub » demande leur
  état (ouvert, corrigé, fermé sans suite), seulement sur clic.
- **Plantages en attente** : sans jeton du système, « Envoyer avec mon compte
  GitHub… » les publie sous le nom du compte, après une confirmation qui les
  liste. Jamais en arrière-plan.

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

Chaque rapport, manuel ou automatique, porte aussi une urgence (#611) :
`urgence:1-quotidien` pour un arrêt du système, `urgence:2-confort` sinon, à
revoir au tri. Selon le chemin d'envoi :

- **Relais ou jeton** : les étiquettes sont posées par l'API GitHub. Le relais
  ajoute `urgence:2-confort` à un rapport qui n'en a pas et n'en garde qu'une.
- **Formulaire dans le navigateur** : GitHub n'applique les étiquettes du lien
  (`&labels=…`) qu'aux membres du dépôt. Le modèle `rapport-winlux.md` pose
  `bug`, `signalement-utilisateur` et `à-trier` pour tout le monde, et la
  première ligne du corps (« > Étiquettes / Labels : … ») donne les autres à
  poser au tri.

Le message « formulaire ouvert » n'apparaît qu'une fois le navigateur lancé :
`winlux-browser` écrit `ouvert`, `annule` ou `attente` dans le fichier nommé
par `WINLUX_BROWSER_STATUS` (#612).

## Tests

`python3 -B scripts/test-report.py` (lancé par la CI) vérifie la lecture du
journal, la cause probable, le masquage, la signature, le modèle d'issue, le
lien navigateur, les réglages et la validation du relais, sans réseau.
