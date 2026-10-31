#!/usr/bin/python3
"""Profils d'utilisation Winlux : Gaming, Bureautique, Développement, Serveur.

Chaque profil règle le noyau (sysctl), le gouverneur CPU, l'ordonnanceur d'E/S
par type de disque et le niveau de performance GPU, et propose des applications.
L'installateur écrit /etc/winlux/profile.json ; `/init` appelle
`winlux-profile --apply-boot` à chaque démarrage (pas de systemd-sysctl ici).
"""
import argparse
import json
import os
import sys


# ── Langue : celle choisie dans l'installateur (WINLUX_LANG), sinon celle du système ──

_COL = {"en": 0, "de": 1, "es": 2, "it": 3, "pt": 4}

# Texte français -> (anglais, allemand, espagnol, italien, portugais).
_TR = {
    'Bureautique': ('Office', 'Büro', 'Ofimática', 'Ufficio', 'Escritório'),
    'Développement': ('Development', 'Entwicklung', 'Desarrollo', 'Sviluppo', 'Desenvolvimento'),
    'Serveur': ('Server', 'Server', 'Servidor', 'Server', 'Servidor'),
    'Latence minimale et processeur à pleine fréquence pour les jeux .exe et natifs.': ('Minimal latency and full-speed processor for .exe and native games.', 'Minimale Latenz und Prozessor mit voller Taktfrequenz für .exe- und native Spiele.', 'Latencia mínima y procesador a máxima frecuencia para juegos .exe y nativos.', 'Latenza minima e processore alla massima frequenza per i giochi .exe e nativi.', 'Latência mínima e processador em frequência máxima para jogos .exe e nativos.'),
    "Bureau réactif et économie d'énergie pour les documents, le web et les mails.": ('Responsive desktop and power saving for documents, the web and email.', 'Reaktionsschneller Desktop und Energiesparen für Dokumente, Web und E-Mail.', 'Escritorio ágil y ahorro de energía para documentos, la web y el correo.', 'Desktop reattivo e risparmio energetico per documenti, web e posta.', 'Área de trabalho responsiva e economia de energia para documentos, web e e-mail.'),
    'Compilation rapide, nombreux fichiers surveillés et outils pour créer des .exe.': ('Fast builds, many watched files and tools to create .exe files.', 'Schnelles Kompilieren, viele überwachte Dateien und Werkzeuge zum Erstellen von .exe-Dateien.', 'Compilación rápida, muchos archivos vigilados y herramientas para crear .exe.', 'Compilazione rapida, molti file monitorati e strumenti per creare .exe.', 'Compilação rápida, muitos arquivos monitorados e ferramentas para criar .exe.'),
    'Débit réseau et disque prioritaires, services en arrière-plan sans bureau chargé.': ('Network and disk throughput first, background services without a heavy desktop.', 'Netzwerk- und Datenträgerdurchsatz zuerst, Hintergrunddienste ohne schweren Desktop.', 'Prioridad al rendimiento de red y disco, servicios en segundo plano sin escritorio pesado.', 'Priorità a rete e disco, servizi in background senza desktop pesante.', 'Prioridade à rede e ao disco, serviços em segundo plano sem área de trabalho pesada.'),
    'swappiness {swappiness} · CPU {governor} · E/S {io} · GPU {gpu}': ('swappiness {swappiness} · CPU {governor} · I/O {io} · GPU {gpu}', 'swappiness {swappiness} · CPU {governor} · E/A {io} · GPU {gpu}', 'swappiness {swappiness} · CPU {governor} · E/S {io} · GPU {gpu}', 'swappiness {swappiness} · CPU {governor} · I/O {io} · GPU {gpu}', 'swappiness {swappiness} · CPU {governor} · E/S {io} · GPU {gpu}'),
    'RAM < 4 Go : swappiness relevée à 60 pour éviter les blocages.': ('RAM < 4 GB: swappiness raised to 60 to avoid freezes.', 'RAM < 4 GB: swappiness auf 60 erhöht, um Hänger zu vermeiden.', 'RAM < 4 GB: swappiness elevada a 60 para evitar bloqueos.', 'RAM < 4 GB: swappiness portata a 60 per evitare blocchi.', 'RAM < 4 GB: swappiness elevada para 60 para evitar travamentos.'),
    'Aucun profil configuré.': ('No profile configured.', 'Kein Profil konfiguriert.', 'Ningún perfil configurado.', 'Nessun profilo configurato.', 'Nenhum perfil configurado.'),
}


def _lang():
    code = os.environ.get("WINLUX_LANG", "")
    if not code:
        try:
            with open("/etc/winlux/locale.conf", encoding="utf-8") as conf:
                for line in conf:
                    if line.startswith("LANG="):
                        code = line[5:].strip()
        except OSError:
            pass
    return (code or "fr")[:2].lower()


