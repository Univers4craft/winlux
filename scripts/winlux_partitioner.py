"""Partitionnement manuel de l'installateur Winlux.

Modèle en mémoire d'un disque (table, partitions existantes et nouvelles) sur
lequel l'utilisateur prépare ses modifications. Rien n'est écrit tant que
l'installation n'est pas lancée : build_steps() produit alors la liste exacte
des commandes (parted, sfdisk, resize2fs, ntfsresize, mkfs) à exécuter.
"""
import json
import math
import os
import re
import subprocess
from dataclasses import dataclass, field

MIB = 1024 * 1024
GIB_MIB = 1024

ESP_GUID = "C12A7328-F81F-11D2-BA4B-00A0C93EC93B"
BIOS_GRUB_GUID = "21686148-6449-6E6F-744E-656564454649"
SWAP_GUID = "0657FD6D-A4AB-43C4-84E5-0933C84B4F4F"

FS_CHOICES = ["ext4", "xfs", "btrfs", "vfat", "swap", "ntfs", "bios_grub", "none"]
MOUNT_CHOICES = ["/", "/home", "/boot/efi", "/var", "/srv", "/opt", "/tmp"]
NO_MOUNT_FS = {"swap", "bios_grub", "none"}
SHRINKABLE_FS = {"ext2", "ext3", "ext4", "ntfs"}
GROWABLE_FS = {"ext2", "ext3", "ext4", "ntfs"}
PARTED_FS_HINT = {"ext4": "ext4", "xfs": "xfs", "btrfs": "btrfs", "vfat": "fat32",
                  "swap": "linux-swap", "ntfs": "ntfs"}
MKFS = {
    "ext4": lambda p, label: ["mkfs.ext4", "-F"] + (["-L", label] if label else []) + [p],
    "xfs": lambda p, label: ["mkfs.xfs", "-f"] + (["-L", label] if label else []) + [p],
    "btrfs": lambda p, label: ["mkfs.btrfs", "-f"] + (["-L", label] if label else []) + [p],
    "vfat": lambda p, label: ["mkfs.fat", "-F32"] + (["-n", label[:11].upper()] if label else []) + [p],
    "swap": lambda p, label: ["mkswap"] + (["-L", label] if label else []) + [p],
    "ntfs": lambda p, label: ["mkfs.ntfs", "-Q", "-F"] + (["-L", label] if label else []) + [p],
}
MIN_ROOT_MIB = 10 * GIB_MIB
MIN_ESP_MIB = 100
MSDOS_MAX_PRIMARY = 4
GPT_MAX = 128


def partition_path(device, number):
    sep = "p" if device[-1:].isdigit() else ""
    return f"{device}{sep}{number}"


@dataclass
class Part:
    start: float
    size: float
    number: int
    fstype: str = ""
    label: str = ""
    existing: bool = False
    orig_size: float = 0.0
    format: bool = True
    mountpoint: str = ""
    flags: set = field(default_factory=set)
    busy: bool = False
    orig_fstype: str = ""

    @property
    def end(self):
        return self.start + self.size

    def resized(self):
        return self.existing and abs(self.size - self.orig_size) >= 1

    def modified(self):
        return (not self.existing) or self.format or self.resized()


