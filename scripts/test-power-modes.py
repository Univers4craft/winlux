#!/usr/bin/python3
"""Détection des modes Économie / Normal / Turbo sur des /sys simulés."""
from importlib.machinery import SourceFileLoader
import importlib.util
import os
from pathlib import Path
import tempfile

script = Path(__file__).resolve().with_name("winlux-power-modes.py")
loader = SourceFileLoader("winlux_power_modes", str(script))
spec = importlib.util.spec_from_loader("winlux_power_modes", loader)
modes = importlib.util.module_from_spec(spec)
loader.exec_module(modes)


def machine(root, driver, governors, governor, preferences="", preference="", cpus=2):
    for index in range(cpus):
        policy = root / f"sys/devices/system/cpu/cpufreq/policy{index}"
        policy.mkdir(parents=True)
        (policy / "scaling_driver").write_text(driver + "\n")
        (policy / "scaling_available_governors").write_text(governors + "\n")
        (policy / "scaling_governor").write_text(governor + "\n")
        if preferences:
            (policy / "energy_performance_available_preferences").write_text(preferences + "\n")
            (policy / "energy_performance_preference").write_text(preference + "\n")


def case(**kwargs):
    temp = Path(tempfile.mkdtemp())
    machine(temp, **kwargs)
    os.environ["WINLUX_POWER_ROOT"] = str(temp)
    return temp


# Machine virtuelle : pas de cpufreq, section cachée.
os.environ["WINLUX_POWER_ROOT"] = tempfile.mkdtemp()
assert modes.detect() == (None, [], None), modes.detect()

# acpi-cpufreq classique : les trois modes.
root = case(driver="acpi-cpufreq", governors="conservative ondemand userspace powersave performance schedutil",
            governor="schedutil")
assert modes.detect() == ("cpufreq", ["saver", "balanced", "performance"], "balanced"), modes.detect()
assert modes.apply("performance")
assert (root / "sys/devices/system/cpu/cpufreq/policy1/scaling_governor").read_text() == "performance"
assert modes.detect()[2] == "performance"
assert modes.apply("saver") and modes.detect()[2] == "saver"

# intel_pstate actif : powersave = normal, l'économie passe par l'EPP « power ».
root = case(driver="intel_pstate", governors="performance powersave", governor="powersave",
            preferences="default performance balance_performance balance_power power", preference="balance_performance")
assert modes.detect() == ("cpufreq", ["saver", "balanced", "performance"], "balanced"), modes.detect()
assert modes.apply("saver")
policy = root / "sys/devices/system/cpu/cpufreq/policy0"
assert (policy / "scaling_governor").read_text() == "powersave"
assert (policy / "energy_performance_preference").read_text() == "power"
assert modes.detect()[2] == "saver"

# intel_pstate sans EPP : seulement normal et turbo (pas d'économie inventée).
case(driver="intel_pstate", governors="performance powersave", governor="powersave")
assert modes.detect()[1] == [], "pstate sans EPP : powersave seul n'est ni économie ni normal vérifiable"

# Un seul gouverneur : pas de choix, section cachée.
case(driver="cppc_cpufreq", governors="performance", governor="performance")
assert modes.detect() == (None, [], None)

# Économie + turbo seulement (pas de gouverneur dynamique).
case(driver="acpi-cpufreq", governors="powersave performance", governor="powersave")
assert modes.detect() == ("cpufreq", ["saver", "performance"], "saver"), modes.detect()

# Gouverneur non modifiable (lecture seule) : rien d'applicable.
if os.geteuid() != 0:
    root = case(driver="acpi-cpufreq", governors="powersave performance schedutil", governor="schedutil")
    for policy in (root / "sys/devices/system/cpu/cpufreq").iterdir():
        (policy / "scaling_governor").chmod(0o444)
    assert modes.detect() == (None, [], None)

print("OK: modes d'alimentation détectés selon le matériel réel, section cachée sans choix")