def _t(text):
    """Texte dans la langue de l'installateur ou du système (le français sert de clé)."""
    row = _TR.get(text)
    col = _COL.get(_lang())
    return row[col] if row and col is not None else text

GiB = 1024 ** 3

PROFILES = {
    "gaming": {
        "name": "Gaming",
        "description": "Latence minimale et processeur à pleine fréquence pour les jeux .exe et natifs.",
        "sysctl": {
            "vm.swappiness": 10,
            "vm.vfs_cache_pressure": 50,
            "vm.max_map_count": 2147483642,
            "kernel.sched_autogroup_enabled": 1,
            "kernel.split_lock_mitigate": 0,
        },
        "governor": ["performance", "schedutil"],
        "io": {"nvme": "none", "ssd": "mq-deadline", "hdd": "bfq"},
        "gpu_level": "high",
        "apps": ["gamemode", "mangohud", "mesa-vulkan-drivers", "vulkan-tools"],
    },
    "office": {
        "name": "Bureautique",
        "description": "Bureau réactif et économie d'énergie pour les documents, le web et les mails.",
        "sysctl": {
            "vm.swappiness": 30,
            "kernel.sched_autogroup_enabled": 1,
        },
        "governor": ["schedutil", "powersave"],
        "io": {"nvme": "none", "ssd": "bfq", "hdd": "bfq"},
        "gpu_level": "auto",
        "apps": ["libreoffice", "evince", "gnome-calculator", "simple-scan"],
    },
    "dev": {
        "name": "Développement",
        "description": "Compilation rapide, nombreux fichiers surveillés et outils pour créer des .exe.",
        "sysctl": {
            "vm.swappiness": 20,
            "vm.max_map_count": 1048576,
            "fs.inotify.max_user_watches": 524288,
            "fs.inotify.max_user_instances": 1024,
            "kernel.sched_autogroup_enabled": 1,
        },
        "governor": ["schedutil", "performance"],
        "io": {"nvme": "none", "ssd": "mq-deadline", "hdd": "bfq"},
        "gpu_level": "auto",
        "apps": ["git", "build-essential", "gdb", "cmake", "python3-venv", "gcc-mingw-w64-x86-64"],
    },
    "server": {
        "name": "Serveur",
        "description": "Débit réseau et disque prioritaires, services en arrière-plan sans bureau chargé.",
        "sysctl": {
            "vm.swappiness": 10,
            "vm.dirty_background_ratio": 5,
            "kernel.sched_autogroup_enabled": 0,
            "net.core.somaxconn": 4096,
            "net.ipv4.tcp_fastopen": 3,
        },
        "governor": ["performance", "schedutil"],
        "io": {"nvme": "none", "ssd": "mq-deadline", "hdd": "mq-deadline"},
        "gpu_level": "low",
        "apps": ["openssh-server", "ufw", "fail2ban", "htop", "tmux"],
    },
}

ORDER = ["gaming", "office", "dev", "server"]


def summary_line(profile_id):
    p = PROFILES[profile_id]
    io = "/".join(dict.fromkeys(p["io"].values()))
    return _t("swappiness {swappiness} · CPU {governor} · E/S {io} · GPU {gpu}").format(
        swappiness=p["sysctl"]["vm.swappiness"], governor=p["governor"][0], io=io, gpu=p["gpu_level"])


def display_name(profile_id):
    """Nom du profil dans la langue de l'interface."""
    return _t(PROFILES[profile_id]["name"])


def display_description(profile_id):
    """Description du profil dans la langue de l'interface."""
    return _t(PROFILES[profile_id]["description"])


def build_config(profile_id, benchmark=None, install_apps=True):
    """Profil effectif, ajusté au matériel mesuré par le benchmark."""
    base = PROFILES[profile_id]
    sysctl = dict(base["sysctl"])
    notes = []
    ram = ((benchmark or {}).get("results") or {}).get("ram", {}).get("total_bytes", 0)
    if ram and ram < 4 * GiB:
        sysctl["vm.swappiness"] = max(sysctl["vm.swappiness"], 60)
        notes.append(_t("RAM < 4 Go : swappiness relevée à 60 pour éviter les blocages."))
    return {
        "id": profile_id,
        "name": base["name"],
        "sysctl": sysctl,
        "governor": list(base["governor"]),
        "io": dict(base["io"]),
        "gpu_level": base["gpu_level"],
        "apps": list(base["apps"]) if install_apps else [],
        "notes": notes,
    }


