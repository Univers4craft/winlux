#!/usr/bin/env python3
"""Pilotes au démarrage : l'affichage n'attend que les siens (#3415).

Exécute le bloc « pilotes » de l'init (scripts/build-live-iso.sh, de
pci_modalias jusqu'au repère drivers) avec un faux /sys, un faux modprobe et
un faux udevadm :
- les cartes de stockage, réseau et graphiques (classes 0x01, 0x02, 0x03)
  sont chargées avant le repère drivers, en un seul modprobe ;
- les autres (son, USB…) sont chargées après, en arrière-plan, et un pilote
  lent (1,5 s ici) ne retarde pas le repère ;
- udev.done n'apparaît qu'après le reste, la suite (droits, inventaire) est
  réveillée par le tube pilotes.fifo, sans boucle d'attente ;
- wait_drivers attend pilotes.done puis rend la main au bout de sa limite.
"""
import pathlib
import re
import subprocess
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
source = (ROOT / "scripts/build-live-iso.sh").read_text(encoding="utf-8")
init = re.search(r'^cat >"\$stage/init" <<.INIT.\n(.*?)^INIT$', source, re.S | re.M).group(1)
block = re.search(r"^pci_modalias\(\) \{\n.*?^boot_mark drivers\n", init, re.S | re.M)
assert block, "bloc des pilotes introuvable dans l'init"
block = block.group(0)

