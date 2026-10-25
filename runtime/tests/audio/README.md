# Fixtures audio générées

Sinus 440 Hz de 0,12 seconde, créés avec FFmpeg ; aucune musique tierce.
`tone` : 44100 Hz stéréo ; `mono` : 22050 Hz mono.

Exemple (adapter le taux, le nombre de canaux et le nom pour `mono`) :

```sh
ffmpeg -f lavfi -i sine=frequency=440:sample_rate=44100:duration=0.12 -ac 2 -c:a libmp3lame -b:a 96k -map_metadata -1 tone.mp3
ffmpeg -f lavfi -i sine=frequency=440:sample_rate=44100:duration=0.12 -ac 2 -c:a aac -b:a 96k -f adts -map_metadata -1 tone.aac
```
