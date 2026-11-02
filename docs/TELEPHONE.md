# Téléphone connecté (issue #284)

Relier un téléphone Android à Winlux, comme « Lien avec un téléphone » sous
Windows 11 : notifications du téléphone sur le PC (avec réponse rapide et
« Ignorer »), SMS lus et envoyés depuis le PC avec les noms des contacts,
appels entrants (couper la sonnerie), batterie, « Faire sonner », fichiers et
liens échangés dans les deux sens.

## Côté téléphone

L'application libre **KDE Connect** (F-Droid ou Google Play). Winlux parle son
protocole ouvert, version 8 ; aucune bibliothèque ni aucun programme KDE n'est
embarqué : le côté Winlux est écrit entièrement dans `scripts/winlux-phone.py`
(Python standard + `openssl` pour créer le certificat, déjà dans l'image).

## Côté Winlux

| Élément | Où |
| --- | --- |
| Service de session (un par compte) | `winlux-phone serve --if-paired`, lancé par `winlux-user-session` et `winlux-native-session` une fois le bureau affiché, **seulement si un téléphone est associé** |
| Application « Téléphone connecté » | menu Démarrer (`winlux-phone.desktop`), `winlux-phone gui` ; associe un premier téléphone et lance le service si besoin |
| Paramètres › Bluetooth et appareils › **Appareils mobiles** (page 29) | masquée tant qu'aucun téléphone n'est associé ; interrupteur général, notifications / SMS / fichiers par téléphone, Faire sonner, Dissocier |
| Pare-feu | `winlux-control phone-firewall-on|off` (relais root, niveau « user ») : règles 9447 (TCP 1716, 1739-1764) et 9448 (UDP 1716), réseaux **privés et de domaine** seulement |

Données du compte : `~/.config/winlux/phone/` (identifiant, certificat, clé
privée en 0600, téléphones associés et leur certificat épinglé),
`~/.local/state/winlux/phone/<téléphone>/` (notifications, SMS, contacts, liste
des fichiers reçus), `$XDG_RUNTIME_DIR/winlux-phone/` (état et socket de
contrôle en 0600). Couper une fonction efface ce qu'elle avait enregistré ;
dissocier efface tout ce qui vient du téléphone.

## Sécurité

- Winlux est toujours **client TLS** sur la liaison : il répond aux annonces
  UDP du téléphone par sa propre annonce et c'est le téléphone qui se
  connecte ; il présente son certificat, lit celui du téléphone.
- Rien n'est accepté d'un téléphone non associé (il reçoit `pair: false`).
- Appairage : code de vérification à 8 caractères affiché des deux côtés,
  demande expirée après 30 s, horloges décalées de plus de 30 min refusées.
  Ensuite le certificat est **épinglé** : un autre appareil au même
  identifiant est refusé.
- Réseau public (profil du pare-feu) : aucune annonce, aucune liaison.
- Fichiers reçus : nom assaini (jamais de chemin, ni de fichier caché), écrits
  en `.part` puis renommés, jamais ouverts automatiquement ; place disque
  vérifiée. Les envois vers le téléphone n'acceptent que l'adresse du
  téléphone associé.
- Le service n'est pas une application Windows : il ne donne aucun accès
  nouveau aux `.exe`, dont les autorisations (dossiers personnels, réseau…)
  restent celles de Confidentialité et sécurité.
- Le pare-feu ne demande plus rien pour un programme dont tous les ports en
  écoute sont déjà ouverts par une règle sans programme (Téléphone connecté,
  partage) : `firewall::new_listeners`.

## Test

`scripts/test-phone.py` (CI, job « Tests unitaires ») fait jouer un faux
téléphone Android contre le vrai service sur la boucle locale : découverte,
TLS, identité v8, refus avant appairage, code de vérification, appairage dans
les deux sens, épinglage face à un imposteur, notifications, réponse, SMS,
contacts, fichiers dans les deux sens, batterie, réglages, dissociation,
réseau public, règles du pare-feu et page des Paramètres.

Captures : `/mnt/project-files/winlux/captures/telephone/`.

L'appairage avec un vrai téléphone reste à valider sur le PC de test
(Wi-Fi commun avec un Android et KDE Connect).
