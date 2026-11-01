#!/usr/bin/env python3
"""Tests du créateur de clé USB (sans droits administrateur ni vraie clé)."""

import ast
import hashlib
import http.server
import json
import os
import re
import string
import subprocess
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(ROOT))

from winlux_usb import disks, i18n, image, writer  # noqa: E402
from winlux_usb.disks import Disk, DiskError  # noqa: E402

if os.name != "nt":
    from winlux_usb import disks_linux  # noqa: E402


def fake_iso(path, size=9 * 1024 * 1024 + 1000, hybrid=True):
    """Image au format d'une ISO hybride : « CD001 » au secteur 16, 0x55AA en fin de MBR."""
    data = bytearray(os.urandom(size))
    data[0x8001:0x8006] = b"CD001"
    data[510:512] = b"\x55\xaa" if hybrid else b"\0\0"
    Path(path).write_bytes(data)
    return bytes(data)


class ImageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def test_check_image(self):
        fake_iso(self.dir / "ok.iso", 64 * 1024)
        self.assertEqual(image.check_image(self.dir / "ok.iso"), 64 * 1024)
        fake_iso(self.dir / "nohybrid.iso", 64 * 1024, hybrid=False)
        with self.assertRaises(image.ImageError) as ctx:
            image.check_image(self.dir / "nohybrid.iso")
        self.assertEqual(ctx.exception.key, "err_not_hybrid")
        (self.dir / "doc.txt").write_bytes(b"bonjour" * 10000)
        with self.assertRaises(image.ImageError) as ctx:
            image.check_image(self.dir / "doc.txt")
        self.assertEqual(ctx.exception.key, "err_not_iso")
        with self.assertRaises(image.ImageError) as ctx:
            image.check_image(self.dir / "absent.iso")
        self.assertEqual(ctx.exception.key, "err_read_image")

    def test_sha256_sidecar(self):
        data = fake_iso(self.dir / "w.iso", 70000)
        digest = hashlib.sha256(data).hexdigest()
        (self.dir / "w.iso.sha256").write_text(f"{digest}  w.iso\n")
        self.assertEqual(image.sidecar_sha256(self.dir / "w.iso"), digest)
        self.assertEqual(image.sha256_file(self.dir / "w.iso"), digest)
        self.assertEqual(image.parse_sha256_text(f"{digest.upper()} *dossier/w.iso", "w.iso"), digest)
        self.assertIsNone(image.parse_sha256_text(f"{digest}  autre.iso", "w.iso"))
        self.assertIsNone(image.sidecar_sha256(self.dir / "absent.iso"))


class _Server:
    """Serveur HTTP local qui imite l'API des versions GitHub et sert l'ISO."""

    def __init__(self, directory):
        handler = lambda *a, **k: _RangeHandler(*a, directory=str(directory), **k)  # noqa: E731
        self.httpd = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        self.url = f"http://127.0.0.1:{self.httpd.server_address[1]}"
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()


