# Comment contribuer

Toute aide est la bienvenue : tester un logiciel Windows et signaler ce qui manque, traduire, corriger un bug, implémenter une API. Cette page explique comment s'y prendre.

## Tester et signaler

Le plus utile pour commencer : installer Winlux dans une VM ([Installation](Installation)), essayer vos programmes et signaler ce qui ne marche pas.

- **Depuis Winlux** : après un plantage, la boîte « Signaler un problème » s'ouvre avec la cause probable ; sinon Paramètres › Signaler un problème. Le rapport (version, matériel, journal) masque les données personnelles et rien ne part sans votre accord ([docs/SIGNALEMENT-BUGS.md](https://github.com/Univers4craft/winlux/blob/master/docs/SIGNALEMENT-BUGS.md)).
- **À la main** : ouvrez une [issue](https://github.com/Univers4craft/winlux/issues/new/choose) avec :
  - la version (`/usr/share/winlux/BUILD-INFO` ou Paramètres › À propos) ;
  - le logiciel, sa version et où le télécharger ;
  - les étapes pour reproduire ;
  - le journal : `WINLUX_LOG=debug winlux programme.exe 2>&1 | tail -100`.

## Préparer son environnement

```sh
git clone https://github.com/Univers4craft/winlux.git
cd winlux
rustup update stable                          # la CI utilise le dernier Rust stable
rustup target add x86_64-pc-windows-msvc      # pour compiler les .exe de test et du shell
sudo apt install gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 clang lld llvm \
                 xvfb python3-gi libmpg123-0 libfaad2
cargo build --release -p winlux
bash scripts/build-tests.sh && bash scripts/test-runtime.sh
```

Il faut Linux x86-64. Pour construire l'ISO, voir [Construire l'ISO](Construire-l-ISO).

## Où mettre son code

| Je veux… | Où |
|---|---|
| Ajouter ou compléter une API Windows | `runtime/src/dlls/<dll>.rs`, enregistrée dans `runtime/src/dlls/mod.rs` |
| Toucher au chargeur, aux exceptions | `runtime/src/loader.rs`, `pe.rs`, `dlls/seh.rs`, `dlls/cxx_eh.rs` |
| Améliorer le CLR .NET | `runtime/src/clr/` |
| Modifier une application maison (Explorateur, Gestionnaire des tâches…) | `testexe/src/shell/` |
| Modifier le menu Démarrer, les Paramètres GTK, le verrouillage, l'UAC | `scripts/winlux-start-menu.py`, `winlux-settings*.py`, `winlux-lock.py`, `winlux-uac.py` |
| Configurer la session XFCE | `integration/native/`, `scripts/winlux-native-config.py` |
| Ajouter un test PE | `testexe/src/bin/<nom>.rs` **et** une ligne dans `scripts/test-runtime.sh` |
| Documenter | `docs/<SUJET>.md`, et la page concernée dans `wiki/` |

## Règles du projet

1. **Aucun Wine, Proton ni DXVK.** La couche de compatibilité est maison ; la CI refuse toute dépendance à Wine. Les outils Linux standard (FFmpeg, CUPS, Samba, rclone…) sont permis, et leur fonction est masquée s'ils manquent.
2. **Aucun élément de marque Microsoft** : ni logo, ni nom de produit dans l'interface au-delà de la compatibilité, ni application Microsoft par défaut, ni compte en ligne. L'identité est Prisme ([docs/IDENTITE-VISUELLE.md](https://github.com/Univers4craft/winlux/blob/master/docs/IDENTITE-VISUELLE.md)).
3. **Quatre langues** : toute chaîne visible est traduite en français, anglais, espagnol et allemand (et en italien et portugais dans l'installateur). En résolvant un conflit, ne supprimez jamais une langue ni une disposition de clavier.
4. **Masquer plutôt que griser** : une fonction sans matériel ou sans outil n'apparaît pas.
5. **Pas de faux succès** : une fonction non implémentée renvoie une erreur claire, jamais une valeur arbitraire.
6. **`Suivi.md` est la source unique de vérité** des tâches ([AGENTS.md](https://github.com/Univers4craft/winlux/blob/master/AGENTS.md)) : cochez ou ajoutez la tâche dans le même commit que le changement. `TODO.md` et `TRACKING.md` en sont des miroirs. Ce fichier est souvent modifié en parallèle : en cas de conflit, gardez les deux côtés et additionnez les compteurs.
7. **Qualité** : `cargo fmt`, `cargo clippy -- -D warnings`, tests ajoutés, scripts en LF et exécutables.

## Pull requests

- Une branche par sujet, une pull request vers `master`.
- Plusieurs pull requests avancent en parallèle : pour résoudre un conflit, **fusionnez `origin/master` dans votre branche** (pas de rebase ni de force-push sur une branche partagée), puis attendez la CI verte.
- Les pull requests sont fusionnées en *squash*, une à la fois, quand la CI est verte et que le propriétaire du dépôt a donné son accord.
- Fichiers souvent disputés (vérifiez `master` juste avant de les modifier) : `scripts/winlux-settings.py`, `testexe/src/shell/settings.rs` (numéros de pages, `PAGE_COUNT`, `NAV_ORDER`, index `S_*` de `locale.rs`), `testexe/src/shell/explorer.rs`, `desktop.rs`/`wm.rs`, `runtime/src/dlls/gui_x11.rs`, `runtime/src/dlls/mod.rs`, `scripts/build-live-iso.sh`, `Suivi.md`.
- Les fichiers de la chaîne de publication (`release.yml`, `build-live-iso.sh`, `prepare-apt-base.py`, `test-live-iso-qemu.sh`) se modifient en coordination avec la préparation de la version en cours.

## Le wiki

Les pages du wiki sont les fichiers du dossier [`wiki/`](https://github.com/Univers4craft/winlux/tree/master/wiki). Le workflow `wiki.yml` les publie sur le wiki GitHub à chaque fusion sur `master`. Modifiez-les par pull request, comme le code ; un nom de fichier `Ma-page.md` devient la page `Ma-page`, et les liens internes s'écrivent `[Texte](Ma-page)`. `_Sidebar.md` est le menu latéral et `_Footer.md` le pied de page.

## Licence

En contribuant, vous acceptez que votre contribution soit distribuée sous [GPL 3 ou ultérieure](https://github.com/Univers4craft/winlux/blob/master/LICENSE).

Voir aussi : [Tests et CI](Tests-et-CI) · [Feuille de route](Feuille-de-route)
