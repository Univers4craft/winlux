"""Ligne de commande. Sans argument, ouvre l'interface graphique.

  winlux-usb-creator                       interface graphique
  winlux-usb-creator list [--all] [--json] clés USB détectées
  winlux-usb-creator check IMAGE.iso       contrôle d'une image
  winlux-usb-creator download [--dest D]   télécharge la dernière ISO
  winlux-usb-creator write --image I --device D [--yes] [--no-verify]
  winlux-usb-creator self-test --output F  diagnostic (CI, exécutable Windows)

`write --machine` est le mode employé par l'interface quand elle relance
l'écriture avec les droits administrateur (pkexec/sudo) : une ligne par
événement sur la sortie standard, « @progress <phase> <fait> <total> »,
« @done », « @error <clé> <champs JSON> ».
"""

import argparse
import json
import sys
import threading

from . import version
from . import disks, image, writer
from .disks import DiskError
from .i18n import error_text, human_size, tr
from .image import Cancelled, ImageError


def _out(text):
    if sys.stdout is not None:  # exécutable Windows « fenêtré » : pas de console
        print(text, flush=True)


def cmd_list(args):
    found = [d for d in disks.list_disks() if args.all or d.candidate]
    if args.json:
        _out(json.dumps([d.__dict__ for d in found], ensure_ascii=False, indent=1))
        return 0
    for d in found:
        flags = ",".join(f for f, on in (("usb", d.usb), ("amovible", d.removable), ("système", d.system)) if on)
        _out(f"{d.path}\t{human_size(d.size)}\t{d.model}\t{flags}\t{' '.join(d.mountpoints)}")
    return 0


def cmd_check(args):
    try:
        size = image.check_image(args.image)
    except ImageError as exc:
        _out(error_text(exc.key, exc.fields))
        return 1
    expected = image.sidecar_sha256(args.image)
    if expected and image.sha256_file(args.image) != expected:
        _out(error_text("err_checksum"))
        return 1
    _out(f"OK {human_size(size)}" + (" sha256" if expected else ""))
    return 0


def cmd_download(args):
    try:
        release = image.latest_release()
        last = [-1]

        def show(done, total):
            pct = done * 100 // total if total else 0
            if pct != last[0]:
                last[0] = pct
                _out(tr("phase_download", done=human_size(done), total=human_size(total)))
        path = image.download(release, args.dest, show)
    except ImageError as exc:
        _out(error_text(exc.key, exc.fields))
        return 1
    _out(str(path))
    return 0


def _confirm_tty(device):
    disk = disks.find_disk(device)
    label = f"{disk.model} ({human_size(disk.size)})" if disk else device
    _out(tr("confirm_body", disk=label, size=device).replace("\n\n", "\n"))
    try:
        answer = input(f"{tr('confirm_ok')} ? [oui/non] ").strip().lower()
    except EOFError:
        return False
    return answer in ("oui", "o", "yes", "y", "ja", "j", "si", "sí", "s", "sim")


def cmd_write(args):
    machine = args.machine
    if not args.yes and not machine and not _confirm_tty(args.device):
        _out(tr("err_cancelled"))
        return 1
    cancel = threading.Event()
    last = {}
    if machine:
        # L'interface (non administrateur) demande l'arrêt en écrivant « cancel ».
        def listen():
            for line in sys.stdin:
                if line.strip() == "cancel":
                    cancel.set()
        threading.Thread(target=listen, daemon=True).start()

    def progress(phase, done, total):
        step = done * 200 // total if total else 0
        if last.get(phase) == step and done != total:
            return
        last[phase] = step
        if machine:
            _out(f"@progress {phase} {done} {total}")
        else:
            _out(f"{phase} {done * 100 // total if total else 0} %")

    try:
        writer.write_image(args.image, args.device, progress, cancel,
                           verify=not args.no_verify, allow_fixed=args.allow_fixed)
    except (DiskError, ImageError) as exc:
        if machine:
            _out(f"@error {exc.key} {json.dumps(exc.fields, default=str)}")
        else:
            _out(error_text(exc.key, exc.fields))
        return 2
    except Cancelled:
        _out("@error err_cancelled {}" if machine else tr("err_cancelled"))
        return 3
    except KeyboardInterrupt:
        _out("@error err_cancelled {}" if machine else tr("err_cancelled"))
        return 3
    _out("@done" if machine else tr("done_title"))
    return 0


def cmd_self_test(args):
    """Diagnostic sans effet : vérifie que l'exécutable embarque tout (Tk, ctypes, disques)."""
    report = {"version": version(), "platform": sys.platform, "admin": writer.is_admin()}
    try:
        import tkinter
        report["tk"] = tkinter.TkVersion
    except ImportError as exc:
        report["tk_error"] = str(exc)
    try:
        report["disks"] = [d.__dict__ for d in disks.list_disks()]
    except Exception as exc:  # noqa: BLE001 - le diagnostic doit toujours répondre
        report["disks_error"] = repr(exc)
    text = json.dumps(report, ensure_ascii=False, indent=1, default=str)
    if args.output:
        with open(args.output, "w", encoding="utf-8") as f:
            f.write(text)
    else:
        _out(text)
    return 0 if "tk" in report and "disks" in report else 1


def parser():
    p = argparse.ArgumentParser(prog="winlux-usb-creator", description=tr("subtitle"))
    p.add_argument("--version", action="version", version=version())
    sub = p.add_subparsers(dest="command")
    s = sub.add_parser("list")
    s.add_argument("--all", action="store_true", help="aussi les disques internes")
    s.add_argument("--json", action="store_true")
    s.set_defaults(func=cmd_list)
    s = sub.add_parser("check")
    s.add_argument("image")
    s.set_defaults(func=cmd_check)
    s = sub.add_parser("download")
    s.add_argument("--dest")
    s.set_defaults(func=cmd_download)
    s = sub.add_parser("write")
    s.add_argument("--image", required=True)
    s.add_argument("--device", required=True)
    s.add_argument("--yes", action="store_true", help="ne pas demander de confirmation")
    s.add_argument("--no-verify", action="store_true")
    s.add_argument("--allow-fixed", action="store_true",
                   help="autoriser un disque non amovible ou un fichier (tests)")
    s.add_argument("--machine", action="store_true", help=argparse.SUPPRESS)
    s.set_defaults(func=cmd_write)
    s = sub.add_parser("self-test")
    s.add_argument("--output")
    s.set_defaults(func=cmd_self_test)
    s = sub.add_parser("gui")
    s.set_defaults(func=None)
    return p


def main(argv=None):
    args = parser().parse_args(argv)
    if getattr(args, "func", None) is None:
        from . import gui
        return gui.main()
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
