# Installation silencieuse (fichier de réponse)

Pour déployer Winlux sur plusieurs postes, un fichier de réponse en YAML ou
JSON pré-remplit l'installateur. Les pages renseignées sont sautées ; avec
`unattended: true`, l'installation démarre sans aucun clic.

Exemple complet : [`integration/autoinstall/winlux-autoinstall.example.yaml`](../integration/autoinstall/winlux-autoinstall.example.yaml).

## Où le placer

L'installateur prend le premier fichier trouvé :

1. variable `WINLUX_AUTOINSTALL=/chemin/fichier.yaml` ;
2. paramètre noyau `winlux.autoinstall=/chemin/fichier.yaml` ;
3. racine du média live : `winlux-autoinstall.yaml`, `.yml` ou `.json` ;
4. `/etc/winlux/autoinstall.yaml` ou `.json`.

Pour l'intégrer à l'ISO : `WINLUX_AUTOINSTALL=mon-fichier.yaml scripts/build-live-iso.sh`.
Le fichier est aussi couvert par la vérification d'intégrité du média.

## Réglages

| Clé | Valeur | Page sautée |
|---|---|---|
| `version` | `1` | — |
| `unattended` | `true` / `false` | Résumé (si `true`) |
| `reboot` | redémarre 10 s après la fin (`unattended` seulement) | — |
| `language` | `fr_FR`, `en_US`, `de_DE`, `es_ES`, `it_IT`, `pt_BR` | Langue (et Clavier) |
| `keyboard` | `fr`, `us`, `gb`, `de`, `es`, `it`, `br`, `ch`, `be`, `ca` | Clavier |
| `timezone` | `Europe/Paris`… | — |
| `license_accepted` | `true` | Licence |
| `partitioning.mode` | `full` (disque entier effacé, seul mode) | Type |
| `partitioning.disk` | `auto` ou `/dev/sda`, `/dev/nvme0n1`… | Disque |
| `account.hostname` | nom de la machine | — |
| `account.username` | minuscules, chiffres, `-`, `_` (pas `root`) | Compte (avec un mot de passe) |
| `account.fullname` | nom affiché | — |
| `account.password_hash` | empreinte `openssl passwd -6` (recommandé) | — |
| `account.password` | mot de passe en clair (à éviter) | — |
| `account.autologin` | `true` / `false` | — |
| `packages` | liste de paquets APT | — |

`unattended: true` exige `language`, `license_accepted: true`,
`partitioning.disk` et un compte complet ; sinon le fichier est refusé et
l'installateur s'ouvre normalement. `disk: auto` ne choisit un disque que s'il
n'y en a qu'un seul éligible ; sinon la page Disque s'affiche.

Les paquets sont installés au premier démarrage du système installé, dès que
le réseau répond (journal : `/var/log/winlux-first-boot-packages.log`).

## Compte local uniquement

Winlux ne crée que des comptes locaux : pas de compte en ligne, pas d'adresse
e-mail, pas d'avatar synchronisé. Toute clé non listée ci-dessus (par exemple
`email`, `online_account`, `avatar`) est une erreur, pas un réglage ignoré, pour
qu'une faute de frappe ne passe jamais inaperçue.

## Tester un fichier

```sh
python3 scripts/test-autoinstall.py
python3 -c 'import sys; sys.path.insert(0, "scripts"); import winlux_autoinstall as a; print(a.load("mon-fichier.yaml"))'
```
