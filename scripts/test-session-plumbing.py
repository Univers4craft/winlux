#!/usr/bin/python3
"""Plomberie de la session native relevée sur le vrai PC de damien (v0.2.19).

- #604 : /dev/rfkill ouvert au groupe netdev par une règle udev Winlux, livrée
  dans /usr (donc aussi par la mise à jour), et paquet rfkill dans l'image ;
- #610 : mesa-utils (glxinfo) dans l'image pour la ligne OpenGL du rapport ;
- #615 : at-spi2-core (bus org.a11y.Bus) dans l'image ;
- check-image-session.sh, appelé par build-live-iso.sh après les composants
  du bureau, échoue s'il manque l'un d'eux et passe quand tout y est.
"""
import runpy
import shutil
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
RULE = ROOT / "integration/native/udev/60-winlux-rfkill.rules"
failures = []


def check(cond, message):
    if not cond:
        failures.append(message)


packages = runpy.run_path(str(HERE / "prepare-desktop-packages.py"))["PACKAGES"]
for name in ("rfkill", "mesa-utils", "at-spi2-core", "blueman"):
    check(name in packages, f"paquet {name} absent de l'image")

rule = [line for line in RULE.read_text().splitlines() if line and not line.startswith("#")]
check(len(rule) == 1, "60-winlux-rfkill.rules doit tenir en une règle")
for part in ('KERNEL=="rfkill"', 'GROUP="netdev"', 'MODE="0664"'):
    check(part in rule[0], f"règle rfkill sans {part}")
if shutil.which("udevadm") and "verify" in subprocess.run(["udevadm", "--help"], capture_output=True, text=True).stdout:
    verify = subprocess.run(["udevadm", "verify", "--no-style", str(RULE)], capture_output=True, text=True)
    check(verify.returncode == 0, "udevadm verify refuse la règle : " + verify.stdout + verify.stderr)

stage_script = (HERE / "stage-desktop-components.sh").read_text()
check('integration/native/udev/60-winlux-rfkill.rules" "$stage/usr/lib/udev/rules.d/60-winlux-rfkill.rules"' in stage_script,
      "stage-desktop-components.sh n'installe pas la règle rfkill")
winlux_path = runpy.run_path(str(HERE / "stage-apt-root.py"), run_name="winlux_test")["winlux_path"]
check(winlux_path("usr/lib/udev/rules.d/60-winlux-rfkill.rules"), "la règle rfkill n'est pas dans le paquet de mise à jour")
check(not winlux_path("usr/lib/udev/rules.d/50-udev-default.rules"), "le paquet de mise à jour reprendrait une règle d'Ubuntu")

build = (HERE / "build-live-iso.sh").read_text()
check('scripts/check-image-session.sh" "$stage"' in build, "build-live-iso.sh n'appelle pas check-image-session.sh")
check(build.index("check-image-session.sh") > build.index("stage-desktop-components.sh"),
      "check-image-session.sh doit passer après les composants du bureau")


def image_check(stage):
    return subprocess.run(["sh", str(HERE / "check-image-session.sh"), str(stage)], capture_output=True, text=True)


tmp = Path(tempfile.mkdtemp(prefix="winlux-session-"))
try:
    files = ["usr/bin/rfkill", "usr/bin/glxinfo", "usr/libexec/at-spi-bus-launcher",
             "usr/share/dbus-1/services/org.a11y.Bus.service"]
    for name in files:
        (tmp / name).parent.mkdir(parents=True, exist_ok=True)
        (tmp / name).write_text("")
    target = tmp / "usr/lib/udev/rules.d/60-winlux-rfkill.rules"
    target.parent.mkdir(parents=True)
    shutil.copy(RULE, target)
    result = image_check(tmp)
    check(result.returncode == 0, "check-image-session.sh refuse une image complète : " + result.stderr)
    for name in files:
        (tmp / name).rename(tmp / (name + ".off"))
        result = image_check(tmp)
        check(result.returncode != 0 and name in result.stderr, f"check-image-session.sh accepte une image sans /{name}")
        (tmp / (name + ".off")).rename(tmp / name)
    target.write_text('KERNEL=="rfkill", MODE="0660"\n')
    check(image_check(tmp).returncode != 0, "check-image-session.sh accepte une règle rfkill réservée à root")
finally:
    shutil.rmtree(tmp)

if failures:
    raise SystemExit("ÉCHEC test-session-plumbing :\n- " + "\n- ".join(failures))
print("OK test-session-plumbing : rfkill, glxinfo, bus d'accessibilité et règle udev")
