#!/usr/bin/env python3
"""Benchmark, profils d'utilisation et migration réseau de l'installateur (sans GTK ni root)."""
import ast
import json
import os
from pathlib import Path
import socket
import sys
import tempfile
import threading
import time

project = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(project / "scripts"))
import winlux_benchmark as bench  # noqa: E402
import winlux_migrate as migrate  # noqa: E402
import winlux_profiles as profiles  # noqa: E402

GiB = 1024 ** 3


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


# ── Benchmark ────────────────────────────────────────────────────
assert bench.score("cpu", 0) == 1.0 and bench.score("cpu", 10 ** 9) == 9.9
assert bench.score("ram", 0.5) == 1.0 and 1.0 < bench.score("ram", 5) < 9.9
assert bench.fr_number(1234.5, 1) == "1 234,5"
with tempfile.TemporaryDirectory() as tmp:
    image = Path(tmp) / "disk.img"
    image.write_bytes(os.urandom(8 * 1024 * 1024))
    events = []
    summary = bench.run_all(str(image), quick=True, emit=events.append, sysfs=tmp)
    assert [e["event"] for e in events][-1] == "done"
    assert set(summary["results"]) == {"cpu", "ram", "disk", "gpu"}
    for result in summary["results"].values():
        assert 1.0 <= result["score"] <= 9.9 and result["detail"], result
    assert summary["base_score"] == min(r["score"] for r in summary["results"].values())
    assert summary["results"]["disk"]["read_mbs"] > 0
    assert summary["recommended_profile"] in profiles.PROFILES
    write_result = bench.bench_disk_write(tmp, seconds=0.2, limit=8 * 1024 * 1024)
    assert write_result["write_mbs"] > 0 and os.listdir(tmp) == ["disk.img"], "fichier de test non supprimé"

with tempfile.TemporaryDirectory() as tmp:
    sysfs = Path(tmp)
    write(sysfs / "class/drm/card0/device/vendor", "0x1002\n")
    write(sysfs / "class/drm/card0/device/mem_info_vram_total", str(8 * GiB))
    write(sysfs / "class/drm/card1/device/vendor", "0x8086\n")
    gpu = bench.bench_gpu(str(sysfs))
    assert gpu["vendor"] == "0x1002" and gpu["score"] == 8.0, gpu
    _tips, rec = bench.recommend({"gpu": gpu, "ram": {"total_bytes": 16 * GiB}, "cpu": {"threads": 16}})
    assert rec == "gaming"
    tips, rec = bench.recommend({"gpu": {"score": 2.0, "available": True}, "ram": {"total_bytes": 2 * GiB},
                                 "cpu": {"threads": 2}, "disk": {"type": "HDD", "read_mbs": 80}})
    assert rec == "office" and any("zram" in t for t in tips) and any("BFQ" in t for t in tips)

# ── Profils ──────────────────────────────────────────────────────
assert profiles.ORDER == ["gaming", "office", "dev", "server"]
for pid in profiles.ORDER:
    p = profiles.PROFILES[pid]
    assert {"nvme", "ssd", "hdd"} == set(p["io"]) and p["apps"] and "vm.swappiness" in p["sysctl"]
low = profiles.build_config("gaming", {"results": {"ram": {"total_bytes": 2 * GiB}}})
assert low["sysctl"]["vm.swappiness"] == 60 and low["notes"]
assert profiles.build_config("dev", None, install_apps=False)["apps"] == []

with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp) / "target"
    config = profiles.build_config("gaming", {"base_score": 5.0, "results": {}})
    profiles.write_config(str(root), config, {"base_score": 5.0})
    assert json.loads((root / "etc/winlux/profile.json").read_text())["id"] == "gaming"
    assert "vm.swappiness = 10" in (root / "etc/sysctl.d/90-winlux-profile.conf").read_text()
    assert "gamemode" in (root / "etc/winlux/recommended-apps.txt").read_text()
    assert json.loads((root / "etc/winlux/benchmark.json").read_text())["base_score"] == 5.0

    procfs, sysfs = Path(tmp) / "proc", Path(tmp) / "sys"
    write(procfs / "sys/vm/swappiness", "60")
    write(procfs / "sys/vm/max_map_count", "65530")
    cpufreq = sysfs / "devices/system/cpu/cpufreq/policy0"
    write(cpufreq / "scaling_available_governors", "performance powersave")
    write(cpufreq / "scaling_governor", "powersave")
    write(sysfs / "block/nvme0n1/queue/scheduler", "[none] mq-deadline kyber")
    write(sysfs / "block/nvme0n1/queue/rotational", "0")
    write(sysfs / "block/sda/queue/scheduler", "[mq-deadline] bfq none")
    write(sysfs / "block/sda/queue/rotational", "1")
    write(sysfs / "block/loop0/queue/scheduler", "[none]")
    write(sysfs / "class/drm/card0/device/power_dpm_force_performance_level", "auto")
    log = profiles.apply(config, str(procfs), str(sysfs))
    assert (procfs / "sys/vm/swappiness").read_text().strip() == "10"
    assert not (procfs / "sys/kernel/split_lock_mitigate").exists(), "clé absente du noyau créée"
    assert (cpufreq / "scaling_governor").read_text().strip() == "performance"
    assert (sysfs / "block/nvme0n1/queue/scheduler").read_text().strip() == "none"
    assert (sysfs / "block/sda/queue/scheduler").read_text().strip() == "bfq"
    assert (sysfs / "block/loop0/queue/scheduler").read_text().strip() == "[none]"
    assert (sysfs / "class/drm/card0/device/power_dpm_force_performance_level").read_text().strip() == "high"
    assert "io sda=bfq" in log

