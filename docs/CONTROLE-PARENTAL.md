# Contrôle parental

Paramètres › Comptes › **Contrôle parental** (page 40, `ms-settings:family-group`), issue #661.

Entièrement local : comptes locaux, aucun compte en ligne, aucune donnée envoyée. Un administrateur surveille un compte **standard** (un compte administrateur ne peut pas être surveillé : il pourrait tout lever lui-même).

## Ce que l'adulte règle

| Réglage | Effet |
|---|---|
| Temps d'écran par jour | Durée maximale (15 min à 8 h, ou illimité) pour chaque jour de la semaine. Le temps compte quand la session du compte est à l'écran et déverrouillée. |
| Plage horaire | Heures où l'ordinateur peut servir (par exemple de 08:00 à 20:00). |
| Avertissements | Bulle 5 minutes puis 1 minute avant la fin. |
| Fin du temps | Écran « Temps d'écran terminé » par-dessus la session ; les applications restent ouvertes derrière. Boutons « Un adulte accorde du temps » (compte administrateur + mot de passe, 15 min à 2 h) et « Changer d'utilisateur ». Quand le temps revient (le lendemain, ou dans la plage), le mot de passe du compte le lève. |
| Accorder du temps | Aussi depuis les Paramètres de l'adulte : + 15 min, + 30 min, + 1 h. |
| Applications bloquées | Programme Windows (`jeu.exe`) ou application Linux : fermée dès qu'elle s'ouvre, tentative notée. Choix parmi les applications installées, celles de l'activité, ou un nom tapé. Les programmes du bureau (panneau, gestionnaire de fenêtres, outils Winlux) ne peuvent pas être bloqués. |
| Sites web | Aucun filtre, « Bloquer les sites de la liste » ou « Autoriser seulement les sites de la liste » ; un site couvre ses sous-domaines. |
| Recherches filtrées | Google, Bing, DuckDuckGo en recherche protégée et YouTube en mode restreint, par les noms publics que ces services documentent (`forcesafesearch.google.com`, `strict.bing.com`, `safe.duckduckgo.com`, `restrict.youtube.com`). |
| Activité | Temps par jour sur 7 jours, applications les plus utilisées (fenêtre active, minute par minute), tentatives bloquées (applications et sites), avec « Bloquer » à côté de chaque application. |

## Pourquoi le compte surveillé ne peut pas le lever

- **Réglages illisibles** : `/etc/winlux/parental/<compte>.json` (root, 0600, dossier 0700) ; activité dans `/var/lib/winlux/parental/<compte>/` (root, 0700). Le compte surveillé ne voit que son temps restant (`winlux-parental status` par le relais).
- **Relais root** (`winlux-root`) : lire les réglages demande un compte administrateur ; les modifier (activer, désactiver, accorder du temps, effacer) demande en plus le Contrôle de compte d'utilisateur, c'est-à-dire le mot de passe d'un administrateur. Les commandes internes (`daemon`, `screen`, `check-login`) ne passent pas par le relais.
- **Session** : hors du temps permis, le relais refuse de déverrouiller la session du compte et d'y basculer, même avec le bon mot de passe ; l'écran de verrouillage dit pourquoi.
- **Horloge** : couper l'heure automatique est réservé aux administrateurs ; pour un compte surveillé, changer de fuseau horaire l'est aussi.
- **Service et écran en root** : `winlux-parental daemon` est lancé par les services système au démarrage (pas de systemd). L'écran de blocage tourne en root : le compte ne peut pas le tuer. S'il le ferme par X11 (`xkill`), il revient aussitôt ; à la deuxième fois, les programmes du compte sont suspendus (SIGSTOP) jusqu'à ce que l'écran soit levé normalement. Une application plein écran qui garde le clavier est fermée.
- **DNS** : nftables (`table inet winlux_parental`, `meta skuid`) redirige le DNS du compte vers le résolveur filtrant du service (port 54000 + UID − 1000, sur 127.0.0.1 et ::1) ; DNS chiffré (port 853) et résolveurs DoH connus refusés pour ce compte ; `use-application-dns.net` coupe le DoH automatique de Firefox.

## Limites connues

- Une application bloquée l'est par son nom de programme : une copie renommée d'un `.exe` n'est pas reconnue. Les applications Flatpak et Snap ne sont pas proposées.
- Le filtre web repose sur le DNS : un VPN, un proxy ou un résolveur DoH inconnu le contournent ; « Autoriser seulement » bloque aussi les domaines techniques des sites autorisés (CDN), à ajouter à la main.
- Le compte doit avoir un UID entre 1000 et 1999 pour le filtre web (comptes créés par Winlux).
- La session Live n'a pas de contrôle parental (page masquée).

## Commandes

```
winlux-parental status                     temps restant du compte appelant
winlux-parental list | get COMPTE | report COMPTE [JOURS]          (administrateur)
winlux-parental set COMPTE (JSON sur l'entrée) | enable | disable | grant COMPTE MIN | forget
```

Test : `scripts/test-parental.py` (CI, `scripts/test-all.sh`, `scripts/test-campaign.py`).
