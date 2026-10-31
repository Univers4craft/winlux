# Lecteur multimédia et moteur vidéo/audio

Winlux lit la vidéo et l'audio avec un lecteur natif du shell
(`mediaplayer.exe`, style Windows 11) et un moteur multimédia intégré au
runtime (`runtime/src/dlls/winlux_media.rs`). Aucune DLL Windows ni Wine.

## Ce qui existait avant

- `audioplayer.exe` : maquette d'interface, sans décodage ni son.
- `PlaySound` : WAV, MP3 (mpg123) et AAC ADTS (FAAD) vers ALSA
  (voir [AUDIO-CODECS.md](AUDIO-CODECS.md)), toujours en place.
- `mfplat`/`quartz` : bouchons qui répondent `E_NOTIMPL`.

`audioplayer.exe` est désormais un alias du nouveau lecteur.

## Architecture

```
Explorer / menu Démarrer / xdg-open
        │  winlux-mediaplayer <fichier>
        ▼
mediaplayer.exe (PE, interface)  ──WinluxMedia*──▶  runtime winlux_media.rs
                                                       │ processus isolés
                                                       ▼
                                         ffprobe (sonde) · ffmpeg (décodage)
```

Le décodage est confié à FFmpeg, le moteur de codecs du système Linux, lancé
comme processus séparé (même logique que mpg123/FAAD pour `PlaySound`) :

- tous les conteneurs et codecs courants d'un coup, et le décodage matériel ;
- isolation : un fichier piégé ou un décodeur qui plante ne fait tomber que
  le processus ffmpeg, jamais le shell ; le processus meurt avec le runtime
  (`PR_SET_PDEATHSIG`) ;
- pas d'ABI C fragile à suivre entre versions de libavcodec.

Le runtime cherche `ffmpeg`/`ffprobe` dans `WINLUX_MEDIA_DIR`, puis
`/usr/lib/winlux/media` (copie embarquée dans l'ISO par
`scripts/stage-media.sh`), puis le `PATH`.

## Formats

Conteneurs : MP4/M4V, MKV/MKA, WebM, AVI, MOV, FLV, WMV/ASF, MPEG/MPG/VOB,
TS/M2TS/MTS, OGV/OGG, 3GP, MXF, RM, DV… et côté audio MP3, AAC/M4A, FLAC,
OGG/Opus, WAV, WMA, APE, AC-3, DTS, WavPack, AIFF, CAF, AMR…

Codecs vidéo suivis : H.264, H.265/HEVC, AV1, VP8, VP9, MPEG-2, MPEG-4 ASP
(DivX/Xvid), VC-1/WMV, Theora, ProRes, DNxHD/DNxHR, H.263, MJPEG.

Codecs audio suivis : AAC, MP3, Opus, Vorbis, FLAC, ALAC, PCM, AC-3, E-AC-3,
DTS, TrueHD, WMA, APE, WavPack, AMR.

Garde-fou : un format n'est annoncé lisible que si son décodeur figure dans
`ffmpeg -decoders` sur la machine. Sinon le lecteur affiche « Format non pris
en charge » avec le nom du décodeur manquant, sans rien lancer. L'écran
d'accueil du lecteur liste les formats réellement décodables.

## Décodage matériel

Politique `WINLUX_MEDIA_HWACCEL` : `auto` (défaut), `cuda`/`nvdec`, `vaapi`,
`vdpau`, `off`.

En `auto` : NVDEC (CUDA) si un GPU NVIDIA est présent (`/dev/nvidia0`),
sinon VA-API sur le premier `/dev/dri/renderD*`, sinon VDPAU, sinon logiciel.
Le décodage matériel n'est tenté que si FFmpeg le propose (`-hwaccels`) et
qu'un périphérique existe. Replis successifs si un essai échoue avant la
première image : matériel → logiciel → logiciel sans son (sortie audio
indisponible). Le lecteur affiche « GPU » quand le matériel est actif.

Les pilotes VA-API de la machine de construction sont copiés dans l'ISO.
Les applications `.exe` qui passent par DXVA2/Media Foundation ne sont pas
encore branchées sur ce moteur (voir Suivi.md).

## Sortie audio

`WINLUX_MEDIA_AUDIO=<format>:<périphérique>` (ex. `alsa:default`,
`pulse:default`, `none`). Par défaut : PulseAudio/PipeWire si son socket
existe, sinon ALSA `default`.

## Lecteur

- Lecture fluide (images BGRA décodées à la taille d'affichage), barre de
  progression avec aperçu de l'image et du chapitre au survol.
- Volume (molette, flèches), muet, vitesse 0,25× à 3× (son corrigé par
  `atempo`), plein écran (double-clic, F, F11, Échap).
- Sous-titres : fichiers à côté de la vidéo (`film.srt`, `film.fr.ass`,
  `.vtt`, `.ssa`, `.sub`, `.sup`) et pistes intégrées, texte (incrustées par
  libass) ou image (PGS, VobSub, DVB). Touche S pour passer à la suivante.
- Pistes audio (langue, titre, 5.1/7.1), touche A. Chapitres : repères sur la
  barre et menu.
- Liste de lecture : fichiers audio/vidéo du dossier, tri naturel, passage
  automatique au suivant.
- Reprise : la position est mémorisée toutes les 5 s et à la fermeture dans
  `~/.local/share/winlux/media-resume.tsv` (effacée en fin de lecture).
- Thème clair/sombre et langue (FR/EN/DE/ES) suivent `control.txt` du bureau.

## Vignettes

`WinluxMediaThumbnail(chemin, taille, sortie)` : vignette `taille²` BGRA
(contenu centré, fond transparent) de l'image à 10 % de la durée (60 s au
plus), ou de la pochette d'un fichier audio. Génération asynchrone par deux
fils : 1 = prête, 0 = en cours, −1 = impossible (afficher l'icône). Cache
disque `~/.cache/winlux/thumbnails/media-<hachage>-<l>x<h>.bgra`, clé chemin +
taille + date de modification. `WinluxMediaPreview` sert les aperçus de survol
(cache mémoire seulement).

## Exports du runtime

| Fonction | Rôle |
| --- | --- |
| `WinluxMediaProbe` | Description tabulée (durée, pistes, chapitres, lisibilité) |
| `WinluxMediaFormats` | Formats et accélération disponibles |
| `WinluxMediaOpen` / `State` / `Frame` / `Error` / `Close` | Session de lecture |
| `WinluxMediaThumbnail` / `Preview` | Vignettes et aperçus |
| `WinluxMediaGetResume` / `SetResume` | Reprise de lecture |
| `WinluxMediaSiblings` / `Subtitles` / `Canonical` / `Kind` | Liste de lecture, sous-titres, chemins |

## Vérification

```sh
cargo test -p winlux winlux_media                       # tests unitaires
cargo test -p winlux media_bout_en_bout -- --ignored    # FFmpeg réel (CI)
```

Le test de bout en bout génère une vidéo, la sonde, en tire une vignette et la
lit à 2× sans sortie audio. Le lecteur a été vérifié sous Xvfb (vidéo H.264 +
2 pistes AAC + chapitres + sous-titres externes, MP3 avec étiquettes).

## Licences

FFmpeg est LGPL ou GPL selon sa compilation ; les binaires de la distribution
gardent leurs licences, et `stage-media.sh` copie leurs notices dans l'ISO.
Distribuer l'ISO impose de fournir les sources correspondantes ou une offre
conforme, comme pour mpg123 et FAAD.