with tempfile.TemporaryDirectory() as tmp:
    tmp = pathlib.Path(tmp)
    devices = tmp / "sys/bus/pci/devices"
    for slot, cls, alias in (("0000:00:02.0", "0x030000", "pci:i915"), ("0000:01:00.0", "0x030200", "pci:nvidia"),
                             ("0000:00:17.0", "0x010601", "pci:ahci"), ("0000:00:14.3", "0x028000", "pci:iwlwifi"),
                             ("0000:00:1f.3", "0x040380", "pci:son"), ("0000:00:14.0", "0x0c0330", "pci:xhci")):
        d = devices / slot
        d.mkdir(parents=True)
        (d / "class").write_text(cls + "\n")
        (d / "modalias").write_text(alias + "\n")
    (tmp / "run").mkdir()
    log = tmp / "journal"
    modprobe = tmp / "modprobe"
    modprobe.write_text(f"""#!/bin/sh
read -r up idle </proc/uptime
echo "modprobe $* @$up" >>{log}
case "$*" in *pci:son*) sleep 1.5 ;; esac
""")
    udevadm = tmp / "udevadm"
    udevadm.write_text(f"#!/bin/sh\necho \"udevadm $*\" >>{log}\n")
    busybox = tmp / "busybox"
    busybox.write_text('#!/bin/sh\ncmd=$1; shift\ncase "$cmd" in mdev) exit 0 ;; esac\nexec "$cmd" "$@"\n')
    for f in (modprobe, udevadm, busybox):
        f.chmod(0o755)
    text = (block.replace("/sys/bus/pci/devices", str(devices))
            .replace("/usr/sbin/modprobe", str(modprobe))
            .replace("/usr/bin/udevadm", str(udevadm))
            .replace("/bin/busybox", str(busybox))
            .replace("/run/winlux", str(tmp / "run")))
    script = f"""
boot_mark() {{ read -r up idle </proc/uptime; echo "repère $1 @$up" >>{log}; }}
{text}
# La suite des pilotes (droits, inventaire) lit le tube : rien ne tourne
# en boucle d'ici là.
read -r drivers_ready <{tmp / 'run/pilotes.fifo'}
echo "suite $drivers_ready" >>{log}
wait
"""
    started = time.monotonic()
    out = subprocess.run(["sh", "-c", script], capture_output=True, text=True, timeout=30)
    lines = log.read_text().splitlines()
    print("\n".join(lines))
    print(out.stdout + out.stderr, end="")
    first = next(i for i, l in enumerate(lines) if l.startswith("modprobe"))
    mark = next(i for i, l in enumerate(lines) if l.startswith("repère drivers"))
    rest = next(i for i, l in enumerate(lines) if l.startswith("modprobe") and "pci:son" in l)
    display_line = lines[first]
    for alias in ("pci:i915", "pci:nvidia", "pci:ahci", "pci:iwlwifi"):
        assert alias in display_line, f"{alias} absent du premier modprobe : {display_line}"
    assert "pci:son" not in display_line and "pci:xhci" not in display_line, display_line
    assert "pci:xhci" in lines[rest], "USB pas chargé avec le reste"
    trigger = next(l for l in lines[first:mark] if l.startswith("udevadm trigger"))
    for sub in ("drm", "input", "block"):
        assert f"--subsystem-match={sub}" in trigger, trigger
    t_mark = float(lines[mark].split("@")[1])
    t_rest = float(lines[rest].split("@")[1])
    assert lines[-1] == "suite pret", lines[-1:]
    assert lines[-3].startswith("udevadm trigger --action=add") and "subsystem" not in lines[-3], lines[-3:]
    done = float((tmp / "run/udev.done").read_text())
    assert done >= t_rest + 1.4, f"udev.done avant la fin du reste ({done} < {t_rest} + 1,5)"
    assert t_mark < t_rest + 1.0, "le repère drivers a attendu le pilote lent"
    print(f"repère drivers {t_rest - t_mark:.2f} s avant le reste ; fin des pilotes {done - t_mark:.2f} s après le repère")

    # wait_drivers : rend la main dès pilotes.done, et au bout de sa limite sinon.
    wait = re.search(r"^wait_drivers\(\) \{\n.*?^\}\n", init, re.S | re.M).group(0)
    wait = wait.replace("/run/winlux", str(tmp / "run")).replace("/bin/busybox", str(busybox))
    quick = wait.replace("-lt 300", "-lt 10")
    (tmp / "run/pilotes.done").unlink(missing_ok=True)
    t0 = time.monotonic()
    r = subprocess.run(["sh", "-c", quick + "\nwait_drivers; echo rendu"], capture_output=True, text=True, timeout=10)
    assert "rendu" in r.stdout and "pas prêts" in r.stderr, r
    assert time.monotonic() - t0 < 3, "limite de wait_drivers pas tenue"
    p = subprocess.Popen(["sh", "-c", wait + "\nwait_drivers; echo rendu"], stdout=subprocess.PIPE, text=True)
    time.sleep(0.3)
    (tmp / "run/pilotes.done").write_text("")
    t0 = time.monotonic()
    assert "rendu" in p.communicate(timeout=5)[0] and time.monotonic() - t0 < 1, "wait_drivers ne voit pas pilotes.done"
    print("wait_drivers : rend la main dès pilotes.done, et après sa limite sinon")
    # Suite des pilotes : attend le tube, puis droits, pilotes.done, inventaire.
    chain = re.search(r"^\(\n    read -r drivers_ready <.*?^\) &\n", init, re.S | re.M)
    assert chain, "suite des pilotes introuvable"
    run = tmp / "run2"
    run.mkdir()
    fifo = run / "pilotes.fifo"
    subprocess.run(["mkfifo", str(fifo)], check=True)
    chain_text = chain.group(0).replace("/run/winlux", str(run)).replace("/bin/busybox", str(busybox))
    chain_text = re.sub(r"/usr/bin/systemctl", str(tmp / "absent"), chain_text)
    chain_text = re.sub(r"/usr/sbin/NetworkManager", "/bin/sh", chain_text)
    order = tmp / "ordre"
    script = f"""
device_access() {{ test -e {run}/pilotes.done && echo "droits APRÈS pilotes.done" >>{order} || echo droits >>{order}; }}
hardware_rest() {{ test -e {run}/pilotes.done && echo inventaire >>{order}; }}
{chain_text}
sleep 0.5
test -e {order} && echo "TROP TOT" >>{order}
echo pret >{fifo}
wait
"""
    r = subprocess.run(["sh", "-c", script], capture_output=True, text=True, timeout=20)
    got = order.read_text().split() if order.exists() else []
    print("suite :", got, r.stderr.strip())
    assert got == ["droits", "inventaire"] and (run / "pilotes.done").exists(), got
    print("suite des pilotes : rien avant le tube, puis droits, pilotes.done, inventaire")
print("pilotes au démarrage : OK")
