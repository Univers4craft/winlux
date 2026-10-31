#!/usr/bin/python3
"""Sons système Winlux, synthétisés ici (aucun échantillon tiers).

Tous reprennent le timbre de la signature « Prisme » (cloches sur une nappe
douce, gamme de do majeur : do, sol, ré). Relancer après une retouche :

    python3 scripts/render-sounds.py

Les WAV (PCM 16 bits mono, 22 050 Hz) sont versionnés dans assets/sounds/ et
installés dans /usr/share/winlux/sounds/. Le runtime les joue pour
MessageBeep et PlaySound(SND_ALIAS), la session pour les notifications.
"""
from pathlib import Path
import wave

import numpy as np

RATE = 22050
OUT = Path(__file__).resolve().parent.parent / "assets" / "sounds"

C4, D4, E4, G4 = 261.63, 293.66, 329.63, 392.00
C5, D5, E5, G5, A5, B5 = 523.25, 587.33, 659.25, 783.99, 880.00, 987.77
C6, D6, G6 = 1046.50, 1174.66, 1567.98
F4, AB4, EB5 = 349.23, 415.30, 622.25


def bell(freq, start, length, amp=0.3):
    """Cloche : fondamentale et partiel inharmonique, attaque brève."""
    n = int(length * RATE)
    t = np.arange(n) / RATE
    env = np.exp(-t * 5.5 / length) * np.minimum(1, t / 0.004)
    tone = np.sin(2 * np.pi * freq * t) * 0.72 + np.sin(2 * np.pi * freq * 2.76 * t) * 0.28 * np.exp(-t * 9)
    return int(start * RATE), amp * env * tone


def pad(freq, start, length, amp=0.18):
    """Nappe : montée lente, deux voix légèrement désaccordées."""
    n = int(length * RATE)
    t = np.arange(n) / RATE
    env = np.minimum(1, t / 0.08) * np.minimum(1, (length - t) / (length * 0.6))
    tone = np.sin(2 * np.pi * freq * t) + np.sin(2 * np.pi * freq * 1.003 * t)
    return int(start * RATE), amp * 0.5 * env * tone


def render(name, notes):
    end = max(s + len(x) for s, x in notes)
    mix = np.zeros(end + RATE // 20)
    for s, x in notes:
        mix[s:s + len(x)] += x
    mix /= max(1.0, np.abs(mix).max() / 0.7)
    data = (mix * 32767).astype("<i2").tobytes()
    with wave.open(str(OUT / f"{name}.wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(data)


SOUNDS = {
    # Notification : deux cloches qui montent (quinte), discrètes.
    "notification": [bell(G5, 0, 0.6), bell(D6, 0.09, 0.7, 0.26)],
    # Information (MessageBeep MB_ICONASTERISK) : une cloche claire.
    "information": [bell(E5, 0, 0.7), pad(C5, 0, 0.5, 0.08)],
    # Question : deux notes, la seconde plus haute et suspendue.
    "question": [bell(C5, 0, 0.45), bell(D5, 0.14, 0.7)],
    # Avertissement : deux notes descendantes, plus graves.
    "avertissement": [bell(A5, 0, 0.5), bell(E5, 0.12, 0.8), pad(E4, 0, 0.6, 0.1)],
    # Erreur : accord mineur grave, court.
    "erreur": [bell(EB5, 0, 0.55, 0.28), bell(C5, 0, 0.55, 0.22), pad(C4, 0, 0.45, 0.16), pad(AB4, 0.02, 0.45, 0.1)],
    # Son par défaut (MessageBeep 0 / MB_OK) : une note douce.
    "defaut": [bell(C6, 0, 0.5, 0.24), pad(G4, 0, 0.35, 0.06)],
    # Appareil branché / retiré : arpège montant / descendant.
    "appareil-branche": [bell(C5, 0, 0.4), bell(G5, 0.08, 0.45), bell(C6, 0.16, 0.6)],
    "appareil-retire": [bell(C6, 0, 0.4), bell(G5, 0.08, 0.45), bell(C5, 0.16, 0.6)],
    # Ouverture et fermeture de session : la signature, raccourcie.
    "connexion": [pad(C4, 0, 1.1), bell(C5, 0.05, 0.9), bell(G5, 0.17, 0.9), bell(D6, 0.29, 1.0)],
    "deconnexion": [pad(C4, 0, 1.0), bell(D6, 0, 0.8), bell(G5, 0.12, 0.8), bell(C5, 0.24, 1.0)],
}


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    for name, notes in SOUNDS.items():
        render(name, notes)
    print(f"{len(SOUNDS)} sons système rendus dans {OUT}")


if __name__ == "__main__":
    main()
