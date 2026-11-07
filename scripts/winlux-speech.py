#!/usr/bin/python3
"""Dictée vocale et sous-titres en direct, entièrement sur ce PC (#662).

Windows + H ouvre une petite barre micro : ce qui est dit est tapé dans la
fenêtre active (session native ou programme .exe), ponctuation comprise.
Windows + Ctrl + L affiche une bande de sous-titres de ce qui sort des
haut-parleurs (et du micro si on le choisit). Le son ne quitte jamais le PC.

Le moteur est whisper.cpp (paquet Ubuntu, installé à la demande par le
Store) ; le modèle Whisper multilingue se télécharge depuis Paramètres ›
Accessibilité, son empreinte SHA-256 est vérifiée, rien n'est dans l'ISO.
La sortie audio (programmes natifs et .exe, qui passent tous par PipeWire)
est lue sur le moniteur de la sortie par défaut, avec parec en locale C.

    winlux-speech dictate             Windows + H (un second appui arrête)
    winlux-speech captions [start|stop]   Windows + Ctrl + L
    winlux-speech status              engine=0|1, model=…, mic=0|1, output=0|1…
    winlux-speech install [base|small]   moteur et modèle, empreinte vérifiée
    winlux-speech remove              retire les modèles et le moteur
    winlux-speech set CLÉ VALEUR      model, dictation_language, captions_language,
                                      captions_source, captions_position
    winlux-speech transcribe FICHIER [langue]   texte d'un enregistrement (tests)
"""
import array
import hashlib
import json
import math
import os
from pathlib import Path
import queue
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import wave


def _interface_lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="): code = line[5:].strip()
        except OSError:
            pass
    code = (code or os.environ.get("LANG", "fr"))[:2].lower()
    return code if code in ("fr", "en", "es", "de") else "fr"


_LANG = _interface_lang()
_COL = {"en": 0, "es": 1, "de": 2}
_TR = {
    'Dictée vocale': ('Voice typing', 'Dictado por voz', 'Spracheingabe'),
    'Écoute… parlez, le texte est tapé dans la fenêtre active.': (
        'Listening… speak and the text is typed into the active window.',
        'Escuchando… hable y el texto se escribe en la ventana activa.',
        'Hört zu… sprechen Sie, der Text wird in das aktive Fenster geschrieben.'),
    'Transcription…': ('Transcribing…', 'Transcribiendo…', 'Wird umgewandelt…'),
    'Aucun micro détecté.': ('No microphone detected.', 'No se detectó ningún micrófono.', 'Kein Mikrofon erkannt.'),
    'La reconnaissance vocale n’est pas encore installée.': (
        'Speech recognition is not installed yet.', 'El reconocimiento de voz aún no está instalado.',
        'Die Spracherkennung ist noch nicht installiert.'),
    'Télécharger (environ {size} Mo)': ('Download (about {size} MB)', 'Descargar (unos {size} MB)', 'Herunterladen (etwa {size} MB)'),
    'Téléchargement de la reconnaissance vocale…': ('Downloading speech recognition…', 'Descargando el reconocimiento de voz…',
                                                   'Spracherkennung wird heruntergeladen…'),
    'Téléchargement impossible pour l’instant.': ('Download not possible right now.', 'No se puede descargar ahora.',
                                                  'Download derzeit nicht möglich.'),
    'Le son ne quitte jamais ce PC.': ('Audio never leaves this PC.', 'El audio nunca sale de este PC.',
                                       'Der Ton verlässt diesen PC nie.'),
    'Sous-titres en direct': ('Live captions', 'Subtítulos en directo', 'Liveuntertitel'),
    'Prêt à afficher des sous-titres. Le son ne quitte jamais ce PC.': (
        'Ready to show captions. Audio never leaves this PC.', 'Listo para mostrar subtítulos. El audio nunca sale de este PC.',
        'Bereit für Untertitel. Der Ton verlässt diesen PC nie.'),
    'Aucune sortie audio à écouter.': ('No audio output to listen to.', 'No hay salida de audio que escuchar.',
                                       'Keine Audioausgabe zum Mithören.'),
    'Paramètres': ('Settings', 'Configuración', 'Einstellungen'),
    'Fermer': ('Close', 'Cerrar', 'Schließen'),
    'Arrêter': ('Stop', 'Detener', 'Beenden'),
    'Empreinte du modèle incorrecte : fichier refusé.': ('Model fingerprint mismatch: file rejected.',
                                                         'Huella del modelo incorrecta: archivo rechazado.',
                                                         'Prüfsumme des Modells falsch: Datei abgelehnt.'),
    'Moteur whisper.cpp introuvable et Store absent.': ('whisper.cpp engine not found and Store missing.',
                                                         'Motor whisper.cpp no encontrado y Store ausente.',
                                                         'whisper.cpp-Engine nicht gefunden und Store fehlt.'),
}