class Plan:
    """État prévu d'un disque après application des modifications."""

    def __init__(self, device, disk_bytes, label=None, parts=None, sector_size=512):
        self.device = device
        self.disk_bytes = disk_bytes
        self.sector_size = sector_size
        self.label = label
        self.orig_label = label
        self.new_table = False
        self.deleted = []
        self.parts = sorted(parts or [], key=lambda p: p.start)
        self._orig = [(p.number, p.start, p.size, p.fstype, p.mountpoint, p.format, set(p.flags))
                      for p in self.parts]

    # ── Géométrie ────────────────────────────────────────────────

    @property
    def first_mib(self):
        return 1

    @property
    def last_mib(self):
        total = self.disk_bytes // MIB
        return total - 1 if self.label == "gpt" or self.label is None else total

    def free_regions(self):
        """Zones libres (start, size) en MiB entiers, alignées sur 1 MiB."""
        regions = []
        cursor = self.first_mib
        for p in sorted(self.parts, key=lambda q: q.start):
            start = math.floor(p.start)
            if start - cursor >= 1:
                regions.append((cursor, start - cursor))
            cursor = max(cursor, math.ceil(p.end))
        if self.last_mib - cursor >= 1:
            regions.append((cursor, self.last_mib - cursor))
        return regions

    def largest_free(self):
        regions = self.free_regions()
        return max(regions, key=lambda r: r[1]) if regions else None

    def max_size(self, part):
        """Taille maximale possible pour une partition (agrandissement vers la droite)."""
        nxt = self.last_mib
        for q in self.parts:
            if q is not part and q.start >= part.end - 0.5:
                nxt = min(nxt, math.floor(q.start))
        return max(part.size, nxt - part.start)

    def layout(self):
        """Partitions et zones libres dans l'ordre du disque (pour l'affichage)."""
        items = [("part", p) for p in self.parts]
        items += [("free", r) for r in self.free_regions()]
        return sorted(items, key=lambda it: it[1].start if it[0] == "part" else it[1][0])

    # ── Opérations ───────────────────────────────────────────────

    def _next_number(self):
        used = {p.number for p in self.parts}
        limit = MSDOS_MAX_PRIMARY if self.label == "msdos" else GPT_MAX
        for n in range(1, limit + 1):
            if n not in used:
                return n
        return None

    def new_partition_table(self, label):
        if label not in ("gpt", "msdos"):
            raise ValueError(label)
        if any(p.busy for p in self.parts if p.existing):
            raise PlanError("err_busy_table")
        self.deleted = []
        self.parts = []
        self.label = label
        self.new_table = True

    def delete(self, part):
        if part.busy:
            raise PlanError("err_busy", path=self.path_of(part))
        self.parts.remove(part)
        if part.existing and not self.new_table:
            self.deleted.append(part.number)

    def create(self, start, size, fstype, mountpoint="", label=""):
        if self.label is None:
            raise PlanError("err_no_table")
        start, size = int(start), int(size)
        if size < 1:
            raise PlanError("err_size")
        for region_start, region_size in self.free_regions():
            if region_start <= start and start + size <= region_start + region_size:
                break
        else:
            raise PlanError("err_no_space")
        number = self._next_number()
        if number is None:
            raise PlanError("err_too_many", limit=MSDOS_MAX_PRIMARY)
        label = re.sub(r"[^A-Za-z0-9_-]", "", label or "")[:32]
        flags = set()
        if fstype == "bios_grub":
            flags.add("bios_grub")
        if mountpoint == "/boot/efi":
            flags.add("esp")
        part = Part(start=start, size=size, number=number, fstype=fstype, label=label,
                    format=fstype not in ("bios_grub", "none"),
                    mountpoint="" if fstype in NO_MOUNT_FS else mountpoint, flags=flags)
        self.parts.append(part)
        self.parts.sort(key=lambda p: p.start)
        return part

    def resize(self, part, new_size):
        new_size = int(new_size)
        if part.busy:
            raise PlanError("err_busy", path=self.path_of(part))
        if new_size < 1 or new_size > self.max_size(part) + 0.5:
            raise PlanError("err_no_space")
        if part.existing and not part.format:
            if new_size < part.orig_size and part.fstype not in SHRINKABLE_FS:
                raise PlanError("err_cannot_shrink", fs=part.fstype or "?")
            if new_size > part.orig_size and part.fstype not in GROWABLE_FS:
                raise PlanError("err_cannot_grow", fs=part.fstype or "?")
        part.size = new_size if (not part.existing or abs(new_size - part.orig_size) >= 1) else part.orig_size

    def configure(self, part, fstype=None, format=None, mountpoint=None, label=None):
        if part.busy and (format or (mountpoint not in (None, ""))):
            raise PlanError("err_busy", path=self.path_of(part))
        if format is not None:
            part.format = bool(format) or not part.existing
        if fstype is not None and (part.format or not part.existing):
            part.fstype = fstype
        if part.existing and not part.format:
            part.fstype = part.orig_fstype
        if label is not None:
            part.label = re.sub(r"[^A-Za-z0-9_-]", "", label)[:32]
        if mountpoint is not None:
            part.mountpoint = "" if part.fstype in NO_MOUNT_FS else mountpoint
        part.flags.discard("esp")
        part.flags.discard("bios_grub")
        if part.mountpoint == "/boot/efi":
            part.flags.add("esp")
        if part.fstype == "bios_grub":
            part.flags.add("bios_grub")
            part.format = False

    def ensure_boot_partitions(self):
        """Ajoute ce qu'il faut pour que GRUB s'installe en BIOS et en UEFI."""
        added = []
        if self.label is None:
            self.new_partition_table("gpt")
        if self.label == "gpt" and not any(p.fstype == "bios_grub" or "bios_grub" in p.flags
                                           for p in self.parts):
            region = next((r for r in self.free_regions() if r[1] >= 1), None)
            if region is None:
                raise PlanError("err_no_space")
            added.append(self.create(region[0], 1 if region[1] < 2 else 2, "bios_grub"))
        if not any(p.mountpoint == "/boot/efi" for p in self.parts):
            existing_esp = next((p for p in self.parts if p.existing and "esp" in p.flags
                                 and p.fstype == "vfat"), None)
            if existing_esp is not None:
                self.configure(existing_esp, format=False, mountpoint="/boot/efi")
                added.append(existing_esp)
            else:
                region = next((r for r in self.free_regions() if r[1] >= 300), None)
                if region is None:
                    raise PlanError("err_no_space")
                added.append(self.create(region[0], min(512, region[1]), "vfat",
                                         mountpoint="/boot/efi", label="ESP"))
        return added

    # ── Résolution ───────────────────────────────────────────────

    def path_of(self, part):
        return partition_path(self.device, part.number)

    def root(self):
        return next((p for p in self.parts if p.mountpoint == "/"), None)

    def esp(self):
        return next((p for p in self.parts if p.mountpoint == "/boot/efi"), None)

    def mounts(self):
        """Points de montage hors racine et ESP, du moins profond au plus profond."""
        return sorted((p for p in self.parts if p.mountpoint not in ("", "/", "/boot/efi")),
                      key=lambda p: p.mountpoint.count("/"))

    def has_changes(self):
        current = [(p.number, p.start, p.size, p.fstype, p.mountpoint, p.format, set(p.flags))
                   for p in self.parts]
        return self.new_table or bool(self.deleted) or current != self._orig

    # ── Validation ───────────────────────────────────────────────

    def validate(self):
        """Liste de (clé de traduction, paramètres). Vide = plan applicable."""
        errors = []
        if self.label is None:
            return [("err_no_table", {})]
        roots = [p for p in self.parts if p.mountpoint == "/"]
        if not roots:
            errors.append(("err_no_root", {}))
        elif len(roots) == 1:
            root = roots[0]
            if not root.format or root.fstype != "ext4":
                errors.append(("err_root_format", {}))
            if root.size < MIN_ROOT_MIB:
                errors.append(("err_root_small", {"size": MIN_ROOT_MIB // GIB_MIB}))
        seen = {}
        for p in self.parts:
            if not p.mountpoint:
                continue
            if not p.mountpoint.startswith("/") or "//" in p.mountpoint or ".." in p.mountpoint:
                errors.append(("err_mount_invalid", {"mount": p.mountpoint}))
            if p.mountpoint in seen:
                errors.append(("err_mount_dup", {"mount": p.mountpoint}))
            seen[p.mountpoint] = p
            if p.mountpoint == "/boot":
                errors.append(("err_boot_separate", {}))
            if not p.format and not p.fstype:
                errors.append(("err_mount_nofs", {"mount": p.mountpoint}))
        esp = seen.get("/boot/efi")
        if esp is None:
            errors.append(("err_no_esp", {}))
        else:
            if esp.fstype != "vfat":
                errors.append(("err_esp_fs", {}))
            if esp.size < MIN_ESP_MIB:
                errors.append(("err_esp_small", {"size": MIN_ESP_MIB}))
        if self.label == "gpt":
            if not any(p.fstype == "bios_grub" or "bios_grub" in p.flags for p in self.parts):
                errors.append(("err_no_bios_grub", {}))
        else:
            if any(p.fstype == "bios_grub" for p in self.parts):
                errors.append(("err_bios_grub_msdos", {}))
            if len(self.parts) > MSDOS_MAX_PRIMARY:
                errors.append(("err_too_many", {"limit": MSDOS_MAX_PRIMARY}))
        for p in self.parts:
            if p.busy and p.modified():
                errors.append(("err_busy", {"path": self.path_of(p)}))
            if p.resized() and not p.format:
                if p.size < p.orig_size and p.fstype not in SHRINKABLE_FS:
                    errors.append(("err_cannot_shrink", {"fs": p.fstype or "?"}))
                if p.size > p.orig_size and p.fstype not in GROWABLE_FS:
                    errors.append(("err_cannot_grow", {"fs": p.fstype or "?"}))
        ordered = sorted(self.parts, key=lambda q: q.start)
        for a, b in zip(ordered, ordered[1:]):
            if a.end > b.start + 0.01:
                errors.append(("err_overlap", {"a": self.path_of(a), "b": self.path_of(b)}))
        if ordered and ordered[-1].end > self.last_mib + 0.5:
            errors.append(("err_no_space", {}))
        return errors

    # ── Description et commandes ─────────────────────────────────

    def describe(self):
        """Opérations prévues : liste de (destructive, clé, paramètres)."""
        ops = []
        if self.new_table:
            ops.append((True, "op_new_table", {"label": self.label.upper(), "device": self.device}))
        else:
            for n in sorted(self.deleted):
                ops.append((True, "op_delete", {"path": partition_path(self.device, n)}))
        for p in self.parts:
            path = self.path_of(p)
            if p.existing and p.resized():
                key = "op_shrink" if p.size < p.orig_size else "op_grow"
                ops.append((p.size < p.orig_size, key, {
                    "path": path, "old": human_mib(p.orig_size), "new": human_mib(p.size)}))
            if not p.existing:
                ops.append((False, "op_create", {
                    "path": path, "size": human_mib(p.size), "fs": p.fstype,
                    "mount": p.mountpoint or "-"}))
            elif p.format and p.fstype not in ("bios_grub", "none"):
                ops.append((True, "op_format", {"path": path, "fs": p.fstype}))
            if p.mountpoint and p.existing and not p.format:
                ops.append((False, "op_mount", {"path": path, "mount": p.mountpoint}))
        return ops

    def build_steps(self):
        """Commandes de modification de la table, dans un ordre sûr.

        Suppressions, puis réductions (système de fichiers avant partition),
        créations, agrandissements (partition avant système de fichiers).
        Chaque étape : (argv, stdin ou None).
        """
        dev = self.device
        steps = []
        if self.new_table:
            steps.append((["parted", "-s", dev, "--", "mklabel", self.label], None))
        else:
            for n in sorted(self.deleted, reverse=True):
                steps.append((["parted", "-s", dev, "--", "rm", str(n)], None))
        for p in self.parts:
            if p.existing and p.resized() and p.size < p.orig_size:
                steps += self._fs_resize(p, shrink=True)
                steps.append(self._sfdisk_resize(p))
        for p in sorted((q for q in self.parts if not q.existing), key=lambda q: q.number):
            start = int(p.start)
            end = int(p.start + p.size)
            kind = "primary" if self.label == "msdos" else (p.label or "WINLUX")
            argv = ["parted", "-s", dev, "--", "mkpart", kind]
            hint = PARTED_FS_HINT.get(p.fstype)
            if hint:
                argv.append(hint)
            argv += [f"{start}MiB", f"{end}MiB"]
            steps.append((argv, None))
        for p in self.parts:
            if p.existing and p.resized() and p.size > p.orig_size:
                steps.append(self._sfdisk_resize(p))
                steps.append((["partprobe", dev], None))
                steps += self._fs_resize(p, shrink=False)
        for p in self.parts:
            if "esp" in p.flags:
                steps.append((["parted", "-s", dev, "--", "set", str(p.number), "esp", "on"], None))
            if "bios_grub" in p.flags and self.label == "gpt":
                steps.append((["parted", "-s", dev, "--", "set", str(p.number), "bios_grub", "on"], None))
        steps.append((["partprobe", dev], None))
        steps.append((["udevadm", "settle", "--timeout=10"], None))
        return steps

    def _sfdisk_resize(self, p):
        return (["sfdisk", "--no-reread", "--no-tell-kernel", "-N", str(p.number), self.device],
                f",{int(p.size)}MiB\n")

    def _fs_resize(self, p, shrink):
        path = self.path_of(p)
        if p.format:
            return []
        if p.fstype in ("ext2", "ext3", "ext4"):
            return [(["e2fsck", "-f", "-y", path], None),
                    (["resize2fs", path] + ([f"{int(p.size) - 1}M"] if shrink else []), None)]
        if p.fstype == "ntfs":
            return [(["ntfsresize", "--force"]
                     + (["--size", f"{int(p.size) - 1}M"] if shrink else []) + [path], "y\n")]
        return []

    def format_steps(self, skip=()):
        """Commandes mkfs pour les partitions à formater (sauf celles de skip)."""
        steps = []
        for p in self.parts:
            if not p.format or p in skip or p.fstype not in MKFS:
                continue
            steps.append((MKFS[p.fstype](self.path_of(p), p.label), None))
        return steps

    def fstab(self, uuid_of, root_luks=False):
        """Contenu de /etc/fstab : uuid_of(path) -> UUID du système de fichiers."""
        lines = ["# /etc/fstab — généré par l'installateur Winlux",
                 "# <fs> <point de montage> <type> <options> <dump> <pass>"]
        root = self.root()
        if root is not None:
            src = "/dev/mapper/winlux_crypt" if root_luks else f"UUID={uuid_of(self.path_of(root))}"
            lines.append(f"{src} / ext4 defaults 0 1")
        for p in sorted(self.parts, key=lambda q: (q.mountpoint.count("/"), q.mountpoint)):
            if p is root:
                continue
            uuid = uuid_of(self.path_of(p))
            if not uuid:
                continue
            if p.fstype == "swap":
                lines.append(f"UUID={uuid} none swap sw 0 0")
            elif p.mountpoint == "/boot/efi":
                lines.append(f"UUID={uuid} /boot/efi vfat umask=0077 0 2")
            elif p.mountpoint:
                fs = "ntfs-3g" if p.fstype == "ntfs" else p.fstype
                lines.append(f"UUID={uuid} {p.mountpoint} {fs} defaults 0 2")
        return "\n".join(lines) + "\n"


class PlanError(Exception):
    def __init__(self, key, **kwargs):
        super().__init__(key)
        self.key = key
        self.kwargs = kwargs


def human_mib(mib):
    if mib >= 1024 * 1024:
        return f"{mib / (1024 * 1024):.1f} To"
    if mib >= 1024:
        return f"{mib / 1024:.1f} Go"
    return f"{int(mib)} Mo"


# ── Lecture du disque ────────────────────────────────────────────

def _run_json(argv):
    try:
        out = subprocess.run(argv, capture_output=True, text=True, timeout=15)
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0 or not out.stdout.strip():
        return None
    try:
        return json.loads(out.stdout)
    except ValueError:
        return None


def parse_disk(device, disk_bytes, sfdisk_json, lsblk_json):
    """Construit un Plan à partir des sorties JSON de sfdisk et lsblk."""
    fsinfo = {}
    for bd in (lsblk_json or {}).get("blockdevices", []):
        for child in bd.get("children", []) or []:
            fsinfo["/dev/" + child.get("name", "")] = child
    table = (sfdisk_json or {}).get("partitiontable")
    if not table:
        return Plan(device, disk_bytes, label=None)
    label = {"dos": "msdos"}.get(table.get("label"), table.get("label"))
    ss = int(table.get("sectorsize") or 512)
    parts = []
    for entry in table.get("partitions", []):
        node = entry.get("node", "")
        m = re.search(r"(\d+)$", node)
        if not m:
            continue
        ptype = str(entry.get("type", "")).upper()
        if label == "msdos" and ptype in ("5", "F", "85"):
            continue
        info = fsinfo.get(node, {})
        fstype = (info.get("fstype") or "").strip()
        flags = set()
        if ptype in (ESP_GUID, "EF"):
            flags.add("esp")
        if ptype == BIOS_GRUB_GUID:
            flags.add("bios_grub")
            fstype = fstype or "bios_grub"
        if fstype == "swap" or ptype in (SWAP_GUID, "82"):
            fstype = "swap"
        size = int(entry.get("size", 0)) * ss / MIB
        mounted = info.get("mountpoint") or ""
        if not mounted:
            mps = [m for m in (info.get("mountpoints") or []) if m]
            mounted = mps[0] if mps else ""
        parts.append(Part(
            start=int(entry.get("start", 0)) * ss / MIB, size=size, number=int(m.group(1)),
            fstype=fstype, label=(info.get("label") or "").strip(), existing=True,
            orig_size=size, format=False, flags=flags, orig_fstype=fstype,
            busy=bool(mounted) or fstype == "swap" and _swap_active(node)))
    return Plan(device, disk_bytes, label=label, parts=parts, sector_size=ss)


def _swap_active(node):
    try:
        with open("/proc/swaps") as f:
            return any(line.split()[0] == node for line in f.readlines()[1:] if line.strip())
    except OSError:
        return False


def read_disk(device, disk_bytes):
    sf = _run_json(["sfdisk", "--json", device])
    lb = _run_json(["lsblk", "-bJo", "NAME,FSTYPE,LABEL,MOUNTPOINT", device])
    return parse_disk(device, disk_bytes, sf, lb)


def blkid_uuid(path):
    try:
        return subprocess.check_output(["blkid", "-s", "UUID", "-o", "value", path],
                                       text=True, timeout=10).strip()
    except (OSError, subprocess.SubprocessError):
        return ""


def wait_for_partitions(plan, timeout=10.0):
    import time
    deadline = time.time() + timeout
    paths = [plan.path_of(p) for p in plan.parts]
    while time.time() < deadline:
        if all(os.path.exists(p) for p in paths):
            return True
        time.sleep(0.25)
    return all(os.path.exists(p) for p in paths)


def run_steps(steps, log=None):
    for argv, stdin in steps:
        if log:
            log(" ".join(argv))
        subprocess.run(argv, input=stdin, text=True, check=True, timeout=1800,
                       capture_output=True)


# ── Traductions ──────────────────────────────────────────────────

TRANSLATIONS = {
    "fr_FR": {
        "pm_title": "Partitionnement manuel",
        "pm_hint": "Les modifications ne sont écrites sur le disque qu'au clic sur « Installer ».",
        "pm_col_device": "Partition",
        "pm_col_type": "Système de fichiers",
        "pm_col_size": "Taille",
        "pm_col_mount": "Point de montage",
        "pm_col_action": "Action",
        "pm_free": "Espace libre",
        "pm_new_table": "Nouvelle table…",
        "pm_create": "Créer…",
        "pm_edit": "Modifier…",
        "pm_delete": "Supprimer",
        "pm_revert": "Annuler les modifications",
        "pm_auto_boot": "Ajouter les partitions de démarrage",
        "pm_status_keep": "Conserver",
        "pm_status_new": "Nouvelle",
        "pm_status_format": "Formater",
        "pm_status_resize": "Redimensionner",
        "pm_status_busy": "En cours d'utilisation",
        "pm_valid": "Plan de partitionnement valide.",
        "pm_no_disk": "Choisissez d'abord un disque.",
        "pm_dlg_create": "Créer une partition",
        "pm_dlg_edit": "Modifier {path}",
        "pm_dlg_size": "Taille (Mo) :",
        "pm_dlg_fs": "Système de fichiers :",
        "pm_dlg_mount": "Point de montage :",
        "pm_dlg_label": "Nom (étiquette) :",
        "pm_dlg_format": "Formater cette partition (efface son contenu)",
        "pm_dlg_ok": "Valider",
        "pm_dlg_cancel": "Annuler",
        "pm_dlg_max": "Maximum disponible : {size}",
        "pm_dlg_table": "Créer une nouvelle table de partitions",
        "pm_dlg_table_warn": "Toutes les partitions de {device} seront supprimées à l'installation.",
        "pm_dlg_table_gpt": "GPT (recommandé, UEFI et BIOS)",
        "pm_dlg_table_msdos": "MBR / DOS (anciens PC, 4 partitions maximum)",
        "pm_confirm_delete": "Supprimer {path} ? Son contenu sera perdu à l'installation.",
        "pm_fs_none": "Aucun (non formaté)",
        "pm_fs_bios_grub": "Amorçage BIOS (bios_grub)",
        "pm_fs_swap": "Mémoire d'échange (swap)",
        "pm_fs_vfat": "FAT32 (partition EFI)",
        "pm_mount_none": "(aucun)",
        "pm_ops_title": "Modifications du disque :",
        "pm_confirm_title": "Confirmer les modifications du disque",
        "pm_confirm_text": "Les opérations suivantes vont être appliquées à {device}. "
                           "Les opérations marquées ⚠ détruisent des données de façon irréversible.",
        "pm_confirm_check": "J'ai sauvegardé mes données et je veux appliquer ces modifications",
        "pm_confirm_apply": "Appliquer et installer",
        "op_new_table": "⚠ Nouvelle table {label} sur {device} (efface toutes les partitions)",
        "op_delete": "⚠ Supprimer {path}",
        "op_shrink": "⚠ Réduire {path} : {old} → {new}",
        "op_grow": "Agrandir {path} : {old} → {new}",
        "op_create": "Créer {path} : {fs}, {size}, montage {mount}",
        "op_format": "⚠ Formater {path} en {fs}",
        "op_mount": "Utiliser {path} sur {mount} (sans formater)",
        "err_no_table": "Le disque n'a pas de table de partitions : créez-en une.",
        "err_no_root": "Aucune partition n'est montée sur « / » (racine du système).",
        "err_root_format": "La partition racine « / » doit être formatée en ext4.",
        "err_root_small": "La partition racine doit faire au moins {size} Go.",
        "err_mount_invalid": "Point de montage invalide : {mount}",
        "err_mount_dup": "Point de montage utilisé deux fois : {mount}",
        "err_mount_nofs": "{mount} : la partition n'a pas de système de fichiers, cochez « Formater ».",
        "err_boot_separate": "Une partition /boot séparée n'est pas prise en charge.",
        "err_no_esp": "Il manque une partition EFI (FAT32 montée sur /boot/efi).",
        "err_esp_fs": "La partition /boot/efi doit être en FAT32 (vfat).",
        "err_esp_small": "La partition EFI doit faire au moins {size} Mo.",
        "err_no_bios_grub": "Sur un disque GPT, il faut une petite partition d'amorçage BIOS (bios_grub).",
        "err_bios_grub_msdos": "bios_grub n'existe que sur les disques GPT.",
        "err_too_many": "Une table MBR accepte au plus {limit} partitions.",
        "err_busy": "{path} est utilisée par le système live et ne peut pas être modifiée.",
        "err_busy_table": "Une partition du disque est utilisée par le système live.",
        "err_overlap": "{a} et {b} se chevauchent.",
        "err_no_space": "Pas assez d'espace libre à cet endroit.",
        "err_size": "Taille invalide.",
        "err_cannot_shrink": "Impossible de réduire une partition {fs} sans la formater.",
        "err_cannot_grow": "Impossible d'agrandir une partition {fs} sans la formater.",
        "progress_manual_apply": "Application du plan de partitionnement...",
        "progress_manual_format": "Formatage de {path} ({fs})...",
        "progress_manual_mount": "Montage de {path} sur {mount}...",
    },
    "en_US": {
        "pm_title": "Manual partitioning",
        "pm_hint": "Nothing is written to the disk until you click \"Install\".",
        "pm_col_device": "Partition",
        "pm_col_type": "File system",
        "pm_col_size": "Size",
        "pm_col_mount": "Mount point",
        "pm_col_action": "Action",
        "pm_free": "Free space",
        "pm_new_table": "New table…",
        "pm_create": "Create…",
        "pm_edit": "Edit…",
        "pm_delete": "Delete",
        "pm_revert": "Revert changes",
        "pm_auto_boot": "Add boot partitions",
        "pm_status_keep": "Keep",
        "pm_status_new": "New",
        "pm_status_format": "Format",
        "pm_status_resize": "Resize",
        "pm_status_busy": "In use",
        "pm_valid": "Partitioning plan is valid.",
        "pm_no_disk": "Choose a disk first.",
        "pm_dlg_create": "Create a partition",
        "pm_dlg_edit": "Edit {path}",
        "pm_dlg_size": "Size (MB):",
        "pm_dlg_fs": "File system:",
        "pm_dlg_mount": "Mount point:",
        "pm_dlg_label": "Name (label):",
        "pm_dlg_format": "Format this partition (erases its content)",
        "pm_dlg_ok": "OK",
        "pm_dlg_cancel": "Cancel",
        "pm_dlg_max": "Maximum available: {size}",
        "pm_dlg_table": "Create a new partition table",
        "pm_dlg_table_warn": "Every partition on {device} will be deleted during installation.",
        "pm_dlg_table_gpt": "GPT (recommended, UEFI and BIOS)",
        "pm_dlg_table_msdos": "MBR / DOS (older PCs, 4 partitions max)",
        "pm_confirm_delete": "Delete {path}? Its content will be lost during installation.",
        "pm_fs_none": "None (unformatted)",
        "pm_fs_bios_grub": "BIOS boot (bios_grub)",
        "pm_fs_swap": "Swap space",
        "pm_fs_vfat": "FAT32 (EFI partition)",
        "pm_mount_none": "(none)",
        "pm_ops_title": "Disk changes:",
        "pm_confirm_title": "Confirm disk changes",
        "pm_confirm_text": "The following operations will be applied to {device}. "
                           "Operations marked ⚠ irreversibly destroy data.",
        "pm_confirm_check": "I have backed up my data and want to apply these changes",
        "pm_confirm_apply": "Apply and install",
        "op_new_table": "⚠ New {label} table on {device} (erases every partition)",
        "op_delete": "⚠ Delete {path}",
        "op_shrink": "⚠ Shrink {path}: {old} → {new}",
        "op_grow": "Grow {path}: {old} → {new}",
        "op_create": "Create {path}: {fs}, {size}, mounted on {mount}",
        "op_format": "⚠ Format {path} as {fs}",
        "op_mount": "Use {path} on {mount} (no format)",
        "err_no_table": "The disk has no partition table: create one.",
        "err_no_root": "No partition is mounted on \"/\" (system root).",
        "err_root_format": "The root partition \"/\" must be formatted as ext4.",
        "err_root_small": "The root partition must be at least {size} GB.",
        "err_mount_invalid": "Invalid mount point: {mount}",
        "err_mount_dup": "Mount point used twice: {mount}",
        "err_mount_nofs": "{mount}: the partition has no file system, tick \"Format\".",
        "err_boot_separate": "A separate /boot partition is not supported.",
        "err_no_esp": "An EFI partition (FAT32 mounted on /boot/efi) is missing.",
        "err_esp_fs": "The /boot/efi partition must be FAT32 (vfat).",
        "err_esp_small": "The EFI partition must be at least {size} MB.",
        "err_no_bios_grub": "A GPT disk needs a small BIOS boot partition (bios_grub).",
        "err_bios_grub_msdos": "bios_grub only exists on GPT disks.",
        "err_too_many": "An MBR table holds at most {limit} partitions.",
        "err_busy": "{path} is used by the live system and cannot be changed.",
        "err_busy_table": "A partition on this disk is used by the live system.",
        "err_overlap": "{a} and {b} overlap.",
        "err_no_space": "Not enough free space there.",
        "err_size": "Invalid size.",
        "err_cannot_shrink": "A {fs} partition cannot be shrunk without formatting it.",
        "err_cannot_grow": "A {fs} partition cannot be grown without formatting it.",
        "progress_manual_apply": "Applying the partitioning plan...",
        "progress_manual_format": "Formatting {path} ({fs})...",
        "progress_manual_mount": "Mounting {path} on {mount}...",
    },
    "de_DE": {
        "pm_title": "Manuelle Partitionierung",
        "pm_hint": "Auf die Festplatte wird erst beim Klick auf „Installieren“ geschrieben.",
        "pm_col_device": "Partition",
        "pm_col_type": "Dateisystem",
        "pm_col_size": "Größe",
        "pm_col_mount": "Einhängepunkt",
        "pm_col_action": "Aktion",
        "pm_free": "Freier Speicher",
        "pm_new_table": "Neue Tabelle…",
        "pm_create": "Erstellen…",
        "pm_edit": "Bearbeiten…",
        "pm_delete": "Löschen",
        "pm_revert": "Änderungen verwerfen",
        "pm_auto_boot": "Boot-Partitionen hinzufügen",
        "pm_status_keep": "Behalten",
        "pm_status_new": "Neu",
        "pm_status_format": "Formatieren",
        "pm_status_resize": "Größe ändern",
        "pm_status_busy": "In Benutzung",
        "pm_valid": "Partitionierungsplan ist gültig.",
        "pm_no_disk": "Wählen Sie zuerst eine Festplatte.",
        "pm_dlg_create": "Partition erstellen",
        "pm_dlg_edit": "{path} bearbeiten",
        "pm_dlg_size": "Größe (MB):",
        "pm_dlg_fs": "Dateisystem:",
        "pm_dlg_mount": "Einhängepunkt:",
        "pm_dlg_label": "Name (Bezeichnung):",
        "pm_dlg_format": "Diese Partition formatieren (löscht den Inhalt)",
        "pm_dlg_ok": "OK",
        "pm_dlg_cancel": "Abbrechen",
        "pm_dlg_max": "Maximal verfügbar: {size}",
        "pm_dlg_table": "Neue Partitionstabelle erstellen",
        "pm_dlg_table_warn": "Alle Partitionen auf {device} werden bei der Installation gelöscht.",
        "pm_dlg_table_gpt": "GPT (empfohlen, UEFI und BIOS)",
        "pm_dlg_table_msdos": "MBR / DOS (ältere PCs, max. 4 Partitionen)",
        "pm_confirm_delete": "{path} löschen? Der Inhalt geht bei der Installation verloren.",
        "pm_fs_none": "Keines (unformatiert)",
        "pm_fs_bios_grub": "BIOS-Boot (bios_grub)",
        "pm_fs_swap": "Auslagerungsspeicher (Swap)",
        "pm_fs_vfat": "FAT32 (EFI-Partition)",
        "pm_mount_none": "(keiner)",
        "pm_ops_title": "Änderungen an der Festplatte:",
        "pm_confirm_title": "Festplattenänderungen bestätigen",
        "pm_confirm_text": "Folgende Vorgänge werden auf {device} angewendet. "
                           "Mit ⚠ markierte Vorgänge löschen Daten unwiderruflich.",
        "pm_confirm_check": "Ich habe meine Daten gesichert und möchte diese Änderungen anwenden",
        "pm_confirm_apply": "Anwenden und installieren",
        "op_new_table": "⚠ Neue {label}-Tabelle auf {device} (löscht alle Partitionen)",
        "op_delete": "⚠ {path} löschen",
        "op_shrink": "⚠ {path} verkleinern: {old} → {new}",
        "op_grow": "{path} vergrößern: {old} → {new}",
        "op_create": "{path} erstellen: {fs}, {size}, eingehängt in {mount}",
        "op_format": "⚠ {path} als {fs} formatieren",
        "op_mount": "{path} für {mount} verwenden (ohne Formatieren)",
        "err_no_table": "Die Festplatte hat keine Partitionstabelle: Erstellen Sie eine.",
        "err_no_root": "Keine Partition ist in „/“ (Systemwurzel) eingehängt.",
        "err_root_format": "Die Wurzelpartition „/“ muss als ext4 formatiert werden.",
        "err_root_small": "Die Wurzelpartition muss mindestens {size} GB groß sein.",
        "err_mount_invalid": "Ungültiger Einhängepunkt: {mount}",
        "err_mount_dup": "Einhängepunkt doppelt verwendet: {mount}",
        "err_mount_nofs": "{mount}: Die Partition hat kein Dateisystem, aktivieren Sie „Formatieren“.",
        "err_boot_separate": "Eine separate /boot-Partition wird nicht unterstützt.",
        "err_no_esp": "Es fehlt eine EFI-Partition (FAT32, eingehängt in /boot/efi).",
        "err_esp_fs": "Die Partition /boot/efi muss FAT32 (vfat) sein.",
        "err_esp_small": "Die EFI-Partition muss mindestens {size} MB groß sein.",
        "err_no_bios_grub": "Eine GPT-Festplatte benötigt eine kleine BIOS-Boot-Partition (bios_grub).",
        "err_bios_grub_msdos": "bios_grub gibt es nur auf GPT-Festplatten.",
        "err_too_many": "Eine MBR-Tabelle fasst höchstens {limit} Partitionen.",
        "err_busy": "{path} wird vom Live-System verwendet und kann nicht geändert werden.",
        "err_busy_table": "Eine Partition dieser Festplatte wird vom Live-System verwendet.",
        "err_overlap": "{a} und {b} überlappen sich.",
        "err_no_space": "Dort ist nicht genug freier Speicher.",
        "err_size": "Ungültige Größe.",
        "err_cannot_shrink": "Eine {fs}-Partition kann ohne Formatieren nicht verkleinert werden.",
        "err_cannot_grow": "Eine {fs}-Partition kann ohne Formatieren nicht vergrößert werden.",
        "progress_manual_apply": "Partitionierungsplan wird angewendet...",
        "progress_manual_format": "{path} wird formatiert ({fs})...",
        "progress_manual_mount": "{path} wird in {mount} eingehängt...",
    },
    "es_ES": {
        "pm_title": "Particionado manual",
        "pm_hint": "No se escribe nada en el disco hasta pulsar «Instalar».",
        "pm_col_device": "Partición",
        "pm_col_type": "Sistema de archivos",
        "pm_col_size": "Tamaño",
        "pm_col_mount": "Punto de montaje",
        "pm_col_action": "Acción",
        "pm_free": "Espacio libre",
        "pm_new_table": "Nueva tabla…",
        "pm_create": "Crear…",
        "pm_edit": "Modificar…",
        "pm_delete": "Eliminar",
        "pm_revert": "Deshacer los cambios",
        "pm_auto_boot": "Añadir particiones de arranque",
        "pm_status_keep": "Conservar",
        "pm_status_new": "Nueva",
        "pm_status_format": "Formatear",
        "pm_status_resize": "Redimensionar",
        "pm_status_busy": "En uso",
        "pm_valid": "El plan de particionado es válido.",
        "pm_no_disk": "Elija primero un disco.",
        "pm_dlg_create": "Crear una partición",
        "pm_dlg_edit": "Modificar {path}",
        "pm_dlg_size": "Tamaño (MB):",
        "pm_dlg_fs": "Sistema de archivos:",
        "pm_dlg_mount": "Punto de montaje:",
        "pm_dlg_label": "Nombre (etiqueta):",
        "pm_dlg_format": "Formatear esta partición (borra su contenido)",
        "pm_dlg_ok": "Aceptar",
        "pm_dlg_cancel": "Cancelar",
        "pm_dlg_max": "Máximo disponible: {size}",
        "pm_dlg_table": "Crear una nueva tabla de particiones",
        "pm_dlg_table_warn": "Todas las particiones de {device} se eliminarán durante la instalación.",
        "pm_dlg_table_gpt": "GPT (recomendado, UEFI y BIOS)",
        "pm_dlg_table_msdos": "MBR / DOS (PC antiguos, máximo 4 particiones)",
        "pm_confirm_delete": "¿Eliminar {path}? Su contenido se perderá durante la instalación.",
        "pm_fs_none": "Ninguno (sin formato)",
        "pm_fs_bios_grub": "Arranque BIOS (bios_grub)",
        "pm_fs_swap": "Área de intercambio (swap)",
        "pm_fs_vfat": "FAT32 (partición EFI)",
        "pm_mount_none": "(ninguno)",
        "pm_ops_title": "Cambios en el disco:",
        "pm_confirm_title": "Confirmar los cambios en el disco",
        "pm_confirm_text": "Se aplicarán las siguientes operaciones a {device}. "
                           "Las marcadas con ⚠ destruyen datos de forma irreversible.",
        "pm_confirm_check": "He hecho una copia de mis datos y quiero aplicar estos cambios",
        "pm_confirm_apply": "Aplicar e instalar",
        "op_new_table": "⚠ Nueva tabla {label} en {device} (borra todas las particiones)",
        "op_delete": "⚠ Eliminar {path}",
        "op_shrink": "⚠ Reducir {path}: {old} → {new}",
        "op_grow": "Ampliar {path}: {old} → {new}",
        "op_create": "Crear {path}: {fs}, {size}, montaje {mount}",
        "op_format": "⚠ Formatear {path} como {fs}",
        "op_mount": "Usar {path} en {mount} (sin formatear)",
        "err_no_table": "El disco no tiene tabla de particiones: cree una.",
        "err_no_root": "Ninguna partición está montada en «/» (raíz del sistema).",
        "err_root_format": "La partición raíz «/» debe formatearse en ext4.",
        "err_root_small": "La partición raíz debe tener al menos {size} GB.",
        "err_mount_invalid": "Punto de montaje no válido: {mount}",
        "err_mount_dup": "Punto de montaje usado dos veces: {mount}",
        "err_mount_nofs": "{mount}: la partición no tiene sistema de archivos, marque «Formatear».",
        "err_boot_separate": "No se admite una partición /boot separada.",
        "err_no_esp": "Falta una partición EFI (FAT32 montada en /boot/efi).",
        "err_esp_fs": "La partición /boot/efi debe ser FAT32 (vfat).",
        "err_esp_small": "La partición EFI debe tener al menos {size} MB.",
        "err_no_bios_grub": "Un disco GPT necesita una pequeña partición de arranque BIOS (bios_grub).",
        "err_bios_grub_msdos": "bios_grub solo existe en discos GPT.",
        "err_too_many": "Una tabla MBR admite como máximo {limit} particiones.",
        "err_busy": "{path} la usa el sistema live y no se puede modificar.",
        "err_busy_table": "El sistema live usa una partición de este disco.",
        "err_overlap": "{a} y {b} se solapan.",
        "err_no_space": "No hay suficiente espacio libre ahí.",
        "err_size": "Tamaño no válido.",
        "err_cannot_shrink": "No se puede reducir una partición {fs} sin formatearla.",
        "err_cannot_grow": "No se puede ampliar una partición {fs} sin formatearla.",
        "progress_manual_apply": "Aplicando el plan de particionado...",
        "progress_manual_format": "Formateando {path} ({fs})...",
        "progress_manual_mount": "Montando {path} en {mount}...",
    },
    "it_IT": {
        "pm_title": "Partizionamento manuale",
        "pm_hint": "Nulla viene scritto sul disco finché non si fa clic su «Installa».",
        "pm_col_device": "Partizione",
        "pm_col_type": "File system",
        "pm_col_size": "Dimensione",
        "pm_col_mount": "Punto di mount",
        "pm_col_action": "Azione",
        "pm_free": "Spazio libero",
        "pm_new_table": "Nuova tabella…",
        "pm_create": "Crea…",
        "pm_edit": "Modifica…",
        "pm_delete": "Elimina",
        "pm_revert": "Annulla le modifiche",
        "pm_auto_boot": "Aggiungi partizioni di avvio",
        "pm_status_keep": "Mantieni",
        "pm_status_new": "Nuova",
        "pm_status_format": "Formatta",
        "pm_status_resize": "Ridimensiona",
        "pm_status_busy": "In uso",
        "pm_valid": "Il piano di partizionamento è valido.",
        "pm_no_disk": "Scegli prima un disco.",
        "pm_dlg_create": "Crea una partizione",
        "pm_dlg_edit": "Modifica {path}",
        "pm_dlg_size": "Dimensione (MB):",
        "pm_dlg_fs": "File system:",
        "pm_dlg_mount": "Punto di mount:",
        "pm_dlg_label": "Nome (etichetta):",
        "pm_dlg_format": "Formatta questa partizione (cancella il contenuto)",
        "pm_dlg_ok": "OK",
        "pm_dlg_cancel": "Annulla",
        "pm_dlg_max": "Massimo disponibile: {size}",
        "pm_dlg_table": "Crea una nuova tabella delle partizioni",
        "pm_dlg_table_warn": "Tutte le partizioni di {device} verranno eliminate durante l'installazione.",
        "pm_dlg_table_gpt": "GPT (consigliato, UEFI e BIOS)",
        "pm_dlg_table_msdos": "MBR / DOS (PC datati, massimo 4 partizioni)",
        "pm_confirm_delete": "Eliminare {path}? Il contenuto andrà perso durante l'installazione.",
        "pm_fs_none": "Nessuno (non formattata)",
        "pm_fs_bios_grub": "Avvio BIOS (bios_grub)",
        "pm_fs_swap": "Area di swap",
        "pm_fs_vfat": "FAT32 (partizione EFI)",
        "pm_mount_none": "(nessuno)",
        "pm_ops_title": "Modifiche al disco:",
        "pm_confirm_title": "Conferma le modifiche al disco",
        "pm_confirm_text": "Le seguenti operazioni verranno applicate a {device}. "
                           "Quelle contrassegnate con ⚠ distruggono i dati in modo irreversibile.",
        "pm_confirm_check": "Ho fatto un backup dei miei dati e voglio applicare queste modifiche",
        "pm_confirm_apply": "Applica e installa",
        "op_new_table": "⚠ Nuova tabella {label} su {device} (cancella tutte le partizioni)",
        "op_delete": "⚠ Elimina {path}",
        "op_shrink": "⚠ Riduci {path}: {old} → {new}",
        "op_grow": "Ingrandisci {path}: {old} → {new}",
        "op_create": "Crea {path}: {fs}, {size}, montata su {mount}",
        "op_format": "⚠ Formatta {path} come {fs}",
        "op_mount": "Usa {path} su {mount} (senza formattare)",
        "err_no_table": "Il disco non ha una tabella delle partizioni: creane una.",
        "err_no_root": "Nessuna partizione è montata su «/» (radice del sistema).",
        "err_root_format": "La partizione radice «/» deve essere formattata in ext4.",
        "err_root_small": "La partizione radice deve essere di almeno {size} GB.",
        "err_mount_invalid": "Punto di mount non valido: {mount}",
        "err_mount_dup": "Punto di mount usato due volte: {mount}",
        "err_mount_nofs": "{mount}: la partizione non ha un file system, seleziona «Formatta».",
        "err_boot_separate": "Una partizione /boot separata non è supportata.",
        "err_no_esp": "Manca una partizione EFI (FAT32 montata su /boot/efi).",
        "err_esp_fs": "La partizione /boot/efi deve essere FAT32 (vfat).",
        "err_esp_small": "La partizione EFI deve essere di almeno {size} MB.",
        "err_no_bios_grub": "Un disco GPT richiede una piccola partizione di avvio BIOS (bios_grub).",
        "err_bios_grub_msdos": "bios_grub esiste solo sui dischi GPT.",
        "err_too_many": "Una tabella MBR contiene al massimo {limit} partizioni.",
        "err_busy": "{path} è usata dal sistema live e non può essere modificata.",
        "err_busy_table": "Una partizione di questo disco è usata dal sistema live.",
        "err_overlap": "{a} e {b} si sovrappongono.",
        "err_no_space": "Spazio libero insufficiente in quel punto.",
        "err_size": "Dimensione non valida.",
        "err_cannot_shrink": "Impossibile ridurre una partizione {fs} senza formattarla.",
        "err_cannot_grow": "Impossibile ingrandire una partizione {fs} senza formattarla.",
        "progress_manual_apply": "Applicazione del piano di partizionamento...",
        "progress_manual_format": "Formattazione di {path} ({fs})...",
        "progress_manual_mount": "Montaggio di {path} su {mount}...",
    },
    "pt_BR": {
        "pm_title": "Particionamento manual",
        "pm_hint": "Nada é gravado no disco até você clicar em \"Instalar\".",
        "pm_col_device": "Partição",
        "pm_col_type": "Sistema de arquivos",
        "pm_col_size": "Tamanho",
        "pm_col_mount": "Ponto de montagem",
        "pm_col_action": "Ação",
        "pm_free": "Espaço livre",
        "pm_new_table": "Nova tabela…",
        "pm_create": "Criar…",
        "pm_edit": "Editar…",
        "pm_delete": "Excluir",
        "pm_revert": "Desfazer alterações",
        "pm_auto_boot": "Adicionar partições de inicialização",
        "pm_status_keep": "Manter",
        "pm_status_new": "Nova",
        "pm_status_format": "Formatar",
        "pm_status_resize": "Redimensionar",
        "pm_status_busy": "Em uso",
        "pm_valid": "O plano de particionamento é válido.",
        "pm_no_disk": "Escolha um disco primeiro.",
        "pm_dlg_create": "Criar uma partição",
        "pm_dlg_edit": "Editar {path}",
        "pm_dlg_size": "Tamanho (MB):",
        "pm_dlg_fs": "Sistema de arquivos:",
        "pm_dlg_mount": "Ponto de montagem:",
        "pm_dlg_label": "Nome (rótulo):",
        "pm_dlg_format": "Formatar esta partição (apaga o conteúdo)",
        "pm_dlg_ok": "OK",
        "pm_dlg_cancel": "Cancelar",
        "pm_dlg_max": "Máximo disponível: {size}",
        "pm_dlg_table": "Criar uma nova tabela de partições",
        "pm_dlg_table_warn": "Todas as partições de {device} serão excluídas durante a instalação.",
        "pm_dlg_table_gpt": "GPT (recomendado, UEFI e BIOS)",
        "pm_dlg_table_msdos": "MBR / DOS (PCs antigos, no máximo 4 partições)",
        "pm_confirm_delete": "Excluir {path}? O conteúdo será perdido durante a instalação.",
        "pm_fs_none": "Nenhum (sem formatação)",
        "pm_fs_bios_grub": "Inicialização BIOS (bios_grub)",
        "pm_fs_swap": "Área de troca (swap)",
        "pm_fs_vfat": "FAT32 (partição EFI)",
        "pm_mount_none": "(nenhum)",
        "pm_ops_title": "Alterações no disco:",
        "pm_confirm_title": "Confirmar alterações no disco",
        "pm_confirm_text": "As operações a seguir serão aplicadas a {device}. "
                           "As marcadas com ⚠ destroem dados de forma irreversível.",
        "pm_confirm_check": "Fiz backup dos meus dados e quero aplicar estas alterações",
        "pm_confirm_apply": "Aplicar e instalar",
        "op_new_table": "⚠ Nova tabela {label} em {device} (apaga todas as partições)",
        "op_delete": "⚠ Excluir {path}",
        "op_shrink": "⚠ Reduzir {path}: {old} → {new}",
        "op_grow": "Aumentar {path}: {old} → {new}",
        "op_create": "Criar {path}: {fs}, {size}, montada em {mount}",
        "op_format": "⚠ Formatar {path} como {fs}",
        "op_mount": "Usar {path} em {mount} (sem formatar)",
        "err_no_table": "O disco não tem tabela de partições: crie uma.",
        "err_no_root": "Nenhuma partição está montada em \"/\" (raiz do sistema).",
        "err_root_format": "A partição raiz \"/\" deve ser formatada em ext4.",
        "err_root_small": "A partição raiz deve ter pelo menos {size} GB.",
        "err_mount_invalid": "Ponto de montagem inválido: {mount}",
        "err_mount_dup": "Ponto de montagem usado duas vezes: {mount}",
        "err_mount_nofs": "{mount}: a partição não tem sistema de arquivos, marque \"Formatar\".",
        "err_boot_separate": "Uma partição /boot separada não é suportada.",
        "err_no_esp": "Falta uma partição EFI (FAT32 montada em /boot/efi).",
        "err_esp_fs": "A partição /boot/efi deve ser FAT32 (vfat).",
        "err_esp_small": "A partição EFI deve ter pelo menos {size} MB.",
        "err_no_bios_grub": "Um disco GPT precisa de uma pequena partição de inicialização BIOS (bios_grub).",
        "err_bios_grub_msdos": "bios_grub só existe em discos GPT.",
        "err_too_many": "Uma tabela MBR comporta no máximo {limit} partições.",
        "err_busy": "{path} é usada pelo sistema live e não pode ser alterada.",
        "err_busy_table": "Uma partição deste disco é usada pelo sistema live.",
        "err_overlap": "{a} e {b} se sobrepõem.",
        "err_no_space": "Não há espaço livre suficiente ali.",
        "err_size": "Tamanho inválido.",
        "err_cannot_shrink": "Não é possível reduzir uma partição {fs} sem formatá-la.",
        "err_cannot_grow": "Não é possível aumentar uma partição {fs} sem formatá-la.",
        "progress_manual_apply": "Aplicando o plano de particionamento...",
        "progress_manual_format": "Formatando {path} ({fs})...",
        "progress_manual_mount": "Montando {path} em {mount}...",
    },
}
