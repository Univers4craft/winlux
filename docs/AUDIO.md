# Son des applications Windows dans Winlux

Les programmes `.exe` (jeux, lecteurs, outils de visio) jouent et enregistrent du
son par les API Windows habituelles. Tout est écrit en Rust dans le runtime
Winlux, sans Wine ni bibliothèque de compatibilité tierce.

## Architecture

```
waveOut*  DirectSound  WASAPI  XAudio2        (DLL intégrées du runtime)
     \         |         |        /
      +---- moteur de mixage (runtime/src/audio) ----+
            48 kHz stéréo f32, périodes de 10 ms
                         |
      PipeWire natif > PulseAudio > ALSA > sortie muette
```

- Un seul fil de mixage par processus (`winlux-audio-mix`). Chaque tampon,
  voix ou flux est une *source* que le moteur tire toutes les 10 ms. Le
  rééchantillonnage (linéaire) et la conversion de format (U8, S16, S24, S32,
  float, 1 à 8 canaux, WAVEFORMATEXTENSIBLE) se font dans la source.
- Le périphérique est libéré après 2 s sans source active et rouvert à la
  demande. Si le serveur son disparaît, le moteur bascule sur la sortie muette
  au lieu de bloquer le jeu.
- La capture (micro et *loopback*) passe par le même moteur.

## Choix du serveur son

Au lancement, le runtime regarde les sockets de `$XDG_RUNTIME_DIR`
(`pipewire-0`, `pulse/native`) puis charge la bibliothèque correspondante avec
`dlopen` : `libpipewire-0.3`, `libpulse-simple`, puis `libasound`. Aucune n'est
obligatoire ; sans aucune, le son est simplement muet.

La variable `WINLUX_AUDIO` impose un choix : `pipewire`, `pulse`, `alsa` ou
`null`.

Diagnostic :

```
winlux --audio-test
```

affiche les sorties candidates, celle retenue, et joue un bip de 440 Hz pendant
0,8 s.

## Volume par application

Chaque `.exe` ouvre un flux unique nommé d'après lui (`application.name`,
`application.process.binary`, `media.role=Game`). Il apparaît comme une
application séparée dans n'importe quel mélangeur PipeWire/PulseAudio, et dans
la section « Applications » de la popup volume du shell
(`scripts/winlux-popup.py`), avec un curseur par programme.

`IAudioEndpointVolume` et le volume du tampon primaire DirectSound règlent le
volume de ce flux, pas celui de tout le système.

## API prises en charge

| API | DLL | Contenu |
|-----|-----|---------|
| winmm | `winmm.dll` | `waveOut*` complet : ouverture avec rappel fonction, événement ou fenêtre, files de `WAVEHDR`, boucles `WHDR_BEGINLOOP/ENDLOOP`, pause, reset, position, volume, vitesse. `waveIn*` est traité à part. |
| DirectSound | `dsound.dll` | `DirectSoundCreate/8`, énumération, tampon primaire (format, volume, auditeur 3D), tampons secondaires (Lock/Unlock, lecture, boucle, volume, pan, fréquence, notifications, 3D de base, duplication), capture et notifications. |
| WASAPI | `mmdevapi.dll`, `audioses.dll`, `avrt.dll` | `IMMDeviceEnumerator`, `IMMDevice`, magasin de propriétés, `IAudioClient/2/3` en mode partagé et exclusif, rappels par événement, loopback, `IAudioRenderClient`, `IAudioCaptureClient`, `IAudioClock`, volumes de session et de flux, `IAudioEndpointVolume`. |
| XAudio2 | `xaudio2_7` à `xaudio2_9` | Moteur 2.7 (COM) et 2.8/2.9, voix mastering, submix et source, envois, matrices, filtres, rappels de voix et de moteur, boucles, `AudioReverb` et `AudioVolumeMeter`. |

`CoCreateInstance` et `CoGetClassObject` créent ces classes même si aucune
inscription n'existe dans le registre Winlux.

## Hors périmètre (fonctions avancées, non faites)

- Formats compressés ADPCM, xWMA et XMA dans XAudio2.
- X3DAudio, effet Doppler, cônes de son 3D, effets `xapofx` et XAPO tiers
  (ces derniers sont acceptés mais laissés transparents).
- Effets DirectSound (`SetFX` répond `DSERR_FXUNAVAILABLE`).
- Les *operation sets* XAudio2 sont appliqués tout de suite.
- L'ISO ne contient aujourd'hui qu'ALSA : ajouter PipeWire à l'image donnerait
  le volume par application dans le système installé.

## Tests

- `cargo test -p winlux` : tests unitaires du moteur, du format, du DSP et des
  quatre API (un test PipeWire/Pulse réel est marqué `#[ignore]`).
- `testexe/src/bin/audio_suite.rs` : un vrai `.exe` qui passe par waveOut,
  DirectSound, WASAPI, XAudio2 2.9 et 2.7. `scripts/test-runtime.sh` le lance
  avec `WINLUX_AUDIO=null`.
