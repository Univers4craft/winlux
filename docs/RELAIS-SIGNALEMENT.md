# Mettre en ligne le relais de signalement

Le relais permet à n'importe quel utilisateur de Winlux d'envoyer un rapport
**sans compte GitHub**. Il n'est pas encore hébergé : tant qu'il ne l'est pas,
Winlux ouvre le formulaire GitHub prérempli et prévient clairement qu'un compte
GitHub gratuit est nécessaire. Rien d'autre ne change quand le relais arrive.

Le code est prêt : `integration/report-relay/winlux-report-relay.py` (Python 3
standard, aucune dépendance) et `winlux-report-relay.service` (systemd).

## Ce qu'il faut

- Une petite machine joignable en HTTPS (un VPS à quelques euros par mois
  suffit ; 50 Mo de mémoire).
- Un nom de domaine, par exemple `signalement.winlux.example`.

## 1. Créer le jeton GitHub

GitHub > Settings > Developer settings > Fine-grained tokens > Generate new token :

- Repository access : **Only select repositories** > `Univers4craft/winlux` ;
- Permissions : **Issues : Read and write**, rien d'autre ;
- Expiration : un an (le renouveler ensuite de la même façon).

## 2. Installer le relais sur le serveur

```sh
sudo install -Dm755 winlux-report-relay.py /opt/winlux-relay/winlux-report-relay.py
sudo install -Dm600 /dev/stdin /etc/winlux-relay/token   # coller le jeton, puis Ctrl+D
sudo install -Dm644 winlux-report-relay.service /etc/systemd/system/winlux-report-relay.service
sudo systemctl enable --now winlux-report-relay
```

Le service écoute sur `127.0.0.1:8787`, tourne sous un utilisateur temporaire
sans droits et ne garde aucun journal des rapports ni des adresses.

## 3. Le publier en HTTPS

Avec Caddy (certificat automatique), `/etc/caddy/Caddyfile` :

```
signalement.winlux.example {
    reverse_proxy 127.0.0.1:8787
}
```

Vérification : `curl -X POST https://signalement.winlux.example/report -d '{}'`
doit répondre `400` avec « titre ou corps manquant ».

## 4. Brancher Winlux dessus

Livrer dans l'image (ou dans une mise à jour) le fichier
`/usr/share/winlux/report.conf` :

```
RELAY=https://signalement.winlux.example/
```

Paramètres > Signaler un problème affiche alors « Automatique : service de
signalement Winlux, sans compte », et les rapports partent directement.

## Réglages du relais

| Variable | Défaut | Rôle |
|---|---|---|
| `WINLUX_RELAY_PER_IP` | 5 | rapports par heure et par adresse |
| `WINLUX_RELAY_GLOBAL` | 60 | rapports par heure au total |
| `WINLUX_RELAY_REPO` | `Univers4craft/winlux` | dépôt cible |
| `WINLUX_RELAY_HOST` | `127.0.0.1` | adresse d'écoute |

Protections intégrées : seuls les rapports au format de `winlux-report` sont
acceptés, les étiquettes sont filtrées, les `@mentions` neutralisées, un
commentaire ne peut viser qu'une issue `signalement-utilisateur`, et un
plantage déjà signalé (même signature `wlx-…`) devient un commentaire au lieu
d'une nouvelle issue. En cas d'abus, révoquer le jeton sur GitHub coupe tout
immédiatement ; Winlux repasse alors au formulaire prérempli.
