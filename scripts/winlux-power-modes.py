#!/usr/bin/python3
"""Modes d'alimentation réellement disponibles sur cette machine.

Trois modes, comme Windows 11 : « saver » (économie), « balanced » (normal) et
« performance » (turbo). Chaque mode n'est proposé que si la plomberie Linux
présente sait l'appliquer, dans cet ordre :

1. power-profiles-daemon (ou tuned-ppd), via powerprofilesctl ;
2. tuned, via tuned-adm ;
3. les gouverneurs cpufreq du noyau (et la préférence EPP des pilotes
   intel_pstate / amd-pstate), appliqués par `winlux-control power-profile-*`.

Sans aucune de ces sources (machine virtuelle, processeur sans cpufreq), la
liste est vide et l'interface cache entièrement la section.

    winlux-power-modes list      modes disponibles, un par ligne (« * » = actif)
    winlux-power-modes get       mode actif
    winlux-power-modes set MODE  applique saver | balanced | performance

WINLUX_POWER_ROOT déplace /sys (tests).
"""
from pathlib import Path
import os
import shutil
import subprocess
import sys

MODES = ("saver", "balanced", "performance")
# Profils tuned rangés par mode Winlux, du plus naturel au moins naturel.
TUNED = {
    "saver": ("powersave", "laptop-battery-powersave", "balanced-battery"),
    "balanced": ("balanced", "desktop", "laptop-ac-powersave"),
    "performance": ("throughput-performance", "latency-performance", "accelerator-performance"),
}
PPD = {"saver": "power-saver", "balanced": "balanced", "performance": "performance"}
PSTATE_DRIVERS = ("intel_pstate", "amd-pstate-epp", "amd-pstate")


def _root():
    return Path(os.environ.get("WINLUX_POWER_ROOT", "/"))


def _run(args, timeout=3):
    try:
        result = subprocess.run(args, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.SubprocessError):
        return None
    return result.stdout if result.returncode == 0 else None


def _read(path):
    try:
        return path.read_text(encoding="ascii", errors="replace").strip()
    except OSError:
        return ""


# ── 1. power-profiles-daemon ────────────────────────────────────────────────

def _ppd():
    if not shutil.which("powerprofilesctl"):
        return None
    output = _run(["powerprofilesctl", "list"])
    if not output:
        return None  # démon absent ou arrêté : on passe à la source suivante
    available, active = [], None
    for line in output.splitlines():
        stripped = line.strip()
        if not stripped.endswith(":") or line.startswith((" " * 4, "\t\t")):
            continue  # lignes de détail (« CpuDriver: … »)
        current = stripped.startswith("*")
        name = stripped.lstrip("* ").rstrip(":")
        mode = next((key for key, value in PPD.items() if value == name), None)
        if mode:
            available.append(mode)
            if current:
                active = mode
    return ("ppd", [m for m in MODES if m in available], active) if available else None


# ── 2. tuned ───────────────────────────────────────────────────────────────

def _tuned():
    if not shutil.which("tuned-adm"):
        return None
    output = _run(["tuned-adm", "list"], timeout=5)
    if not output:
        return None
    names = {line.strip()[2:].split(" ", 1)[0] for line in output.splitlines() if line.strip().startswith("- ")}
    active_name = ""
    for line in output.splitlines():
        if line.startswith("Current active profile:"):
            active_name = line.split(":", 1)[1].strip()
    available = [mode for mode in MODES if any(name in names for name in TUNED[mode])]
    active = next((mode for mode in MODES if active_name in TUNED[mode]), None)
    return ("tuned", available, active) if available else None


def _tuned_profile(mode):
    output = _run(["tuned-adm", "list"], timeout=5) or ""
    names = {line.strip()[2:].split(" ", 1)[0] for line in output.splitlines() if line.strip().startswith("- ")}
    return next((name for name in TUNED[mode] if name in names), None)


# ── 3. cpufreq (gouverneurs du noyau) ───────────────────────────────────────