def sysctl_text(config):
    lines = [f"# Profil Winlux « {config['name']} » — généré par l'installateur"]
    lines += [f"{key} = {value}" for key, value in config["sysctl"].items()]
    return "\n".join(lines) + "\n"


def write_config(root, config, benchmark=None):
    """Écrit le profil dans le système cible monté sur `root`."""
    etc = os.path.join(root, "etc")
    os.makedirs(os.path.join(etc, "winlux"), exist_ok=True)
    os.makedirs(os.path.join(etc, "sysctl.d"), exist_ok=True)
    with open(os.path.join(etc, "winlux/profile.json"), "w") as f:
        json.dump(config, f, ensure_ascii=False, indent=2)
    with open(os.path.join(etc, "sysctl.d/90-winlux-profile.conf"), "w") as f:
        f.write(sysctl_text(config))
    apps = os.path.join(etc, "winlux/recommended-apps.txt")
    if config["apps"]:
        with open(apps, "w") as f:
            f.write("\n".join(config["apps"]) + "\n")
    elif os.path.exists(apps):
        os.unlink(apps)
    if benchmark:
        with open(os.path.join(etc, "winlux/benchmark.json"), "w") as f:
            json.dump(benchmark, f, ensure_ascii=False, indent=2)


def _write(path, value):
    try:
        with open(path, "w") as f:
            f.write(f"{value}\n")
        return True
    except OSError:
        return False


def _read(path):
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return ""


def disk_kind(sysfs, name):
    if name.startswith("nvme"):
        return "nvme"
    return "hdd" if _read(os.path.join(sysfs, "block", name, "queue/rotational")) == "1" else "ssd"


def apply(config, procfs="/proc", sysfs="/sys"):
    """Applique le profil au noyau en cours. Ignore ce que le noyau ne propose pas."""
    log = []
    for key, value in config["sysctl"].items():
        path = os.path.join(procfs, "sys", *key.split("."))
        if os.path.exists(path) and _write(path, value):
            log.append(f"sysctl {key}={value}")
    cpus = os.path.join(sysfs, "devices/system/cpu")
    try:
        policies = sorted(n for n in os.listdir(os.path.join(cpus, "cpufreq")) if n.startswith("policy"))
    except OSError:
        policies = []
    for policy in policies:
        base = os.path.join(cpus, "cpufreq", policy)
        available = _read(os.path.join(base, "scaling_available_governors")).split()
        chosen = next((g for g in config["governor"] if g in available), None)
        if chosen and _write(os.path.join(base, "scaling_governor"), chosen):
            log.append(f"cpufreq {policy}={chosen}")
    try:
        disks = sorted(os.listdir(os.path.join(sysfs, "block")))
    except OSError:
        disks = []
    for name in disks:
        if name.startswith(("loop", "ram", "zram", "sr", "dm-", "md")):
            continue
        sched_path = os.path.join(sysfs, "block", name, "queue/scheduler")
        available = _read(sched_path).replace("[", "").replace("]", "").split()
        wanted = config["io"].get(disk_kind(sysfs, name))
        if wanted in available and _write(sched_path, wanted):
            log.append(f"io {name}={wanted}")
    try:
        cards = sorted(n for n in os.listdir(os.path.join(sysfs, "class/drm")) if n.startswith("card") and "-" not in n)
    except OSError:
        cards = []
    for card in cards:
        level = os.path.join(sysfs, "class/drm", card, "device/power_dpm_force_performance_level")
        if os.path.exists(level) and _write(level, config["gpu_level"]):
            log.append(f"gpu {card}={config['gpu_level']}")
    return log


def load(root="/"):
    with open(os.path.join(root, "etc/winlux/profile.json")) as f:
        return json.load(f)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument("--apply-boot", action="store_true", help="appliquer /etc/winlux/profile.json")
    group.add_argument("--set", choices=ORDER, help="changer de profil (root) et l'appliquer")
    group.add_argument("--list", action="store_true", help="lister les profils")
    group.add_argument("--show", action="store_true", help="afficher le profil actif")
    args = parser.parse_args()
    if args.list:
        for pid in ORDER:
            print(f"{pid:<8} {display_name(pid):<14} {summary_line(pid)}")
        return 0
    if args.show:
        try:
            print(json.dumps(load(), ensure_ascii=False, indent=2))
        except OSError:
            print(_t("Aucun profil configuré."))
        return 0
    if args.set:
        benchmark = None
        try:
            with open("/etc/winlux/benchmark.json") as f:
                benchmark = json.load(f)
        except (OSError, ValueError):
            pass
        write_config("/", build_config(args.set, benchmark), None)
    try:
        config = load()
    except (OSError, ValueError):
        return 0
    for line in apply(config):
        print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