class _RangeHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_GET(self):
        rng = self.headers.get("Range")
        path = Path(self.translate_path(self.path))
        if not rng or not path.is_file():
            return super().do_GET()
        start = int(re.match(r"bytes=(\d+)-", rng).group(1))
        data = path.read_bytes()[start:]
        self.send_response(206)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        (self.dir / "srv").mkdir()
        self.data = fake_iso(self.dir / "srv/winlux-live-v9.9.9.iso", 3 * 1024 * 1024 + 7)
        self.server = _Server(self.dir / "srv")
        digest = hashlib.sha256(self.data).hexdigest()
        release = {"tag_name": "v9.9.9", "assets": [
            {"name": "winlux-usb-creator-v9.9.9.exe", "size": 1, "browser_download_url": self.server.url + "/x"},
            {"name": "winlux-live-v9.9.9.iso", "size": len(self.data), "digest": f"sha256:{digest}",
             "browser_download_url": self.server.url + "/winlux-live-v9.9.9.iso"}]}
        (self.dir / "srv/latest.json").write_text(json.dumps(release))
        self.api = mock.patch.object(image, "RELEASE_API", self.server.url + "/latest.json")
        self.api.start()

    def tearDown(self):
        self.api.stop()
        self.server.close()
        self.tmp.cleanup()

    def test_latest_release_picks_the_iso(self):
        rel = image.latest_release()
        self.assertEqual(rel["tag"], "v9.9.9")
        self.assertEqual(rel["name"], "winlux-live-v9.9.9.iso")
        self.assertEqual(rel["sha256"], hashlib.sha256(self.data).hexdigest())

    def test_sha256_from_sidecar_asset_when_no_digest(self):
        digest = hashlib.sha256(self.data).hexdigest()
        (self.dir / "srv/winlux-live-v9.9.9.iso.sha256").write_text(f"{digest}  winlux-live-v9.9.9.iso\n")
        release = json.loads((self.dir / "srv/latest.json").read_text())
        del release["assets"][1]["digest"]
        release["assets"].append({"name": "winlux-live-v9.9.9.iso.sha256", "size": 90,
                                  "browser_download_url": self.server.url + "/winlux-live-v9.9.9.iso.sha256"})
        (self.dir / "srv/latest.json").write_text(json.dumps(release))
        self.assertEqual(image.latest_release()["sha256"], digest)

    def test_download_resume_and_reuse(self):
        rel = image.latest_release()
        dest = self.dir / "dl"
        dest.mkdir()
        (dest / (rel["name"] + ".part")).write_bytes(self.data[:1000000])  # téléchargement interrompu
        seen = []
        path = image.download(rel, dest, lambda d, t: seen.append(d))
        self.assertEqual(Path(path).read_bytes(), self.data)
        self.assertEqual(seen[0] > 1000000, True)  # reprise, pas de recommencement
        self.assertFalse((dest / (rel["name"] + ".part")).exists())
        # Deuxième appel : le fichier intact est réutilisé sans réseau.
        self.server.close()
        self.assertEqual(image.download(rel, dest), path)
        self.server = _Server(self.dir / "srv")

    def test_download_rejects_bad_checksum(self):
        rel = image.latest_release()
        rel["sha256"] = "0" * 64
        with self.assertRaises(image.ImageError) as ctx:
            image.download(rel, self.dir / "dl2")
        self.assertEqual(ctx.exception.key, "err_checksum")
        self.assertFalse((self.dir / "dl2" / rel["name"]).exists())

    def test_offline(self):
        with mock.patch.object(image, "RELEASE_API", "http://127.0.0.1:9/absent"):
            with self.assertRaises(image.ImageError) as ctx:
                image.latest_release(timeout=2)
        self.assertEqual(ctx.exception.key, "err_release")


class WriterTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.iso = self.dir / "w.iso"
        self.data = fake_iso(self.iso)
        self.target = self.dir / "cle.img"
        self.target.write_bytes(b"\xff" * (16 * 1024 * 1024))  # ancienne clé pleine

    def tearDown(self):
        self.tmp.cleanup()

    def test_write_and_verify_file_target(self):
        phases = []
        size = writer.write_image(str(self.iso), str(self.target), lambda p, d, t: phases.append(p),
                                  allow_fixed=True)
        self.assertEqual(size, len(self.data))
        written = self.target.read_bytes()
        self.assertEqual(written[:len(self.data)], self.data)
        # Le dernier secteur incomplet est complété par des zéros, rien au-delà n'est touché.
        pad = (-len(self.data)) % 512
        self.assertEqual(written[len(self.data):len(self.data) + pad], b"\0" * pad)
        self.assertEqual(written[len(self.data) + pad:len(self.data) + pad + 512], b"\xff" * 512)
        self.assertEqual(phases[0], "prepare")
        self.assertIn("write", phases)
        self.assertEqual(phases[-1], "verify")

    def test_verify_detects_corruption(self):
        real_open = disks.open_target

        def corrupting(path):
            target = real_open(path)
            original = target.read
            target.read = lambda n: bytes(b ^ 1 for b in original(n))
            return target
        with mock.patch.object(disks, "open_target", corrupting):
            with self.assertRaises(DiskError) as ctx:
                writer.write_image(str(self.iso), str(self.target), allow_fixed=True)
        self.assertEqual(ctx.exception.key, "err_verify")

    def test_cancel(self):
        event = threading.Event()
        event.set()
        with self.assertRaises(image.Cancelled):
            writer.write_image(str(self.iso), str(self.target), cancel=event, allow_fixed=True)

    def test_refuses_unknown_device_without_allow_fixed(self):
        with self.assertRaises(DiskError) as ctx:
            writer.write_image(str(self.iso), str(self.target))
        self.assertEqual(ctx.exception.key, "err_no_disk")

    def _with_disk(self, disk, on_disk=False):
        return mock.patch.multiple(disks, find_disk=lambda p: disk, image_on_disk=lambda i, d: on_disk)

    def test_safety_checks(self):
        stick = dict(path=str(self.target), model="Clé", size=16 * 1024 * 1024, usb=True)
        with self._with_disk(Disk(**stick, system=True)):
            with self.assertRaises(DiskError) as ctx:
                writer.validate(str(self.iso), str(self.target))
            self.assertEqual(ctx.exception.key, "err_system_disk")
        with self._with_disk(Disk(**stick), on_disk=True):
            with self.assertRaises(DiskError) as ctx:
                writer.validate(str(self.iso), str(self.target))
            self.assertEqual(ctx.exception.key, "err_source_on_disk")
        with self._with_disk(Disk(**{**stick, "size": 1024 * 1024})):
            with self.assertRaises(DiskError) as ctx:
                writer.validate(str(self.iso), str(self.target))
            self.assertEqual(ctx.exception.key, "err_too_small")
        with self._with_disk(Disk(**{**stick, "usb": False})):
            with self.assertRaises(DiskError) as ctx:
                writer.validate(str(self.iso), str(self.target))
            self.assertEqual(ctx.exception.key, "err_not_removable")
        with self._with_disk(Disk(**stick)), mock.patch.object(disks, "prepare") as prep:
            prep.return_value = mock.Mock()
            writer.write_image(str(self.iso), str(self.target))
            prep.assert_called_once()
            prep.return_value.close.assert_called_once()  # volumes relâchés après écriture

    def test_cli_machine_protocol(self):
        out = subprocess.run([sys.executable, str(ROOT / "winlux-usb-creator.py"), "write", "--image", str(self.iso),
                              "--device", str(self.target), "--machine", "--yes", "--allow-fixed"],
                             capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=120)
        lines = out.stdout.splitlines()
        self.assertEqual(out.returncode, 0, out.stdout + out.stderr)
        self.assertTrue(lines[0].startswith("@progress prepare"))
        self.assertTrue(any(line.startswith("@progress verify") for line in lines))
        self.assertEqual(lines[-1], "@done")
        bad = subprocess.run([sys.executable, str(ROOT / "winlux-usb-creator.py"), "write", "--image",
                              str(self.dir / "absent.iso"), "--device", str(self.target), "--machine", "--yes",
                              "--allow-fixed"], capture_output=True, text=True, timeout=60)
        self.assertEqual(bad.returncode, 2)
        self.assertTrue(bad.stdout.strip().startswith("@error err_read_image "))


