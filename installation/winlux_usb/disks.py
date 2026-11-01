"""Périphériques de stockage : liste des clés USB et ouverture en écriture brute."""

import os
from dataclasses import dataclass, field


@dataclass
class Disk:
    path: str                 # /dev/sdb ou \\.\PhysicalDrive2
    model: str                # « SanDisk Ultra »
    size: int                 # octets
    usb: bool = False         # bus USB, carte SD/MMC
    removable: bool = False   # média amovible annoncé par le pilote
    system: bool = False      # porte le système en cours : jamais proposé
    mountpoints: list = field(default_factory=list)  # points de montage ou lettres
    number: int = -1          # numéro de disque Windows

    @property
    def candidate(self):
        """Proposé dans l'interface : clé USB ou carte, hors disque système."""
        return (self.usb or self.removable) and not self.system and self.size > 0


class DiskError(Exception):
    """Erreur liée au disque ; `key` est la clé du message traduit."""

    def __init__(self, key, **fields):
        super().__init__(key if not fields else f"{key}: {fields}")
        self.key = key
        self.fields = fields


if os.name == "nt":
    from . import disks_windows as _impl
else:
    from . import disks_linux as _impl


def list_disks():
    return _impl.list_disks()


def find_disk(path):
    """Disque désigné par son chemin ; None si inconnu (fichier ordinaire, test)."""
    norm = path.lower()
    for disk in list_disks():
        if disk.path.lower() == norm:
            return disk
    return None


def image_on_disk(image, disk):
    """Vrai si le fichier image est stocké sur ce disque (l'effacer le détruirait)."""
    return _impl.image_on_disk(image, disk)


def prepare(disk):
    """Démonte/verrouille les volumes du disque ; renvoie un objet à fermer après écriture."""
    return _impl.prepare(disk)


def open_target(path):
    """Ouvre la cible en lecture/écriture brute (disque ou fichier image)."""
    return _impl.open_target(path)