def _policies():
    base = _root() / "sys/devices/system/cpu/cpufreq"
    try:
        return sorted(path for path in base.iterdir() if path.name.startswith("policy"))
    except OSError:
        return []


def _cpufreq():
    policies = _policies()
    if not policies:
        return None
    first = policies[0]
    governors = set(_read(first / "scaling_available_governors").split())
    preferences = set(_read(first / "energy_performance_available_preferences").split())
    driver = _read(first / "scaling_driver")
    if not os.access(first / "scaling_governor", os.W_OK):
        return None  # lecture seule (conteneur, compte sans droits) : rien d'applicable
    pstate = driver in PSTATE_DRIVERS
    available = []
    # Économie : gouverneur économe, ou préférence EPP « power » des pilotes pstate.
    if "power" in preferences or "conservative" in governors or ("powersave" in governors and not pstate):
        available.append("saver")
    # Normal : gouverneur dynamique, ou pstate « powersave » avec EPP équilibrée.
    if governors & {"schedutil", "ondemand"} or (pstate and "powersave" in governors
                                                 and preferences & {"balance_performance", "balance_power", "default"}):
        available.append("balanced")
    if "performance" in governors:
        available.append("performance")
    governor = _read(first / "scaling_governor")
    preference = _read(first / "energy_performance_preference")
    if governor == "performance":
        active = "performance"
    elif preference == "power" or governor == "conservative" or (governor == "powersave" and not pstate):
        active = "saver"
    else:
        active = "balanced"
    return ("cpufreq", available, active if active in available else None)


def detect():
    """(source, modes disponibles, mode actif) ; modes vides si rien ne s'applique.

    Un seul mode disponible n'est pas un choix : la liste est alors vide aussi."""
    if os.environ.get("WINLUX_POWER_ROOT"):
        sources = (_cpufreq,)  # tests : uniquement l'arborescence simulée
    else:
        sources = (_ppd, _tuned, _cpufreq)
    for source in sources:
        found = source()
        if found and len(found[1]) >= 2:
            return found
    return (None, [], None)


def apply(mode, source=None):
    if mode not in MODES:
        raise ValueError(mode)
    source = source or detect()[0]
    if source == "ppd":
        return _run(["powerprofilesctl", "set", PPD[mode]]) is not None
    if source == "tuned":
        profile = _tuned_profile(mode)
        return bool(profile) and _run(["tuned-adm", "profile", profile], timeout=30) is not None
    if source == "cpufreq":
        # Même chemin que Paramètres > Alimentation : gouverneur, EPP, turbo et
        # périphériques, mode mémorisé pour le prochain démarrage.
        if shutil.which("winlux-control") and not os.environ.get("WINLUX_POWER_ROOT"):
            return _run(["winlux-control", "power-profile-" + mode], timeout=15) is not None
        governor = {"saver": ("powersave", "conservative"), "balanced": ("schedutil", "ondemand", "powersave"),
                    "performance": ("performance",)}[mode]
        written = False
        for policy in _policies():
            available = _read(policy / "scaling_available_governors").split()
            choice = next((name for name in governor if name in available), None)
            if choice:
                try:
                    (policy / "scaling_governor").write_text(choice)
                    written = True
                except OSError:
                    pass
            preferences = _read(policy / "energy_performance_available_preferences").split()
            epp = {"saver": "power", "balanced": "balance_performance", "performance": "performance"}[mode]
            if epp in preferences:
                try:
                    (policy / "energy_performance_preference").write_text(epp)
                except OSError:
                    pass
        return written
    return False


def main(argv):
    command = argv[1] if len(argv) > 1 else "list"
    source, available, active = detect()
    if command == "list":
        for mode in available:
            print(("* " if mode == active else "  ") + mode)
        return 0
    if command == "get":
        print(active or "")
        return 0 if active else 1
    if command == "set" and len(argv) == 3 and argv[2] in available:
        return 0 if apply(argv[2], source) else 1
    print(__doc__.split("\n\n")[-2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