def _t(text):
    row = _TR.get(text)
    return row[_COL[_LANG]] if row and _LANG in _COL else text


# ── Modèles ────────────────────────────────────────────────────────────────
# Whisper (licence MIT) au format ggml de whisper.cpp, multilingue (fr, en,
# de, es et d'autres), quantifié q5_1. Révision du dépôt figée : un fichier
# changé en amont est refusé par son empreinte.
MODEL_REVISION = "5359861c739e955e79d9a303bcbc70fb988958b1"
MODEL_URL = "https://huggingface.co/ggerganov/whisper.cpp/resolve/{revision}/{file}"
MODELS = {
    "base": {"file": "ggml-base-q5_1.bin", "mb": 57, "bytes": 59707625,
             "sha256": "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898"},
    "small": {"file": "ggml-small-q5_1.bin", "mb": 181, "bytes": 190085487,
              "sha256": "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb"},
}
DEFAULT_MODEL = "base"
ENGINE_PACKAGES = ["whisper.cpp"]
LANGUAGES = ("auto", "fr", "en", "de", "es")


def data_dirs():
    """Modèles du compte d'abord, puis ceux de la machine (une image qui en embarquerait)."""
    override = os.environ.get("WINLUX_SPEECH_MODEL_DIR")
    if override: return [Path(override)]
    base = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    return [base / "winlux/speech", Path("/usr/share/winlux/speech")]


def config_path():
    return Path(os.environ.get("XDG_CONFIG_HOME") or Path.home() / ".config") / "winlux/speech.conf"


def runtime_dir():
    """Fichiers d'état du compte (pas /run/winlux partagé entre comptes, #578)."""
    base = os.environ.get("XDG_RUNTIME_DIR")
    path = Path(base) / "winlux" if base and os.access(base, os.W_OK) else Path.home() / ".cache/winlux"
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    return path


DEFAULTS = {"model": DEFAULT_MODEL, "dictation_language": _LANG, "captions_language": "auto",
            "captions_source": "output", "captions_position": "bottom"}
CHOICES = {"model": tuple(MODELS), "dictation_language": LANGUAGES, "captions_language": LANGUAGES,
           "captions_source": ("output", "both"), "captions_position": ("bottom", "top")}


def settings():
    values = dict(DEFAULTS)
    try:
        for line in config_path().read_text(encoding="utf-8").splitlines():
            key, _, value = line.partition("=")
            if key.strip() in CHOICES and value.strip() in CHOICES[key.strip()]:
                values[key.strip()] = value.strip()
    except (OSError, UnicodeDecodeError):
        pass
    return values


def save_setting(key, value):
    if key not in CHOICES or value not in CHOICES[key]: return False
    values = settings()
    values[key] = value
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(".tmp")
    temporary.write_text("".join(f"{k}={v}\n" for k, v in values.items() if v != DEFAULTS[k] or k == key), encoding="utf-8")
    temporary.replace(path)
    return True


def engine():
    return os.environ.get("WINLUX_SPEECH_ENGINE") or shutil.which("whisper-cli")


def model_file(name):
    info = MODELS[name]
    for folder in data_dirs():
        path = folder / info["file"]
        try:
            if path.stat().st_size == info["bytes"]: return path
        except OSError:
            continue
    return None


def installed_models():
    return [name for name in MODELS if model_file(name)]


def current_model():
    """Modèle choisi s'il est là, sinon le premier présent ; None sans modèle."""
    wanted = settings()["model"]
    if model_file(wanted): return model_file(wanted)
    present = installed_models()
    return model_file(present[0]) if present else None


# ── Son : PipeWire / PulseAudio par pactl et parec, en locale C ────────────

def _c_env():
    return dict(os.environ, LC_ALL="C", LANG="C")


def _pactl_json(*what):
    if not shutil.which("pactl"): return None
    try:
        out = subprocess.run(["pactl", "-f", "json", "list", *what], capture_output=True, text=True,
                             timeout=5, env=_c_env())
        return json.loads(out.stdout) if out.returncode == 0 else None
    except (OSError, subprocess.SubprocessError, ValueError):
        return None


def microphones(sources=None):
    """Vraies entrées son : ni moniteur d'une sortie, ni source factice."""
    sources = _pactl_json("sources") if sources is None else sources
    found = []
    for source in sources or []:
        name = source.get("name", "")
        if source.get("monitor_source") or source.get("monitor_of_sink") not in (None, "", "n/a") \
                or name.endswith(".monitor") or name in ("auto_null", "") or "null" in name.split("."):
            continue
        found.append(name)
    return found


