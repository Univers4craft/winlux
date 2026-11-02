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
8. **Passage 6** : la vérification « DXGI 1.2 » d'ANGLE passe, puis ANGLE
   plantait dans `createSwapChain` : l'adaptateur gardait un pointeur brut
   sur sa fabrique sans `AddRef`, ANGLE relâchait la fabrique après
   `EnumAdapters` et `GetParent` lisait une fabrique libérée. L'adaptateur
   tient maintenant une référence sur sa fabrique (test `refcount_tests`
   de `dxgi.rs`). L'horloge de jeu est exacte en partie normale (00:56 de
   jeu pour 60 s réelles) ; l'accélération vue avec `--autogame` est
   l'avance rapide du mode spectateur du jeu.

9. **Passage 7** : ANGLE crée son appareil, sa chaîne d'échange et la
   fenêtre, puis nomme chaque objet par `SetPrivateData` (méthode 5 de
   `ID3D11DeviceChild`) : les vues, shaders, dispositions d'entrée et états
   n'avaient que les 3 méthodes d'IUnknown. Leur table a maintenant les
   9 méthodes de `ID3D11DeviceChild` et des vues. SuperTuxKart : pas de
   régression avec #326 (course de démonstration à 117,5 FPS).
10. **Passage 8** : ANGLE s'initialise sur le D3D11 de Winlux (« ANGLE
    Direct3D11 vs_5_0 ps_5_0 », OpenGL ES 3.0), puis
    `ID3D11View::GetResource` rendait un pointeur nul. Les vues gardent
    maintenant leur ressource (avec une référence) et leur descripteur.
11. **Passage 9** : ANGLE ne trouvait pas `d3dcompiler_47.dll`. Un
    `d3dcompiler_47` maison renvoie maintenant un conteneur DXBC qui porte le
    type de shader et garde le texte HLSL. ANGLE crée aussi ses shaders depuis
    ses threads de travail : un contexte GL partagé leur est donné. Plus de
    plantage, la partie tourne, mais l'image reste noire : la traduction des
    shaders DXBC/HLSL vers GLSL manque (#137). `CreateDCW("\\.\DISPLAY1")`
    ouvrait une imprimante au lieu de l'écran (SDL3) : corrigé. Plein écran
    1920×1080 en Vulkan : 60 FPS.
12. **libcurl de Warzone 2100** (8.19, Schannel ; vérification des mises à
    jour, actualités, partie en ligne). `curl_global_init` échouait parce
    que `InitSecurityInterfaceW` rendait une table nulle : la table SSPI
    existe depuis #320. Le reste du chemin de libcurl est maintenant fourni :
    moteur de chaînes aux racines exclusives et `CryptQueryObject` (fichier
    d'autorités), tous les noms DNS du certificat, `CryptDecodeObjectEx`
    (noms alternatifs), `CryptImportKey`/`CryptEncrypt` et MD4 (NTLM).
    Rejoué par `tls_suite.exe` contre un serveur HTTPS local ; à confirmer
    dans le jeu sur le vrai PC.

12. **Passage 10** (après #346 et #347) : `curl_global_init` réussit, curl
    démarre avec Schannel. Les quatre requêtes HTTPS du menu (actualités,
    vérification de mise à jour) échouaient pourtant : au premier appel,
    curl déclare son tampon de sortie `SECBUFFER_EMPTY` et
    `InitializeSecurityContextW` rendait `SEC_E_BUFFER_TOO_SMALL`. Ce tampon
    devient maintenant le jeton, comme sous Windows. Vérifié sur le PC de
    damien : `wz2100_updates.json` et `wz2100_compat.json` (JSON signé du
    serveur) sont téléchargés au lancement, sans erreur réseau ni TLS.
    crashpad reste
    volontairement non lancé (`is_crash_handler`) ; le jeu n'est pas bloqué.

Entrée brute (`WM_INPUT` par evdev), changement de définition (xrandr) et
rampe gamma (RandR) sont arrivés avec #326 ; à revoir en jouant à la souris.

## Manques connus, par fil

| Manque | Jeux touchés | Effet aujourd'hui | Fil |
|---|---|---|---|
| Manettes HID (`hid.dll` sans périphérique, `SetupDi*` vide pour la classe HID) | SDL2/SDL3 | seules les manettes XInput sont vues | #271 |
| Sélecteur de dossier `SHBrowseForFolderW` (répond « Annuler ») | SDL3 (`SDL_ShowOpenFolderDialog`) | pas de choix de dossier | — |
| Instantané des modules (`Module32FirstW`), `TransactNamedPipe` | crashpad (rapport de plantage de Warzone 2100) | pas de rapport crashpad | — |
| Tubes nommés entre processus et héritage de handles | crashpad | pas de rapport de plantage | — |
| Traduction du HLSL compilé à la volée (`D3DCompile`) vers GLSL ; le DXBC précompilé est traduit depuis #354 | Warzone 2100 en ANGLE (Direct3D 11, OpenGL ES) | image noire : utiliser OpenGL ou Vulkan | #137 |
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
