#!/usr/bin/env python3
"""Page Performances de l'installateur : processeur et carte graphique décrits juste.

Sur un portable à 6 cœurs et 12 threads, la page affichait « 1 cœur : … Mo/s ».
C'était la mesure mono-cœur, mais elle se lisait comme le nombre de cœurs. La
carte NVIDIA n'était décrite que par « NVIDIA · carte dédiée (estimation) ».
"""
import os
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import winlux_benchmark as bench  # noqa: E402

with tempfile.TemporaryDirectory() as temp:
    root = Path(temp)
    # 6 cœurs, 2 threads par cœur : cpu0..cpu11.
    for n in range(12):
        topo = root / f"sys/devices/system/cpu/cpu{n}/topology"
        topo.mkdir(parents=True)
        (topo / "core_id").write_text(f"{n % 6}\n")
        (topo / "physical_package_id").write_text("0\n")
    (root / "sys/devices/system/cpu/cpufreq").mkdir()
    assert bench.cpu_cores(str(root / "sys"), threads=12) == 6
    assert bench.cpu_cores(str(root / "absent"), threads=12) == 12

    info = root / "cpuinfo"
    info.write_text("processor\t: 0\nmodel name\t: 12th Gen Intel(R) Core(TM) i5-12450H\n")
    assert bench.cpu_model(str(info)) == "12th Gen Intel(R) Core(TM) i5-12450H"

    ids = root / "pci.ids"
    ids.write_text("# base\n10de  NVIDIA Corporation\n\t1f99  TU117M\n"
                   "\t25a2  GA107M [GeForce RTX 3050 Mobile]\n\t\t17aa 3a3a  Sous-système\n"
                   "8086  Intel Corporation\n\t46a3  Alder Lake-P GT1 [UHD Graphics]\n")
    assert bench.pci_ids_name("0x10de", "0x25a2", [str(ids)]) == "GA107M [GeForce RTX 3050 Mobile]"
    assert bench.pci_ids_name("0x8086", "0x25a2", [str(ids)]) == ""
    gpu = {"vendor": "0x10de", "device": "0x25a2", "slot": "0000:01:00.0", "driver": "nouveau", "vram_bytes": 0}
    assert bench.gpu_model(gpu, "", [str(ids)], str(root / "noproc")) == "GeForce RTX 3050 Mobile"
    # Pilote NVIDIA propriétaire : son nom exact passe avant pci.ids.
    proc = root / "nvidia/0000:01:00.0"
    proc.mkdir(parents=True)
    (proc / "information").write_text("Model: \t\t NVIDIA GeForce RTX 3050 Laptop GPU\nIRQ: 16\n")
    assert bench.gpu_model(gpu, "", [str(ids)], str(root / "nvidia")) == "NVIDIA GeForce RTX 3050 Laptop GPU"
    # Rien de reconnu : rendu OpenGL s'il est matériel, sinon vide (estimation).
    unknown = {**gpu, "device": "0xffff", "slot": ""}
    assert bench.gpu_model(unknown, "NV137/PCIe/SSE2", [str(ids)], str(root / "noproc")) == "NV137"
    assert bench.gpu_model(unknown, "llvmpipe (LLVM 19)", [str(ids)], str(root / "noproc")) == ""

    # bench_gpu sur un faux /sys : le modèle remplace « (estimation) ».
    card = root / "sys/class/drm/card0"
    dev = root / "sys/devices/pci0000:00/0000:01:00.0"
    dev.mkdir(parents=True)
    (dev / "vendor").write_text("0x10de\n")
    (dev / "device").write_text("0x25a2\n")
    card.mkdir(parents=True)
    os.symlink(dev, card / "device")
    bench.PCI_IDS = (str(ids),)
    bench.gpu_model.__defaults__ = ("", (str(ids),), str(root / "noproc"))
    result = bench.bench_gpu(str(root / "sys"))
    assert result["model"] == "NVIDIA GeForce RTX 3050 Mobile", result
    assert result["detail"].startswith("NVIDIA GeForce RTX 3050 Mobile · carte dédiée"), result
    assert "estimation" not in result["detail"], result

# Le détail du processeur dit « par cœur » et donne les cœurs physiques.
detail = bench.bench_cpu(0.05)["detail"]
assert "1 cœur" not in detail and "par cœur" in detail and "cœurs, " in detail, detail
row = bench._TR["{cores} cœurs, {threads} threads · {multi} Mo/s en tout, {single} Mo/s par cœur"]
assert len(row) == 5 and all("{cores}" in t and "{single}" in t for t in row)
print("test-benchmark-materiel : OK (cœurs physiques, modèle du processeur et de la carte graphique)")
