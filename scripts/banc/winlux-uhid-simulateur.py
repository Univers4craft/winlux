#!/usr/bin/env python3
"""Clavier RVB simulé pour le banc de test (#1531, #1534). OUTIL DE BANC.

Crée, par /dev/uhid, un appareil HID virtuel (par défaut le clavier RVB
d'essai de Winlux, VID 1209 PID 0001 de pid.codes) et journalise ce qu'un
logiciel d'éclairage lancé dans Winlux lui envoie : rapports de sortie,
SET_REPORT et GET_REPORT (feature), avec l'heure, le type, le numéro, la
taille et les octets, plus la cadence en trames par seconde.

C'est l'observation permise par le cadre d'interopérabilité de AGENTS.md
(directive 2009/24/CE, art. 5 § 3) : on regarde ce que le logiciel envoie à
NOTRE appareil. Le descripteur HID d'une marque peut être passé avec
--descripteur s'il vient d'une source publique ou d'un pilote libre (noter la
source dans la PR).

Sécurité (décision du 10/11/2026) :
  - réservé à root, dans une machine virtuelle de test : l'outil refuse de
    tourner ailleurs. /dev/uhid permet de fabriquer un clavier, donc de taper
    à la place de quelqu'un ; il ne doit JAMAIS être ouvert aux comptes
    (aucune règle udev), et cet outil n'est pas installé dans l'ISO ;
  - l'outil n'envoie aucune frappe : il ne fait que répondre et journaliser.

Utilisation, dans la VM, en root :
  modprobe uhid
  python3 -I winlux-uhid-simulateur.py --journal /root/uhid.log --duree 120
Vérification hors ligne de l'encodage (sans /dev/uhid ni root) :
  python3 -I winlux-uhid-simulateur.py --verifier
"""

import argparse
import os
import struct
import sys
import time

# linux/uhid.h (interface publique du noyau) : types d'événements.
UHID_DESTROY = 1
UHID_START = 2
UHID_STOP = 3
UHID_OPEN = 4
UHID_CLOSE = 5
UHID_OUTPUT = 6
UHID_GET_REPORT = 9
UHID_GET_REPORT_REPLY = 10
UHID_CREATE2 = 11
UHID_SET_REPORT = 13
UHID_SET_REPORT_REPLY = 14

UHID_DATA_MAX = 4096
# Taille de struct uhid_event : u32 + la plus grande variante (create2).
EVENT_SIZE = 4 + 128 + 64 + 64 + 2 + 2 + 4 + 4 + 4 + 4 + UHID_DATA_MAX
BUS_USB = 3
RTYPE = {0: "feature", 1: "sortie", 2: "entrée"}

# Clavier RVB d'essai de Winlux (le même que scripts/test-hid-rgb.sh) :
# collection clavier (rapport 1, modificateurs) et collection fabricant
# 0xFF00 (rapport 7 feature de 64 octets, rapport 8 sortie de 32 couleurs).
DESCRIPTEUR_ESSAI = bytes.fromhex(
    "05010906a1018501050719e029e715002501750195088102c0"
    "0600ff0901a10185070902150026ff007508964000b102"
    "8508090395209102c0"
)


def event_create2(name, vendor, product, descriptor, version=0x0105):
    if len(descriptor) > UHID_DATA_MAX:
        raise ValueError("descripteur trop long")
    body = struct.pack(
        "<128s64s64sHHIIII",
        name.encode()[:127],
        b"winlux-banc",
        b"BANC0001",
        len(descriptor),
        BUS_USB,
        vendor,
        product,
        version,
        0,
    ) + descriptor.ljust(UHID_DATA_MAX, b"\0")
    return pad(struct.pack("<I", UHID_CREATE2) + body)


def event_simple(kind):
    return pad(struct.pack("<I", kind))


def event_get_reply(ident, data, err=0):
    return pad(
        struct.pack("<IIHH", UHID_GET_REPORT_REPLY, ident, err, len(data))
        + data.ljust(UHID_DATA_MAX, b"\0")
    )