@unittest.skipIf(os.name == "nt", "sysfs Linux")
class LinuxDiskTests(unittest.TestCase):
    """Faux /sys : une clé USB, un SSD système et un lecteur de carte SD."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        sys_root = root / "sys"
        devices = sys_root / "devices"

        def disk(name, real, size_sectors, removable="0", vendor="", model="", parts=()):
            node = devices / real / name
            (node / "device").mkdir(parents=True)
            (node / "size").write_text(f"{size_sectors}\n")
            (node / "removable").write_text(removable + "\n")
            if vendor:
                (node / "device/vendor").write_text(vendor + "\n")
            if model:
                (node / "device/model").write_text(model + "\n")
            for part in parts:
                (node / part).mkdir()
                (node / part / "partition").write_text("1\n")
            (sys_root / "block").mkdir(parents=True, exist_ok=True)
            (sys_root / "block" / name).symlink_to(node)

        disk("sdb", "pci0000:00/0000:00:14.0/usb2/2-1/2-1:1.0/host6/target6:0:0/6:0:0:0/block", 62914560, "1",
             "SanDisk", "Ultra", ("sdb1",))
        disk("nvme0n1", "pci0000:00/0000:00:1d.0/nvme/nvme0", 1000215216, "0", "", "Samsung SSD 980",
             ("nvme0n1p1", "nvme0n1p2"))
        disk("mmcblk0", "platform/mmc0/mmc0:0001/block", 31116288)
        disk("loop0", "virtual/block", 1000)
        disk("sr0", "pci0000:00/ata1/block", 2000000, "1")
        mounts = root / "mounts"
        mounts.write_text("/dev/nvme0n1p2 / ext4 rw 0 0\n/dev/nvme0n1p1 /boot/efi vfat rw 0 0\n"
                          "/dev/sdb1 /media/damien/MA\\040CLE vfat rw 0 0\n")
        swaps = root / "swaps"
        swaps.write_text("Filename Type Size Used Priority\n")
        self.patch = mock.patch.multiple(disks_linux, SYS=str(sys_root), MOUNTS=str(mounts), SWAPS=str(swaps))
        self.patch.start()

    def tearDown(self):
        self.patch.stop()
        self.tmp.cleanup()

    def test_list(self):
        found = {d.path: d for d in disks_linux.list_disks(include_loops=False)}
        self.assertEqual(sorted(found), ["/dev/mmcblk0", "/dev/nvme0n1", "/dev/sdb"])
        stick = found["/dev/sdb"]
        self.assertTrue(stick.usb and stick.removable and stick.candidate)
        self.assertEqual(stick.model, "SanDisk Ultra")
        self.assertEqual(stick.size, 62914560 * 512)
        self.assertEqual(stick.mountpoints, ["/media/damien/MA CLE"])
        ssd = found["/dev/nvme0n1"]
        self.assertTrue(ssd.system)
        self.assertFalse(ssd.candidate)
        self.assertTrue(found["/dev/mmcblk0"].candidate)

    def test_live_medium_is_system(self):
        mounts = Path(disks_linux.MOUNTS)
        mounts.write_text(mounts.read_text() + "/dev/sdb /run/winlux/live/media iso9660 ro 0 0\n")
        stick = {d.path: d for d in disks_linux.list_disks()}["/dev/sdb"]
        self.assertTrue(stick.system)
        self.assertFalse(stick.candidate)


class TranslationTests(unittest.TestCase):
    def test_every_language_and_placeholder(self):
        for key, entry in i18n.T.items():
            self.assertEqual(len(entry), len(i18n.LANGS), key)
            fields = {f for _, f, _, _ in string.Formatter().parse(entry[0]) if f}
            for text in entry:
                self.assertTrue(text.strip(), key)
                self.assertEqual({f for _, f, _, _ in string.Formatter().parse(text) if f}, fields, key)

    def test_messages(self):
        old = i18n.LANG
        try:
            i18n.set_lang("fr")
            self.assertEqual(i18n.human_size(32 * 1000 ** 3), "32,0 Go")
            self.assertIn("2,1 Go", i18n.error_text("err_too_small", {"need": 2057637888, "have": 1000 ** 3}))
            i18n.set_lang("en")
            self.assertEqual(i18n.human_size(1500), "1.5 KB")
            self.assertIn("Unexpected", i18n.error_text("no_such_key"))
        finally:
            i18n.set_lang(old)


@unittest.skipIf(os.name == "nt", "installateur Linux")
class InstallerLiveMediumTests(unittest.TestCase):
    """L'installateur ne propose jamais la clé USB dont il a démarré."""

    def _function(self):
        source = (ROOT.parent / "scripts/winlux-installer.py").read_text(encoding="utf-8")
        tree = ast.parse(source)
        node = next(n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "live_medium_disk")
        scope = {"os": os}
        exec(compile(ast.Module([node], []), "winlux-installer.py", "exec"), scope)  # noqa: S102
        return scope["live_medium_disk"]

    def test_live_medium_disk(self):
        live_medium_disk = self._function()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sdb = root / "sys/devices/usb/sdb"
            (sdb / "sdb1").mkdir(parents=True)
            (sdb / "sdb1/partition").write_text("1\n")
            (root / "sys/class/block").mkdir(parents=True)
            (root / "sys/class/block/sdb").symlink_to(sdb)
            (root / "sys/class/block/sdb1").symlink_to(sdb / "sdb1")
            mounts = root / "mounts"
            mounts.write_text("/dev/sdb /run/winlux/live/media iso9660 ro 0 0\n")
            self.assertEqual(live_medium_disk(str(mounts), str(root / "sys")), "sdb")
            mounts.write_text("/dev/sdb1 /run/winlux/live/media vfat ro 0 0\n")
            self.assertEqual(live_medium_disk(str(mounts), str(root / "sys")), "sdb")
            mounts.write_text("/dev/nvme0n1p2 / ext4 rw 0 0\n")
            self.assertIsNone(live_medium_disk(str(mounts), str(root / "sys")))


@unittest.skipUnless(os.environ.get("DISPLAY") or os.name == "nt", "pas d'affichage")
class GuiSmokeTests(unittest.TestCase):
    def test_window_builds(self):
        from winlux_usb import gui
        with mock.patch.object(gui.image, "latest_release", side_effect=image.ImageError("err_release")):
            app = gui.App()
            try:
                for _ in range(20):
                    app.update()
                app._handle(("release", None))
                self.assertEqual(app.mode.get(), "file")  # hors ligne : fichier ISO proposé
                self.assertEqual(str(app.start_btn.cget("state")), "disabled")
                app._handle(("status", "x", 0.5))
                self.assertEqual(int(float(app.progress.cget("value"))), 500)
            finally:
                app.destroy()


if __name__ == "__main__":
    unittest.main(verbosity=2)
