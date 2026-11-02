# Vrais jeux Windows sous Winlux

Suivi des jeux du commerce ou libres, **64 bits, sans Steam**, lancés tels
quels (le `.exe` Windows d'origine) par le chargeur PE de Winlux, sans Wine
ni Proton. Chaque jeu demande un vrai GPU (OpenGL, Vulkan ou Direct3D).

Ce fichier sert aux fils 32 bits (#232), shaders D3D (#137), manettes (#271),
OpenGL/Vulkan (#233) et HTTPS (#235) : chaque manque y est rattaché.

## Jeux suivis

| Jeu | Version | Moteur graphique | Bibliothèques | Téléchargement |
|---|---|---|---|---|
| SuperTuxKart | 1.5 (64 bits) | OpenGL 3.3 (moteur « Antarctica ») | SDL2, OpenAL, libcurl, shaderc, MinGW + msvcrt | `SuperTuxKart-1.5-win.zip` sur SourceForge |
| Warzone 2100 | 4.7.0 (x64) | OpenGL, Vulkan, Direct3D 11 (ANGLE) au choix (`--gfxbackend`) | SDL3, OpenAL, libcurl, crashpad, llvm-mingw + UCRT + libc++ | `warzone2100_win_x64_archive.zip` sur SourceForge |

Les deux sont choisis parce qu'ils sont gratuits, librement redistribuables,
en 64 bits et exigeants en GPU. Ils couvrent les deux CRT de MinGW (msvcrt et
UCRT), SDL2 et SDL3, et les trois API graphiques.

## Où en est chaque jeu

Machine de test réelle : PC de damien, Intel Iris Xe (Alder Lake GT2),
Mesa 25.2.8, OpenGL 4.6, Vulkan 1.4 (ANV), session X11.

| Étape | SuperTuxKart | Warzone 2100 |
|---|---|---|
| Imports résolus (analyse statique) | oui, 0 manquant | oui, 0 manquant |
| Démarrage de la CRT et des DLL | oui | oui |
| Fenêtre | oui (fenêtre 1040×815 et plein écran 1920×1080) | oui |
| Menu | oui | oui |
| Partie jouable | **oui, 117 FPS** (limite `max_fps=120` du jeu), fenêtre et plein écran | **oui, 60 FPS** (synchro d'un écran 60 Hz) en OpenGL 4.6 et Vulkan 1.4 ; ANGLE (Direct3D 11, OpenGL ES) échouait et le jeu basculait seul sur Vulkan |

Souris, clavier et son de SuperTuxKart restent à confirmer en jouant une
course à la main (la session était verrouillée pendant la mesure).

### Corrigé en chemin

1. **Passage 1** : SuperTuxKart refusait de se charger (`DNSAPI.dll` et
   `HID.DLL` absentes) ; Warzone 2100 s'arrêtait sur `_mbtowc_l`, puis
   `__p__acmdln` rendait 0. Ajout de ≈ 400 fonctions (CRT et Win32), de
   `hid.dll` et de `dnsapi.dll` (`runtime/src/dlls/crt_extra.rs`,
   `runtime/src/dlls/games_win32.rs`).
2. **`FindFirstFileW`** écrivait `cFileName` 4 octets trop loin (FILETIME
   aligné sur 8 au lieu de 4) : tout vrai `.exe` voyait des noms de fichiers
   vides. Corrigé dans le runtime et dans les PE de test.
3. **Passage 2** : SuperTuxKart plantait dans la création du curseur de SDL2
   (`CreateDIBSection` refusait `BI_BITFIELDS` 32 bits sur `GetDC(NULL)` et
   laissait `ppvBits` indéfini) ; Warzone 2100 lisait
   `PEB->LoaderLock->OwningThread` (pointeur nul). Corrigés.
4. `_filelength` prenait un `FILE*` au lieu d'un descripteur.
5. **Passage 3** : SuperTuxKart s'affichait dans un coin de la fenêtre avec la
   mise à l'échelle de l'écran ; un jeu qui dessine lui-même (Direct3D, DXGI,
   OpenGL, Vulkan, SDL, GLFW, ANGLE) est maintenant lancé à l'échelle 1.
   `crashpad_handler.exe` n'est plus lancé (Warzone 2100 continue sans lui).
6. **Passage 4** (Warzone 2100) : `CloseHandle(-1)` échouait et le `CHECK` de
   crashpad gelait le jeu ; `SleepConditionVariableSRW` expirait avec 258 au
   lieu de 1460 ; la table `ID3D11DeviceContext` n'avait pas les 38 méthodes
   `Get*` (72 à 109), donc ANGLE appelait `Flush` à la place de
   `ClearState` et plantait. Test : `offset_of!(clear_state) == 110 × 8`.
7. **Passage 5** : escarmouche complète (carte Sk-Rush, 3 IA) en OpenGL et
   Vulkan. ANGLE refusait de présenter (« DXGI 1.2 required ») car l'appareil
   D3D11 ne répondait pas à `IDXGIDevice2` : l'appareil expose maintenant
   `IDXGIDevice` 0 à 3, avec `GetAdapter`/`GetParent` vers l'adaptateur DXGI.
   À vérifier : l'horloge de jeu semblait avancer trop vite avec
   `--autogame` (≈ 20 min de jeu en 30 s).

## Manques connus, par fil

| Manque | Jeux touchés | Effet aujourd'hui | Fil |
|---|---|---|---|
| Entrée brute (`WM_INPUT`) : `RegisterRawInputDevices` accepte, mais aucun périphérique n'est listé et `WM_INPUT` n'arrive jamais | tous les jeux SDL (souris relative, vue à la souris) | SDL retombe sur les messages souris classiques ; la vue à la souris des FPS reste à vérifier | #271 |
| Manettes HID (`hid.dll` sans périphérique, `SetupDi*` vide pour la classe HID) | SDL2/SDL3 | seules les manettes XInput sont vues | #271 |
| Changement de résolution (`ChangeDisplaySettingsEx` répond « réussi » sans changer le mode) | plein écran exclusif | le jeu tourne à la résolution du bureau | #271 |
| Rampe gamma (`SetDeviceGammaRamp` refusée) | SDL (luminosité) | réglage de luminosité sans effet | — |
| Chaîne de certificats Schannel (`CertCreateCertificateChainEngine`, `CryptQueryObject`…) | libcurl de Warzone 2100 (vérification de mise à jour, actualités) | HTTPS en échec, le jeu continue sans | #235 |
| `CryptImportKey`/`CryptEncrypt` (NTLM de libcurl) | libcurl | authentification NTLM indisponible | #235 |
| Sélecteur de dossier `SHBrowseForFolderW` (répond « Annuler ») | SDL3 (`SDL_ShowOpenFolderDialog`) | pas de choix de dossier | — |
| Instantané des modules (`Module32FirstW`), `TransactNamedPipe` | crashpad (rapport de plantage de Warzone 2100) | pas de rapport crashpad | — |
| Shaders HLSL compilés (DXBC/DXIL) | Warzone 2100 en `--gfxbackend=directx` (ANGLE produit du HLSL pour D3D11) | à mesurer | #137 |
| libcurl : `curl_global_init` échoue | Warzone 2100 | partie en ligne et mises à jour indisponibles | #235 |
| Tubes nommés entre processus et héritage de handles | crashpad | pas de rapport de plantage | — |
| 32 bits | versions i686 des deux jeux (non testées) | — | #232 |

## À vérifier sur le vrai PC (pas de GPU dans une VM)

- SuperTuxKart : une course jouée à la main (souris, clavier, son) ;
- Warzone 2100 avec chacun des moteurs : `--gfxbackend=opengl`, `vulkan`,
  `directx` ;
- souris et clavier dans le menu et en partie ; plein écran (Alt+Entrée) ;
- son (OpenAL sur la sortie audio Winlux).

## Tests

- `testc/games_suite.c`, compilé deux fois par `scripts/build-mingw-tests.sh`
  (msvcrt et UCRT) et lancé par `scripts/test-mingw.sh` : setjmp/longjmp,
  maths C99, multioctets, `_wstat64`, `_wfindfirst64`, `FindFirstFileW`,
  `_wfullpath`, `_fdopen`, `freopen`, `PostThreadMessageW`, entrée brute,
  `HidD_GetHidGuid`, `DnsQuery_W`, hachage CryptoAPI, `WinHttpCrackUrl`.
  Les deux `.exe` sont copiés dans l'ISO (`/opt/winlux/tests`).
- `scripts/test-games-imports.py` (dans `scripts/test-all.sh`, test
  `jeux-imports`) : chaque import système des deux jeux
  (`docs/jeux/imports-jeux.tsv`, 1 593 lignes) doit être fourni par
  `winlux --exports`.

## Mesurer un nouveau jeu

```sh
winlux --exports > exports.tsv
python3 scripts/compat-imports.py exports.tsv DOSSIER_DU_JEU
```

puis lancer le jeu sur une machine avec GPU et relever, à chaque arrêt, la
fonction manquante (`WINLUX_MISSING=ignore` liste les suivantes) ou le
plantage (`crash.log`, `gdb`).