def event_set_reply(ident, err=0):
    return pad(struct.pack("<IIH", UHID_SET_REPORT_REPLY, ident, err))


def pad(b):
    return b.ljust(EVENT_SIZE, b"\0")


def decode(buf):
    """Événement lu sur /dev/uhid → dict (type, et champs utiles)."""
    (kind,) = struct.unpack_from("<I", buf, 0)
    ev = {"type": kind}
    if kind == UHID_OUTPUT:
        size, rtype = struct.unpack_from("<HB", buf, 4 + UHID_DATA_MAX)
        ev.update(data=bytes(buf[4 : 4 + min(size, UHID_DATA_MAX)]), rtype=rtype)
    elif kind == UHID_GET_REPORT:
        ident, rnum, rtype = struct.unpack_from("<IBB", buf, 4)
        ev.update(id=ident, rnum=rnum, rtype=rtype)
    elif kind == UHID_SET_REPORT:
        ident, rnum, rtype, size = struct.unpack_from("<IBBH", buf, 4)
        start = 4 + 8
        ev.update(id=ident, rnum=rnum, rtype=rtype, data=bytes(buf[start : start + min(size, UHID_DATA_MAX)]))
    return ev


def in_vm():
    """Machine virtuelle de test : fabricant DMI d'un hyperviseur, ou
    indicateur « hypervisor » du processeur."""
    try:
        with open("/sys/class/dmi/id/sys_vendor", encoding="utf-8") as f:
            vendor = f.read().lower()
        if any(v in vendor for v in ("qemu", "kvm", "bochs", "virtualbox", "innotek", "vmware", "microsoft corporation")):
            return True
    except OSError:
        pass
    try:
        with open("/proc/cpuinfo", encoding="utf-8") as f:
            return any(line.startswith("flags") and " hypervisor" in line for line in f)
    except OSError:
        return False


class Journal:
    def __init__(self, path):
        self.f = open(path, "a", encoding="utf-8") if path else sys.stdout
        self.t0 = time.monotonic()
        self.frames = 0
        self.first = None
        self.last = None

    def line(self, text):
        self.f.write("%9.3f %s\n" % (time.monotonic() - self.t0, text))
        self.f.flush()

    def report(self, what, rtype, rnum, data):
        now = time.monotonic()
        if what in ("OUTPUT", "SET_REPORT"):
            self.frames += 1
            self.first = self.first or now
            self.last = now
        self.line("%s %s n°%d %d o %s" % (what, RTYPE.get(rtype, rtype), rnum, len(data), data.hex()))

    def summary(self):
        if self.frames > 1 and self.last > self.first:
            fps = (self.frames - 1) / (self.last - self.first)
            self.line("bilan : %d trames, %.1f par seconde" % (self.frames, fps))
        else:
            self.line("bilan : %d trame(s)" % self.frames)


def run(args):
    if os.geteuid() != 0:
        sys.exit("outil de banc réservé à root (dans la VM de test)")
    if not in_vm():
        sys.exit("refusé : cet outil ne tourne que dans une machine virtuelle de test")
    descriptor = DESCRIPTEUR_ESSAI
    if args.descripteur:
        with open(args.descripteur, encoding="ascii") as f:
            descriptor = bytes.fromhex("".join(f.read().split()))
    journal = Journal(args.journal)
    features = {}
    fd = os.open("/dev/uhid", os.O_RDWR | os.O_CLOEXEC)
    try:
        os.write(fd, event_create2(args.nom, args.vid, args.pid, descriptor))
        journal.line("appareil créé : %04x:%04x « %s », descripteur %d o" % (args.vid, args.pid, args.nom, len(descriptor)))
        deadline = time.monotonic() + args.duree if args.duree else None
        import select

        while deadline is None or time.monotonic() < deadline:
            ready, _, _ = select.select([fd], [], [], 0.5)
            if not ready:
                continue
            ev = decode(os.read(fd, EVENT_SIZE))
            kind = ev["type"]
            if kind == UHID_OUTPUT:
                data = ev["data"]
                journal.report("OUTPUT", ev["rtype"], data[0] if data else 0, data)
            elif kind == UHID_SET_REPORT:
                features[ev["rnum"]] = ev["data"]
                journal.report("SET_REPORT", ev["rtype"], ev["rnum"], ev["data"])
                os.write(fd, event_set_reply(ev["id"]))
            elif kind == UHID_GET_REPORT:
                data = features.get(ev["rnum"], bytes([ev["rnum"]]) + bytes(63))
                journal.report("GET_REPORT", ev["rtype"], ev["rnum"], b"")
                os.write(fd, event_get_reply(ev["id"], data))
            elif kind in (UHID_START, UHID_STOP, UHID_OPEN, UHID_CLOSE):
                journal.line({UHID_START: "START", UHID_STOP: "STOP", UHID_OPEN: "OPEN", UHID_CLOSE: "CLOSE"}[kind])
    except KeyboardInterrupt:
        pass
    finally:
        try:
            os.write(fd, event_simple(UHID_DESTROY))
        finally:
            os.close(fd)
        journal.summary()


