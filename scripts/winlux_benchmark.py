#!/usr/bin/python3
"""Évaluation rapide du matériel Winlux (processeur, mémoire, disque, graphique).

Les notes vont de 1,0 à 9,9 comme l'indice de performance de Windows ; la note
globale est celle du composant le plus lent. Lancé en processus séparé par
l'installateur : `--json-lines` émet une ligne JSON par étape terminée.
"""
import argparse
import hashlib
import json
import math
import mmap
import os
import shutil
import subprocess
import sys
import threading
import time

MiB = 1024 * 1024
GiB = 1024 * MiB

# Bornes (valeur → note 1,0 et valeur → note 9,9), échelle logarithmique.
SCALES = {
    "cpu": (40.0, 16000.0),      # Mo/s SHA-256 cumulés sur tous les cœurs
    "ram": (0.5, 25.0),          # Go/s de copie mémoire
    "disk": (15.0, 5000.0),      # Mo/s de lecture séquentielle
}

VIRTUAL_GPU_VENDORS = {"0x1af4", "0x1234", "0x15ad", "0x1b36", "0x80ee", "0x1414"}
GPU_VENDOR_NAMES = {"0x10de": "NVIDIA", "0x1002": "AMD", "0x8086": "Intel"}


def score(kind, value):
    lo, hi = SCALES[kind]
    if not value or value <= 0:
        return 1.0
    raw = 1.0 + 8.9 * math.log(value / lo) / math.log(hi / lo)
    return round(min(9.9, max(1.0, raw)), 1)


def fr_number(value, digits=0):
    text = f"{value:,.{digits}f}".replace(",", " ")
    return text.replace(".", ",")


def cpu_threads():
    try:
        return len(os.sched_getaffinity(0))
    except (AttributeError, OSError):
        return os.cpu_count() or 1


def _hash_worker(seconds, block, out, index):
    data = os.urandom(block)
    done = 0
    end = time.perf_counter() + seconds
    while time.perf_counter() < end:
        hashlib.sha256(data).digest()
        done += block
    out[index] = done


def bench_cpu(seconds=1.5):
    """SHA-256 en C : hashlib relâche le GIL, les threads chargent tous les cœurs."""
    block = 1 * MiB
    single = [0]
    start = time.perf_counter()
    _hash_worker(seconds / 2, block, single, 0)
    single_mbs = single[0] / MiB / (time.perf_counter() - start)
    threads = cpu_threads()
    results = [0] * threads
    workers = [threading.Thread(target=_hash_worker, args=(seconds, block, results, i)) for i in range(threads)]
    start = time.perf_counter()
    for w in workers:
        w.start()
    for w in workers:
        w.join()
    multi_mbs = sum(results) / MiB / (time.perf_counter() - start)
    return {
        "threads": threads,
        "single_mbs": round(single_mbs, 1),
        "multi_mbs": round(multi_mbs, 1),
        "score": score("cpu", multi_mbs),
        "detail": f"{threads} threads · {fr_number(multi_mbs)} Mo/s (1 cœur : {fr_number(single_mbs)} Mo/s)",
    }


def mem_total():
    try:
        with open("/proc/meminfo") as f:
            for line in f:
                if line.startswith("MemTotal:"):
                    return int(line.split()[1]) * 1024
    except OSError:
        pass
    return 0


def mem_available():
    try:
        with open("/proc/meminfo") as f:
            for line in f:
                if line.startswith("MemAvailable:"):
                    return int(line.split()[1]) * 1024
    except OSError:
        pass
    return 0