def output_monitor():
    """Moniteur de la sortie par défaut (tout ce qui sort des haut-parleurs), ou None."""
    if not shutil.which("parec") or not shutil.which("pactl"): return None
    try:
        sink = subprocess.run(["pactl", "get-default-sink"], capture_output=True, text=True, timeout=5,
                              env=_c_env()).stdout.strip()
    except (OSError, subprocess.SubprocessError):
        return None
    return sink + ".monitor" if sink else None


# ── Découpage de la parole (silences) ──────────────────────────────────────
RATE = 16000
FRAME = RATE // 10          # 100 ms
FRAME_BYTES = FRAME * 2      # s16le mono


def rms(frame):
    samples = array.array("h")
    samples.frombytes(frame[:len(frame) - len(frame) % 2])
    if sys.byteorder == "big": samples.byteswap()
    if not samples: return 0.0
    return math.sqrt(sum(s * s for s in samples) / len(samples))


class Segmenter:
    """Coupe un flux 16 kHz mono en phrases : une phrase se termine après un
    silence (ou à la durée maximale) ; le bruit de fond est suivi en continu."""

    def __init__(self, silence_ms=700, max_ms=12000, min_speech_ms=300, floor=120.0):
        self.silence_frames = max(1, silence_ms // 100)
        self.max_frames = max(2, max_ms // 100)
        self.min_speech = max(1, min_speech_ms // 100)
        self.noise = floor
        self.floor = floor
        self.pre = []
        self.frames = []
        self.speech = 0
        self.quiet = 0
        self.level = 0.0

    def threshold(self):
        return max(400.0, self.noise * 3.0)

    def feed(self, frame):
        """Ajoute 100 ms ; rend les octets d'une phrase terminée, ou None."""
        self.level = rms(frame)
        loud = self.level > self.threshold()
        if not self.frames:
            if not loud:
                self.noise = max(self.floor, self.noise * 0.95 + self.level * 0.05)
                self.pre = (self.pre + [frame])[-3:]
                return None
            self.frames = self.pre + [frame]
            self.pre, self.speech, self.quiet = [], 1, 0
            return None
        self.frames.append(frame)
        if loud: self.speech, self.quiet = self.speech + 1, 0
        else: self.quiet += 1
        if self.quiet >= self.silence_frames or len(self.frames) >= self.max_frames:
            return self.flush()
        return None

    def flush(self):
        frames, speech = self.frames, self.speech
        self.frames, self.speech, self.quiet = [], 0, 0
        return b"".join(frames) if speech >= self.min_speech else None


# ── Reconnaissance ─────────────────────────────────────────────────────────
# Textes que Whisper invente sur du silence ou de la musique.
HALLUCINATIONS = re.compile(
    r"^(sous-titr(es|age).*|merci d'avoir regardé.*|thanks? (you )?for watching.*|untertitel (der|im auftrag).*|"
    r"subtítulos (realizados|por).*|amara\.org.*|\.+|…|you|merci\.?|thank you\.?|danke\.?|gracias\.?)$", re.I)


def clean_text(text):
    """Une ligne, sans annotations de bruit ([Musique], (rires), ♪…) ni texte inventé."""
    text = " ".join(line.strip() for line in text.splitlines() if line.strip())
    text = re.sub(r"\[[^\]]*\]|\([^)]*\)|\*[^*]*\*|♪+|♫+", " ", text)
    text = re.sub(r"\s+", " ", text).strip(" -")
    text = re.sub(r"\s+([,.;:!?…])", lambda m: m.group(1) if m.group(1) in ",." else " " + m.group(1), text)
    return "" if HALLUCINATIONS.match(text.strip().strip("!")) else text


def write_wav(path, pcm):
    with wave.open(str(path), "wb") as out:
        out.setnchannels(1)
        out.setsampwidth(2)
        out.setframerate(RATE)
        out.writeframes(pcm)


def read_wav_16k(path):
    """PCM 16 kHz mono d'un enregistrement (ffmpeg si le format diffère)."""
    try:
        with wave.open(str(path), "rb") as source:
            if source.getframerate() == RATE and source.getnchannels() == 1 and source.getsampwidth() == 2:
                return source.readframes(source.getnframes())
    except (wave.Error, EOFError, OSError):
        pass
    if not shutil.which("ffmpeg"): return None
    try:
        out = subprocess.run(["ffmpeg", "-v", "quiet", "-i", str(path), "-f", "s16le", "-ac", "1", "-ar", str(RATE), "-"],
                             capture_output=True, timeout=300)
    except (OSError, subprocess.SubprocessError):
        return None
    return out.stdout if out.returncode == 0 and out.stdout else None


def whisper_args(model, wav, language):
    threads = str(max(1, min(4, os.cpu_count() or 2)))
    return [engine(), "-m", str(model), "-f", str(wav), "-l", language if language in LANGUAGES else "auto",
            "-nt", "-np", "-t", threads]


def transcribe_pcm(pcm, language="auto", model=None):
    """Texte d'un extrait PCM ; chaîne vide si rien, None si le moteur manque ou échoue."""
    model = model or current_model()
    if not engine() or model is None: return None
    with tempfile.TemporaryDirectory(prefix="winlux-speech-") as folder:
        wav = Path(folder) / "extrait.wav"
        write_wav(wav, pcm)
        try:
            result = subprocess.run(whisper_args(model, wav, language), capture_output=True, text=True, timeout=300)
        except (OSError, subprocess.SubprocessError):
            return None
    return clean_text(result.stdout) if result.returncode == 0 else None


def transcribe_file(path, language="auto"):
    pcm = read_wav_16k(path)
    if pcm is None: return None
    texts = []
    # Par tranches de 25 s au plus, coupées aux silences comme en direct.
    cutter = Segmenter(silence_ms=500, max_ms=25000)
    for at in range(0, len(pcm), FRAME_BYTES):
        part = cutter.feed(pcm[at:at + FRAME_BYTES].ljust(FRAME_BYTES, b"\0"))
        if part:
            text = transcribe_pcm(part, language)
            if text is None: return None
            if text: texts.append(text)
    rest = cutter.flush()
    if rest:
        text = transcribe_pcm(rest, language)
        if text is None: return None
        if text: texts.append(text)
    return " ".join(texts)


# ── Commandes de saisie ────────────────────────────────────────────────────
NEW_LINE = re.compile(r"^(à la ligne|nouvelle ligne|retour à la ligne|new line|next line|neue zeile|nueva línea)[.!]?$", re.I)


def typing_commands(text, previous_ended):
    """Commandes xdotool pour taper une phrase dictée (espace entre deux phrases)."""
    if NEW_LINE.match(text.strip()):
        return [["xdotool", "key", "--clearmodifiers", "Return"]], False
    prefix = " " if previous_ended is True else ""
    return [["xdotool", "type", "--clearmodifiers", "--delay", "6", "--", prefix + text]], True


# ── Capture ────────────────────────────────────────────────────────────────

class Capture(threading.Thread):
    """Lit une source son (micro ou moniteur de sortie) par parec, ou un
    fichier WAV (WINLUX_SPEECH_INPUT, tests), et pousse les phrases dans `out`."""

    def __init__(self, device, segmenter, out, label=""):
        super().__init__(daemon=True)
        self.device, self.segmenter, self.out, self.label = device, segmenter, out, label
        self.process = None
        self.stopped = threading.Event()

    def frames(self):
        test_input = os.environ.get("WINLUX_SPEECH_INPUT")
        if test_input:
            pcm = read_wav_16k(test_input) or b""
            for at in range(0, len(pcm), FRAME_BYTES):
                if self.stopped.is_set(): return
                yield pcm[at:at + FRAME_BYTES].ljust(FRAME_BYTES, b"\0")
                time.sleep(0.02)
            for _ in range(20): yield b"\0" * FRAME_BYTES
            return
        args = ["parec", "--format=s16le", f"--rate={RATE}", "--channels=1", "--raw", "--latency-msec=100",
                "--client-name=winlux-speech"]
        if self.device: args += ["-d", self.device]
        try:
            self.process = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=_c_env())
        except OSError:
            return
        while not self.stopped.is_set():
            chunk = self.process.stdout.read(FRAME_BYTES)
            if not chunk or len(chunk) < FRAME_BYTES: return
            yield chunk

    def run(self):
        for frame in self.frames():
            part = self.segmenter.feed(frame)
            if part: self.out.put((self.label, part))
        rest = self.segmenter.flush()
        if rest: self.out.put((self.label, rest))
        self.out.put((self.label, None))

    def stop(self):
        self.stopped.set()
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()


# ── Instance unique (un second Windows + H arrête) ─────────────────────────

def pid_file(kind):
    return runtime_dir() / f"speech-{kind}.pid"


def running_pid(kind):
    try:
        pid = int(pid_file(kind).read_text().strip())
        os.kill(pid, 0)
        cmdline = Path(f"/proc/{pid}/cmdline").read_bytes()
        return pid if b"speech" in cmdline else None
    except (OSError, ValueError):
        return None


def stop_running(kind):
    pid = running_pid(kind)
    if pid is None: return False
    try: os.kill(pid, signal.SIGTERM)
    except OSError: return False
    return True


def claim(kind):
    path = pid_file(kind)
    temporary = path.with_suffix(".tmp")
    temporary.write_text(str(os.getpid()))
    temporary.replace(path)


def release(kind):
    try:
        if pid_file(kind).read_text().strip() == str(os.getpid()): pid_file(kind).unlink()
    except OSError:
        pass


# ── Installation ───────────────────────────────────────────────────────────

def download(name, progress=None):
    """Télécharge le modèle dans le dossier du compte ; empreinte vérifiée avant de le garder."""
    info = MODELS[name]
    folder = data_dirs()[0]
    folder.mkdir(parents=True, exist_ok=True)
    target = folder / info["file"]
    partial = target.with_suffix(".part")
    url = os.environ.get("WINLUX_SPEECH_MODEL_URL") or MODEL_URL
    url = url.format(revision=MODEL_REVISION, file=info["file"])
    digest = hashlib.sha256()
    done = 0
    try:
        if url.startswith("file://"):
            source = open(url[7:], "rb")
        else:
            import urllib.request
            source = urllib.request.urlopen(urllib.request.Request(url, headers={"User-Agent": "winlux-speech"}), timeout=60)
        with source, open(partial, "wb") as out:
            while True:
                chunk = source.read(1 << 20)
                if not chunk: break
                digest.update(chunk)
                out.write(chunk)
                done += len(chunk)
                if progress: progress(min(99, done * 100 // info["bytes"]))
    except OSError as error:
        partial.unlink(missing_ok=True)
        print(error, file=sys.stderr)
        return False
    if done != info["bytes"] or digest.hexdigest() != info["sha256"]:
        partial.unlink(missing_ok=True)
        print(_t("Empreinte du modèle incorrecte : fichier refusé."), file=sys.stderr)
        return False
    partial.replace(target)
    if progress: progress(100)
    return True


def install(name=None):
    name = name or settings()["model"]
    if name not in MODELS: return 2
    if not engine():
        store = shutil.which("winlux-store")
        if not store:
            print(_t("Moteur whisper.cpp introuvable et Store absent."), file=sys.stderr)
            return 1
        for package in ENGINE_PACKAGES:
            if subprocess.run([store, "install", "apt:" + package], stdout=subprocess.DEVNULL,
                              stderr=subprocess.DEVNULL, timeout=1800).returncode != 0:
                return 1
    if not model_file(name):
        if not download(name, lambda p: print(f"progress={p}", flush=True)): return 1
    save_setting("model", name)
    return 0 if engine() and model_file(name) else 1


def remove():
    stop_running("dictation")
    stop_running("captions")
    for info in MODELS.values():
        (data_dirs()[0] / info["file"]).unlink(missing_ok=True)
    store = shutil.which("winlux-store")
    failed = False
    if store and not os.environ.get("WINLUX_SPEECH_ENGINE"):
        for package in ENGINE_PACKAGES:
            failed |= subprocess.run([store, "remove", "apt:" + package], stdout=subprocess.DEVNULL,
                                     stderr=subprocess.DEVNULL, timeout=600).returncode != 0
    return 1 if failed else 0


def status():
    values = settings()
    model = current_model()
    name = next((n for n, i in MODELS.items() if model is not None and model.name == i["file"]), "none")
    print(f"engine={int(bool(engine()))}")
    print(f"model={name}")
    print(f"models={','.join(installed_models())}")
    print(f"mic={int(bool(microphones()))}")
    print(f"output={int(bool(output_monitor()))}")
    print(f"dictation={'on' if running_pid('dictation') else 'off'}")
    print(f"captions={'on' if running_pid('captions') else 'off'}")
    for key in ("dictation_language", "captions_language", "captions_source", "captions_position"):
        print(f"{key}={values[key]}")
    for key, info in MODELS.items():
        print(f"size_{key}={info['mb']}")
    return 0


# ── Fenêtres ───────────────────────────────────────────────────────────────

def _palette():
    try:
        theme = json.loads((Path.home() / ".config/winlux/appearance.json").read_text()).get("theme", "dark")
    except (OSError, ValueError, AttributeError):
        theme = "dark"
    if theme == "light":
        return {"bg": "rgba(249,249,251,0.97)", "fg": "#1b1b1f", "sub": "#5d5d66", "accent": "#6a3df0", "border": "rgba(0,0,0,0.12)"}
    return {"bg": "rgba(32,32,36,0.96)", "fg": "#ffffff", "sub": "#c8c8d0", "accent": "#b69cff", "border": "rgba(255,255,255,0.10)"}


def _gtk():
    import gi
    gi.require_version("Gtk", "3.0")
    gi.require_version("Gdk", "3.0")
    from gi.repository import Gdk, GLib, Gtk
    colors = _palette()
    css = f"""
    .winlux-speech {{ padding: 8px 14px; background: {colors['bg']}; border: 1px solid {colors['border']}; border-radius: 12px; }}
    .winlux-speech label {{ color: {colors['fg']}; }}
    .winlux-speech .sub {{ color: {colors['sub']}; font-size: 0.9em; }}
    .winlux-speech label.caption {{ font-size: 22px; }}
    .winlux-speech button {{ border-radius: 6px; padding: 2px 10px; }}
    .winlux-speech button.accent {{ background: {colors['accent']}; color: #ffffff; border: none; }}
    """
    provider = Gtk.CssProvider()
    provider.load_from_data(css.encode())
    Gtk.StyleContext.add_provider_for_screen(Gdk.Screen.get_default(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION + 1)
    return Gdk, GLib, Gtk, colors


def _floating(Gdk, Gtk, title):
    window = Gtk.Window(title=title)
    # Jamais le clavier : le texte dicté va dans la fenêtre où l'on écrivait.
    window.set_accept_focus(False)
    window.set_focus_on_map(False)
    window.set_keep_above(True)
    window.set_skip_taskbar_hint(True)
    window.set_skip_pager_hint(True)
    window.set_type_hint(Gdk.WindowTypeHint.UTILITY)
    window.set_decorated(False)
    window.stick()
    visual = window.get_screen().get_rgba_visual()
    if visual is not None: window.set_visual(visual)
    window.set_app_paintable(True)
    return window


def _workarea(Gdk):
    display = Gdk.Display.get_default()
    monitor = display.get_primary_monitor() or display.get_monitor(0)
    return monitor.get_workarea()


def _keep_placed(window, area, where):
    """Centrée en bas (au-dessus de la barre des tâches) ou en haut, replacée
    à chaque changement de taille (texte qui passe sur deux lignes…)."""
    def place(*_):
        width, height = window.get_size()
        x = area.x + (area.width - width) // 2
        y = area.y + 16 if where == "top" else area.y + area.height - height - 16
        if window.get_position() != (x, y): window.move(x, y)
    window.connect("size-allocate", place)
    place()


def _mic_icon(Gtk, colors, level_source):
    """Micro dessiné (pas d'icône externe), anneau violet qui suit le niveau."""
    area = Gtk.DrawingArea()
    area.set_size_request(40, 40)

    def hex_rgb(value):
        value = value.lstrip("#")
        return tuple(int(value[i:i + 2], 16) / 255 for i in (0, 2, 4))

    def draw(widget, cr):
        width, height = widget.get_allocated_width(), widget.get_allocated_height()
        cx, cy = width / 2, height / 2
        accent = hex_rgb(colors["accent"])
        level = min(1.0, level_source() / 3000.0)
        cr.set_source_rgba(*accent, 0.25 + 0.5 * level)
        cr.arc(cx, cy, 14 + 5 * level, 0, 2 * math.pi)
        cr.fill()
        cr.set_source_rgb(*accent)
        cr.arc(cx, cy, 14, 0, 2 * math.pi)
        cr.fill()
        cr.set_source_rgb(1, 1, 1)
        cr.set_line_width(2)
        # Capsule du micro, arceau et pied.
        cr.new_sub_path()
        cr.arc(cx, cy - 4, 3.2, math.pi, 0)
        cr.line_to(cx + 3.2, cy + 1)
        cr.arc(cx, cy + 1, 3.2, 0, math.pi)
        cr.close_path()
        cr.fill()
        cr.arc(cx, cy, 6.5, 0.15 * math.pi, 0.85 * math.pi)
        cr.stroke()
        cr.move_to(cx, cy + 6.5)
        cr.line_to(cx, cy + 9.5)
        cr.stroke()
    area.connect("draw", draw)
    return area


def _download_button(Gtk, GLib, status_label, on_done):
    size = MODELS[settings()["model"]]["mb"]
    button = Gtk.Button(label=_t("Télécharger (environ {size} Mo)").format(size=size))
    button.get_style_context().add_class("accent")

    def start(_):
        button.set_sensitive(False)
        status_label.set_text(_t("Téléchargement de la reconnaissance vocale…"))
        try:
            process = subprocess.Popen([sys.executable, os.path.abspath(__file__), "install"],
                                       stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        except OSError:
            status_label.set_text(_t("Téléchargement impossible pour l’instant."))
            return

        def wait():
            if process.poll() is None: return True
            button.set_sensitive(True)
            if process.returncode == 0 and engine() and current_model():
                button.hide()
                on_done()
            else:
                status_label.set_text(_t("Téléchargement impossible pour l’instant."))
            return False
        GLib.timeout_add(500, wait)
    button.connect("clicked", start)
    return button


def dictate():
    """Windows + H : barre micro ; un second appui l'arrête."""
    if stop_running("dictation"): return 0
    Gdk, GLib, Gtk, colors = _gtk()
    claim("dictation")
    window = _floating(Gdk, Gtk, _t("Dictée vocale"))
    box = Gtk.Box(spacing=12, margin=10)
    box.get_style_context().add_class("winlux-speech")
    window.add(box)
    phrases = queue.Queue()
    state = {"capture": None, "ended": None, "busy": 0}
    segmenter = Segmenter(silence_ms=700, max_ms=12000)
    box.pack_start(_mic_icon(Gtk, colors, lambda: segmenter.level if state["capture"] else 0), False, False, 0)
    texts = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2, valign=Gtk.Align.CENTER)
    title = Gtk.Label(label=_t("Dictée vocale"), xalign=0)
    status_label = Gtk.Label(label="", xalign=0, wrap=True, max_width_chars=48)
    status_label.get_style_context().add_class("sub")
    texts.pack_start(title, False, False, 0)
    texts.pack_start(status_label, False, False, 0)
    box.pack_start(texts, True, True, 0)
    close = Gtk.Button(label="✕", tooltip_text=_t("Fermer"), valign=Gtk.Align.CENTER)
    close.set_relief(Gtk.ReliefStyle.NONE)

    def listen():
        status_label.set_text(_t("Écoute… parlez, le texte est tapé dans la fenêtre active."))
        mics = microphones()
        if not mics and not os.environ.get("WINLUX_SPEECH_INPUT"):
            status_label.set_text(_t("Aucun micro détecté."))
            GLib.timeout_add_seconds(3, lambda: window.destroy())
            return
        state["capture"] = Capture("@DEFAULT_SOURCE@", segmenter, phrases)
        state["capture"].start()
        threading.Thread(target=worker, daemon=True).start()

    def worker():
        language = settings()["dictation_language"]
        while True:
            _, pcm = phrases.get()
            if pcm is None:
                if os.environ.get("WINLUX_SPEECH_INPUT"): GLib.idle_add(window.destroy)
                return
            GLib.idle_add(status_label.set_text, _t("Transcription…"))
            text = transcribe_pcm(pcm, language)
            GLib.idle_add(status_label.set_text, _t("Écoute… parlez, le texte est tapé dans la fenêtre active."))
            if text: GLib.idle_add(type_text, text)

    def type_text(text):
        commands, state["ended"] = typing_commands(text, state["ended"])
        log = os.environ.get("WINLUX_SPEECH_TYPED")
        for command in commands:
            if log:
                with open(log, "a", encoding="utf-8") as out: out.write(command[-1] + "\n")
                continue
            try: subprocess.run(command, timeout=30, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            except (OSError, subprocess.SubprocessError): pass
        return False

    def finish(*_):
        if state["capture"] is not None: state["capture"].stop()
        release("dictation")
        Gtk.main_quit()

    close.connect("clicked", lambda _: window.destroy())
    box.pack_end(close, False, False, 0)
    if engine() and current_model():
        GLib.idle_add(listen)
    else:
        status_label.set_text(_t("La reconnaissance vocale n’est pas encore installée."))
        box.pack_end(_download_button(Gtk, GLib, status_label, listen), False, False, 0)
    GLib.timeout_add(100, lambda: (window.queue_draw(), True)[1])
    window.connect("destroy", finish)
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, lambda: window.destroy() or False)
    area = _workarea(Gdk)
    window.set_default_size(min(520, area.width - 32), -1)
    window.show_all()
    _keep_placed(window, area, "bottom")
    Gtk.main()
    return 0


def captions(action="toggle"):
    """Windows + Ctrl + L : bande de sous-titres de la sortie audio."""
    if action in ("toggle", "stop") and stop_running("captions"): return 0
    if action == "stop": return 0
    if running_pid("captions"): return 0
    Gdk, GLib, Gtk, colors = _gtk()
    claim("captions")
    values = settings()
    window = _floating(Gdk, Gtk, _t("Sous-titres en direct"))
    window.set_title(_t("Sous-titres en direct"))
    outer = Gtk.Box(spacing=8, margin=12)
    outer.get_style_context().add_class("winlux-speech")
    window.add(outer)
    text_label = Gtk.Label(label="", xalign=0, wrap=True, lines=2)
    text_label.get_style_context().add_class("caption")
    text_label.set_valign(Gtk.Align.CENTER)
    outer.pack_start(text_label, True, True, 6)
    tools = Gtk.Box(spacing=2, valign=Gtk.Align.START)
    gear = Gtk.Button(label="⚙", tooltip_text=_t("Paramètres"))
    gear.set_relief(Gtk.ReliefStyle.NONE)
    gear.connect("clicked", lambda _: subprocess.Popen(["winlux-settings", "accessibility"], start_new_session=True,
                                                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                 if shutil.which("winlux-settings") else None)
    close = Gtk.Button(label="✕", tooltip_text=_t("Arrêter"))
    close.set_relief(Gtk.ReliefStyle.NONE)
    close.connect("clicked", lambda _: window.destroy())
    tools.pack_start(gear, False, False, 0)
    tools.pack_start(close, False, False, 0)
    outer.pack_end(tools, False, False, 0)
    phrases = queue.Queue()
    captures = []
    shown = []

    def show(text):
        shown.append(text)
        line = " ".join(shown)[-220:]
        text_label.set_text(line[line.find(" ") + 1:] if len(" ".join(shown)) > 220 else line)
        del shown[:-4]
        log = os.environ.get("WINLUX_SPEECH_TYPED")
        if log:
            with open(log, "a", encoding="utf-8") as out: out.write(text + "\n")
        return False

    def worker():
        ended = 0
        while True:
            label, pcm = phrases.get()
            if pcm is None:
                ended += 1
                if ended >= len(captures):
                    if os.environ.get("WINLUX_SPEECH_INPUT"): GLib.idle_add(window.destroy)
                    return
                continue
            # Rester en direct : une file trop longue perd les plus anciens extraits.
            while phrases.qsize() > 2:
                skipped = phrases.get()
                if skipped[1] is None:
                    phrases.put(skipped)
                    break
            text = transcribe_pcm(pcm, values["captions_language"])
            if text: GLib.idle_add(show, ("🎤 " if label == "mic" else "") + text)

    def start():
        if not (engine() and current_model()):
            text_label.set_text(_t("La reconnaissance vocale n’est pas encore installée."))
            button = _download_button(Gtk, GLib, text_label, start)
            outer.pack_start(button, False, False, 0)
            button.show()
            return False
        monitor = output_monitor()
        if monitor is None and not os.environ.get("WINLUX_SPEECH_INPUT"):
            text_label.set_text(_t("Aucune sortie audio à écouter."))
            return False
        text_label.set_text(_t("Prêt à afficher des sous-titres. Le son ne quitte jamais ce PC."))
        captures.append(Capture(monitor, Segmenter(silence_ms=500, max_ms=6000, min_speech_ms=400), phrases, "output"))
        if values["captions_source"] == "both" and microphones() and not os.environ.get("WINLUX_SPEECH_INPUT"):
            captures.append(Capture("@DEFAULT_SOURCE@", Segmenter(silence_ms=500, max_ms=6000), phrases, "mic"))
        for capture in captures: capture.start()
        threading.Thread(target=worker, daemon=True).start()
        return False

    def finish(*_):
        for capture in captures: capture.stop()
        release("captions")
        Gtk.main_quit()

    window.connect("destroy", finish)
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, lambda: window.destroy() or False)
    area = _workarea(Gdk)
    width = min(area.width - 48, 1100)
    # Largeur fixée pour que la hauteur soit celle de deux lignes à cette largeur.
    text_label.set_size_request(width - 140, -1)
    window.set_default_size(width, -1)
    window.show_all()
    _keep_placed(window, area, values["captions_position"])
    GLib.idle_add(start)
    Gtk.main()
    return 0


def linux_path(value):
    if len(value) >= 2 and value[1] == ":" and value[0].isalpha():
        rest = value[2:].replace("\\", "/").lstrip("/")
        if value[0].upper() == "Z": return "/" + rest
        if value[0].upper() == "C": return str(Path.home() / rest)
    return value


def main(argv):
    args = argv[1:]
    if args == ["status"]: return status()
    if args and args[0] == "install" and len(args) <= 2: return install(args[1] if len(args) == 2 else None)
    if args == ["remove"]: return remove()
    if len(args) == 3 and args[0] == "set": return 0 if save_setting(args[1], args[2]) else 2
    if args == ["dictate"]: return dictate()
    if args and args[0] == "captions" and len(args) <= 2:
        return captions(args[1] if len(args) == 2 else "toggle")
    if args and args[0] == "transcribe" and len(args) in (2, 3):
        language = args[2] if len(args) == 3 else "auto"
        text = transcribe_file(linux_path(args[1]), language)
        if text is None: return 1
        print(text)
        return 0
    print(__doc__.strip().split("\n\n")[3], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