# ── Migration : DNS-SD, validation ───────────────────────────────
for name in ["a.b.local", "PC de Damien._winlux-migrate._tcp.local"]:
    assert migrate.decode_name(migrate.encode_name(name), 0) == (name, len(migrate.encode_name(name)))
query = migrate.parse_packet(migrate.build_query(migrate.BROWSE_TYPES, qid=7))
assert query["id"] == 7 and not query["response"] and [q for q, _ in query["questions"]] == migrate.BROWSE_TYPES
reply = migrate.build_response(7, [migrate.SERVICE], "PC de Damien", "pcdamien", "192.168.1.20", 22,
                               {"os": "winlux", "user": "damien"}, unicast=True)
packet = migrate.parse_packet(reply)
assert packet["response"] and packet["id"] == 7
hosts = migrate.hosts_from_records(packet["records"], "192.168.1.99")
assert hosts == [{"name": "PC de Damien", "host": "pcdamien.local", "ip": "192.168.1.20", "port": 22,
                  "kind": "winlux", "os": "winlux", "user": "damien"}], hosts
# Pointeurs de compression (format des réponses Avahi).
compressed = b"\0\0\x84\0\0\0\0\x01\0\0\0\0" + migrate.encode_name("_ssh._tcp.local") + b"\0\x0c\0\x01\0\0\0\x78"
rdata = b"\x04serv\xc0\x0c"
compressed += len(rdata).to_bytes(2, "big") + rdata
assert migrate.parse_packet(compressed)["records"][0][2] == "serv._ssh._tcp.local"
for bad in [("-oProxyCommand=x", "u"), ("host;rm", "u"), ("host", "-l"), ("host", "a b@c"), ("", "u")]:
    try:
        migrate.validate(*bad)
    except ValueError:
        pass
    else:
        raise AssertionError(f"accepté : {bad}")
assert migrate.validate(" 192.168.1.5 ", "Jean Dupont") == ("192.168.1.5", "Jean Dupont", 22)
assert migrate.rsync_host("fe80::1%eth0") == "[fe80::1%eth0]" and migrate.rsync_host("pc.local") == "pc.local"
assert "SSH n'est pas activé" in migrate.explain_ssh_error("ssh: connect to host x port 22: Connection refused")
assert "Mot de passe" in migrate.explain_ssh_error("user@x: Permission denied (password).")

# Découverte réelle sur la boucle locale quand le multicast est disponible.
stop = threading.Event()
try:
    server = threading.Thread(target=migrate.serve_mdns, args=("Test", "testpc", 2222, {"os": "winlux"}, stop), daemon=True)
    server.start()
    time.sleep(0.3)
    found = migrate.scan(1.5) if server.is_alive() else []
    if found:
        assert any(h["name"] == "Test" and h["port"] == 2222 for h in found), found
    else:
        print("mDNS : pas de multicast dans cet environnement, découverte réelle non vérifiée")
except OSError as e:
    print(f"mDNS : {e}, découverte réelle non vérifiée")
finally:
    stop.set()

# ── Installateur : étapes et traductions cohérentes ──────────────
tree = ast.parse((project / "scripts/winlux-installer.py").read_text())
values = {}
for node in tree.body:
    if isinstance(node, ast.Assign) and isinstance(node.targets[0], ast.Name):
        if node.targets[0].id in ("STEPS", "STEP_KEYS", "TRANSLATIONS"):
            values[node.targets[0].id] = ast.literal_eval(node.value)
assert len(values["STEPS"]) == len(values["STEP_KEYS"]) == 12
for key in ("benchmark", "profile", "migration"):
    assert key in values["STEP_KEYS"]
assert values["STEP_KEYS"].index("disk") < values["STEP_KEYS"].index("benchmark"), "le benchmark lit le disque choisi"
for lang, table in values["TRANSLATIONS"].items():
    assert len(table["steps"]) == len(values["STEPS"]), lang
source = (project / "scripts/winlux-installer.py").read_text()
assert "self.current_step == " not in source, "numéro d'étape codé en dur"

print("Installateur : benchmark, profils et migration OK")