def bench_ram(seconds=1.0):
    total = mem_total()
    available = mem_available() or total
    size = max(16 * MiB, min(256 * MiB, available // 8))
    src = bytearray(size)
    dst = bytearray(size)
    src_view, dst_view = memoryview(src), memoryview(dst)
    dst_view[:] = src_view
    copied = 0
    start = time.perf_counter()
    while True:
        dst_view[:] = src_view
        copied += size
        elapsed = time.perf_counter() - start
        if elapsed >= seconds:
            break
    # Une copie lit et écrit : on compte les deux sens comme le font les outils usuels.
    gbs = 2 * copied / GiB / elapsed
    return {
        "total_bytes": total,
        "bandwidth_gbs": round(gbs, 2),
        "score": score("ram", gbs),
        "detail": f"{fr_number(total / GiB, 1)} Go · {fr_number(gbs, 1)} Go/s",
    }


def block_info(device):
    name = os.path.basename(os.path.realpath(device))
    info = {"name": name, "rotational": None, "kind": "inconnu"}
    try:
        with open(f"/sys/block/{name}/queue/rotational") as f:
            info["rotational"] = f.read().strip() == "1"
    except OSError:
        pass
    if name.startswith("nvme"):
        info["kind"] = "NVMe"
    elif info["rotational"]:
        info["kind"] = "HDD"
    elif info["rotational"] is False:
        info["kind"] = "SSD"
    return info


def _read_direct(device, seconds, limit):
    block = 4 * MiB
    buf = mmap.mmap(-1, block)
    fd = os.open(device, os.O_RDONLY | getattr(os, "O_DIRECT", 0))
    try:
        done = 0
        start = time.perf_counter()
        while done < limit and time.perf_counter() - start < seconds:
            n = os.readv(fd, [buf])
            if n <= 0:
                break
            done += n
        return done, time.perf_counter() - start
    finally:
        os.close(fd)
        buf.close()


def _read_buffered(device, seconds, limit):
    fd = os.open(device, os.O_RDONLY)
    try:
        try:
            os.posix_fadvise(fd, 0, 0, os.POSIX_FADV_DONTNEED)
        except (AttributeError, OSError):
            pass
        done = 0
        start = time.perf_counter()
        while done < limit and time.perf_counter() - start < seconds:
            chunk = os.read(fd, 4 * MiB)
            if not chunk:
                break
            done += len(chunk)
        return done, time.perf_counter() - start
    finally:
        os.close(fd)


def bench_disk_read(device, seconds=2.0, limit=2 * GiB):
    """Lecture seule : aucun octet n'est écrit sur le disque choisi."""
    info = block_info(device)
    try:
        done, elapsed = _read_direct(device, seconds, limit)
    except OSError:
        done, elapsed = _read_buffered(device, seconds, limit)
    mbs = done / MiB / elapsed if elapsed > 0 else 0.0
    return {
        "device": device,
        "type": info["kind"],
        "rotational": info["rotational"],
        "read_mbs": round(mbs, 1),
        "score": score("disk", mbs),
        "detail": f"{info['kind']} · lecture {fr_number(mbs)} Mo/s",
    }


def bench_disk_write(directory, seconds=2.0, limit=512 * MiB):
    """Écriture séquentielle dans un fichier temporaire, fsync compris, puis suppression."""
    path = os.path.join(directory, f".winlux-bench-{os.getpid()}")
    block = os.urandom(4 * MiB)
    done = 0
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    try:
        start = time.perf_counter()
        while done < limit and time.perf_counter() - start < seconds:
            done += os.write(fd, block)
        os.fsync(fd)
        elapsed = time.perf_counter() - start
    finally:
        os.close(fd)
        try:
            os.unlink(path)
        except OSError:
            pass
    mbs = done / MiB / elapsed if elapsed > 0 else 0.0
    return {"write_mbs": round(mbs, 1), "detail": f"écriture {fr_number(mbs)} Mo/s"}


def _read(path):
    try:
        with open(path) as f:
            return f.read().strip()
    except OSError:
        return ""


def detect_gpus(sysfs="/sys"):
    gpus = []
    drm = os.path.join(sysfs, "class/drm")
    try:
        cards = sorted(n for n in os.listdir(drm) if n.startswith("card") and "-" not in n)
    except OSError:
        cards = []
    for card in cards:
        dev = os.path.join(drm, card, "device")
        vendor = _read(os.path.join(dev, "vendor")).lower()
        driver = os.path.basename(os.path.realpath(os.path.join(dev, "driver"))) if os.path.exists(os.path.join(dev, "driver")) else ""
        vram = _read(os.path.join(dev, "mem_info_vram_total"))
        gpus.append({
            "card": card,
            "vendor": vendor,
            "driver": driver,
            "vram_bytes": int(vram) if vram.isdigit() else 0,
        })
    return gpus


def gpu_renderer():
    if not shutil.which("glxinfo") or not os.environ.get("DISPLAY"):
        return ""
    try:
        out = subprocess.run(["glxinfo", "-B"], capture_output=True, text=True, timeout=5).stdout
    except (OSError, subprocess.SubprocessError):
        return ""
    for line in out.splitlines():
        if "OpenGL renderer string" in line:
            return line.split(":", 1)[1].strip()
    return ""


def gpu_tier(gpu, renderer=""):
    """Estimation d'après le modèle : aucun test 3D n'est lancé pendant l'installation."""
    if any(s in renderer.lower() for s in ("llvmpipe", "softpipe", "swrast")):
        return 1.5, "rendu logiciel"
    vendor, vram = gpu["vendor"], gpu["vram_bytes"]
    if vendor in VIRTUAL_GPU_VENDORS or gpu["driver"] in ("virtio-pci", "virtio_gpu", "bochs-drm", "vmwgfx", "qxl", "simpledrm"):
        return 2.0, "adaptateur virtuel"
    if vendor == "0x8086":
        return 4.5, "graphique intégré"
    if vendor in ("0x10de", "0x1002"):
        if vram >= 12 * GiB:
            return 9.0, "carte dédiée"
        if vram >= 8 * GiB:
            return 8.0, "carte dédiée"
        if vram >= 4 * GiB:
            return 7.0, "carte dédiée"
        if vram >= 1 * GiB:
            return 6.0, "carte dédiée"
        if vendor == "0x1002" and vram:
            return 4.5, "graphique intégré"
        return 6.5, "carte dédiée"
    return 3.0, "adaptateur inconnu"


def bench_gpu(sysfs="/sys"):
    gpus = detect_gpus(sysfs)
    if not gpus:
        return {"available": False, "score": 1.0, "detail": "aucun adaptateur graphique détecté", "kind": "aucun"}
    renderer = gpu_renderer()
    rated = [(gpu_tier(g, renderer), g) for g in gpus]
    (value, kind), best = max(rated, key=lambda item: item[0][0])
    name = GPU_VENDOR_NAMES.get(best["vendor"], best["driver"] or best["vendor"] or "?")
    vram = f" · {fr_number(best['vram_bytes'] / GiB, 0)} Go VRAM" if best["vram_bytes"] else ""
    return {
        "available": True,
        "vendor": best["vendor"],
        "driver": best["driver"],
        "vram_bytes": best["vram_bytes"],
        "renderer": renderer,
        "kind": kind,
        "score": value,
        "estimated": True,
        "detail": f"{name} · {kind}{vram} (estimation)",
    }


def recommend(results):
    """Conseils et profil suggéré à partir des mesures."""
    tips = []
    cpu = results.get("cpu", {})
    ram = results.get("ram", {})
    disk = results.get("disk", {})
    gpu = results.get("gpu", {})
    total = ram.get("total_bytes", 0)
    if total and total < 4 * GiB:
        tips.append("Mémoire faible : compression zram activée et effets visuels réduits conseillés.")
    elif total >= 16 * GiB:
        tips.append("Mémoire abondante : swappiness basse (10) pour garder les applications en RAM.")
    if disk.get("type") == "HDD":
        tips.append("Disque mécanique : ordonnanceur d'E/S BFQ pour garder le bureau réactif.")
    elif disk.get("type") == "NVMe":
        tips.append("SSD NVMe : ordonnanceur d'E/S « none » pour une latence minimale.")
    elif disk.get("type") == "SSD":
        tips.append("SSD : ordonnanceur d'E/S « mq-deadline ».")
    if disk.get("read_mbs") and disk["read_mbs"] < 100:
        tips.append("Disque lent : installation et démarrage plus longs, un SSD est conseillé.")
    if gpu.get("score", 1.0) < 3.0:
        tips.append("Accélération graphique limitée : réduisez les animations du bureau.")
    if cpu.get("threads", 1) >= 8:
        tips.append(f"Processeur {cpu['threads']} threads : compilation parallèle efficace (make -j{cpu['threads']}).")
    if gpu.get("score", 0) >= 6.0:
        profile = "gaming"
    elif cpu.get("threads", 1) >= 8 and total >= 16 * GiB:
        profile = "dev"
    elif not gpu.get("available", True):
        profile = "server"
    else:
        profile = "office"
    return tips, profile


def run_all(device=None, quick=False, emit=None, sysfs="/sys"):
    factor = 0.2 if quick else 1.0
    results = {}

    def step(key, func):
        if emit:
            emit({"event": "start", "test": key})
        try:
            results[key] = func()
        except Exception as e:  # un composant en échec ne doit pas bloquer l'installation
            results[key] = {"score": 1.0, "detail": f"test impossible : {e}", "error": str(e)}
        if emit:
            emit({"event": "result", "test": key, "result": results[key]})

    step("cpu", lambda: bench_cpu(1.5 * factor))
    step("ram", lambda: bench_ram(1.0 * factor))
    if device:
        step("disk", lambda: bench_disk_read(device, 2.0 * factor, (2 * GiB) if not quick else 64 * MiB))
    step("gpu", lambda: bench_gpu(sysfs))
    scores = [r["score"] for r in results.values() if "score" in r]
    tips, profile = recommend(results)
    summary = {
        "base_score": min(scores) if scores else 1.0,
        "results": results,
        "recommendations": tips,
        "recommended_profile": profile,
        "date": time.strftime("%Y-%m-%d %H:%M:%S"),
    }
    if emit:
        emit({"event": "done", "summary": summary})
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--device", help="disque à mesurer en lecture seule (ex. /dev/sda)")
    parser.add_argument("--write-dir", help="dossier où mesurer l'écriture (fichier temporaire supprimé)")
    parser.add_argument("--quick", action="store_true", help="durées réduites (tests)")
    parser.add_argument("--json-lines", action="store_true", help="une ligne JSON par étape")
    args = parser.parse_args()

    if args.write_dir:
        print(json.dumps(bench_disk_write(args.write_dir, 0.4 if args.quick else 2.0), ensure_ascii=False))
        return 0

    def emit(event):
        if args.json_lines:
            print(json.dumps(event, ensure_ascii=False), flush=True)

    summary = run_all(args.device, args.quick, emit)
    if not args.json_lines:
        labels = {"cpu": "Processeur", "ram": "Mémoire", "disk": "Disque", "gpu": "Graphique"}
        for key, result in summary["results"].items():
            print(f"{labels[key]:<12} {fr_number(result['score'], 1):>4}  {result['detail']}")
        print(f"{'Note globale':<12} {fr_number(summary['base_score'], 1):>4}")
        for tip in summary["recommendations"]:
            print(f"  • {tip}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