def verifier():
    """Encodage et décodage, sans /dev/uhid."""
    ok = True

    def check(cond, label):
        nonlocal ok
        print(("OK     " if cond else "ÉCHEC  ") + label)
        ok = ok and cond

    ev = event_create2("Clavier", 0x1209, 0x0001, DESCRIPTEUR_ESSAI)
    check(len(ev) == EVENT_SIZE == 4376, "taille d'un événement uhid (4376 o)")
    check(struct.unpack_from("<I", ev, 0)[0] == UHID_CREATE2, "type CREATE2")
    rd_size, bus, vid, pid = struct.unpack_from("<HHII", ev, 4 + 256)
    check((rd_size, bus, vid, pid) == (len(DESCRIPTEUR_ESSAI), BUS_USB, 0x1209, 1), "taille du descripteur, bus USB, VID:PID")
    check(ev[4 + 256 + 20 : 4 + 256 + 20 + len(DESCRIPTEUR_ESSAI)] == DESCRIPTEUR_ESSAI, "descripteur recopié")
    out = pad(struct.pack("<I", UHID_OUTPUT) + bytes([8, 1, 2, 3]).ljust(UHID_DATA_MAX, b"\0") + struct.pack("<HB", 33, 1))
    d = decode(out)
    check(d["data"][:4] == bytes([8, 1, 2, 3]) and len(d["data"]) == 33 and d["rtype"] == 1, "décodage d'un rapport de sortie")
    sr = pad(struct.pack("<IIBBH", UHID_SET_REPORT, 42, 7, 0, 3) + b"\x07\xaa\xbb")
    d = decode(sr)
    check((d["id"], d["rnum"], d["rtype"], d["data"]) == (42, 7, 0, b"\x07\xaa\xbb"), "décodage d'un SET_REPORT")
    gr = event_get_reply(5, b"\x07\x01")
    check(struct.unpack_from("<IIHH", gr, 0) == (UHID_GET_REPORT_REPLY, 5, 0, 2), "réponse GET_REPORT")
    check(struct.unpack_from("<IIH", event_set_reply(9), 0) == (UHID_SET_REPORT_REPLY, 9, 0), "réponse SET_REPORT")
    return ok


def main():
    p = argparse.ArgumentParser(description="Clavier RVB simulé (outil de banc, root, VM seulement)")
    p.add_argument("--vid", type=lambda s: int(s, 16), default=0x1209)
    p.add_argument("--pid", type=lambda s: int(s, 16), default=0x0001)
    p.add_argument("--nom", default="Winlux Essais Clavier RVB")
    p.add_argument("--descripteur", help="fichier texte du descripteur HID en hexadécimal (source publique)")
    p.add_argument("--journal", help="fichier journal (sinon la sortie standard)")
    p.add_argument("--duree", type=float, default=0, help="secondes avant de retirer l'appareil (0 : Ctrl+C)")
    p.add_argument("--verifier", action="store_true", help="vérifie l'encodage hors ligne et quitte")
    args = p.parse_args()
    if args.verifier:
        sys.exit(0 if verifier() else 1)
    run(args)


if __name__ == "__main__":
    main()
