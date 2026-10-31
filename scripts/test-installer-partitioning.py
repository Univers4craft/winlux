#!/usr/bin/python3
"""Tests du partitionnement manuel et de l'adaptation à l'écran (sans root ni GTK)."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import winlux_partitioner as wp  # noqa: E402
import winlux_screen as ws  # noqa: E402

GIB = 1024 ** 3
SS = 512


def mib_to_sectors(mib):
    return int(mib * wp.MIB / SS)


def windows_disk():
    """Disque GPT de 256 Gio : ESP Windows, MSR, C: NTFS, récupération NTFS."""
    sf = {"partitiontable": {"label": "gpt", "device": "/dev/nvme0n1", "unit": "sectors",
                             "sectorsize": SS, "partitions": [
        {"node": "/dev/nvme0n1p1", "start": mib_to_sectors(1), "size": mib_to_sectors(100),
         "type": wp.ESP_GUID},
        {"node": "/dev/nvme0n1p2", "start": mib_to_sectors(101), "size": mib_to_sectors(16),
         "type": "E3C9E316-0B5C-4DB8-817D-F92DF00215AE"},
        {"node": "/dev/nvme0n1p3", "start": mib_to_sectors(117), "size": mib_to_sectors(200000),
         "type": "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7"},
        {"node": "/dev/nvme0n1p4", "start": mib_to_sectors(200117), "size": mib_to_sectors(700),
         "type": "DE94BBA4-06D1-4D40-A16A-BFD50179D6AC"},
    ]}}
    lb = {"blockdevices": [{"name": "nvme0n1", "children": [
        {"name": "nvme0n1p1", "fstype": "vfat", "label": "SYSTEM", "mountpoint": None},
        {"name": "nvme0n1p2", "fstype": None, "label": None, "mountpoint": None},
        {"name": "nvme0n1p3", "fstype": "ntfs", "label": "Windows", "mountpoint": None},
        {"name": "nvme0n1p4", "fstype": "ntfs", "label": "Recovery", "mountpoint": None},
    ]}]}
    return wp.parse_disk("/dev/nvme0n1", 256 * GIB, sf, lb)


def argvs(steps):
    return [" ".join(argv) for argv, _stdin in steps]


class ParseTests(unittest.TestCase):
    def test_parse_existing_gpt(self):
        plan = windows_disk()
        self.assertEqual(plan.label, "gpt")
        self.assertEqual([p.number for p in plan.parts], [1, 2, 3, 4])
        esp = plan.parts[0]
        self.assertIn("esp", esp.flags)
        self.assertEqual(esp.fstype, "vfat")
        self.assertFalse(esp.format)
        self.assertEqual(plan.path_of(plan.parts[2]), "/dev/nvme0n1p3")
        self.assertFalse(plan.has_changes())

    def test_blank_disk_has_no_table(self):
        plan = wp.parse_disk("/dev/sda", 64 * GIB, None, None)
        self.assertIsNone(plan.label)
        self.assertEqual(plan.validate(), [("err_no_table", {})])

    def test_mounted_partition_is_busy(self):
        sf = {"partitiontable": {"label": "dos", "sectorsize": SS, "partitions": [
            {"node": "/dev/sdb1", "start": 2048, "size": mib_to_sectors(4000), "type": "83"}]}}
        lb = {"blockdevices": [{"name": "sdb", "children": [
            {"name": "sdb1", "fstype": "ext4", "label": "LIVE", "mountpoint": "/run/winlux/live"}]}]}
        plan = wp.parse_disk("/dev/sdb", 16 * GIB, sf, lb)
        self.assertEqual(plan.label, "msdos")
        part = plan.parts[0]
        self.assertTrue(part.busy)
        with self.assertRaises(wp.PlanError):
            plan.delete(part)
        with self.assertRaises(wp.PlanError):
            plan.new_partition_table("gpt")


class EditTests(unittest.TestCase):
    def test_new_table_and_create_layout(self):
        plan = windows_disk()
        plan.new_partition_table("gpt")
        self.assertEqual(plan.parts, [])
        self.assertEqual(plan.free_regions(), [(1, 256 * 1024 - 2)])
        plan.ensure_boot_partitions()
        start, size = plan.largest_free()
        plan.create(start, 8192, "swap")
        start, size = plan.largest_free()
        plan.create(start, 60 * 1024, "ext4", "/")
        start, size = plan.largest_free()
        plan.create(start, size, "ext4", "/home", label="home perso!")
        self.assertEqual(plan.validate(), [])
        self.assertEqual([p.number for p in plan.parts], [1, 2, 3, 4, 5])
        self.assertEqual(plan.parts[4].label, "homeperso")
        cmds = argvs(plan.build_steps())
        self.assertEqual(cmds[0], "parted -s /dev/nvme0n1 -- mklabel gpt")
        self.assertIn("parted -s /dev/nvme0n1 -- mkpart WINLUX 1MiB 3MiB", cmds)
        self.assertIn("parted -s /dev/nvme0n1 -- mkpart ESP fat32 3MiB 515MiB", cmds)
        self.assertIn("parted -s /dev/nvme0n1 -- set 1 bios_grub on", cmds)
        self.assertIn("parted -s /dev/nvme0n1 -- set 2 esp on", cmds)
        self.assertTrue(any("linux-swap" in c for c in cmds))
        self.assertFalse(any(c.startswith("parted -s /dev/nvme0n1 -- rm") for c in cmds))
        self.assertEqual(cmds[-2:], ["partprobe /dev/nvme0n1", "udevadm settle --timeout=10"])
        root = plan.root()
        fmt = argvs(plan.format_steps(skip=(root,)))
        self.assertIn("mkfs.fat -F32 -n ESP /dev/nvme0n1p2", fmt)
        self.assertIn("mkswap /dev/nvme0n1p3", fmt)
        self.assertIn("mkfs.ext4 -F -L homeperso /dev/nvme0n1p5", fmt)
        self.assertFalse(any("p4" in c for c in fmt))
        ops = plan.describe()
        self.assertTrue(ops[0][0])
        self.assertEqual(ops[0][1], "op_new_table")

    def test_dualboot_manual_shrink_and_reuse_esp(self):
        plan = windows_disk()
        windows = plan.parts[2]
        plan.resize(windows, 120000)
        plan.ensure_boot_partitions()
        esp = plan.parts[0]
        self.assertEqual(esp.mountpoint, "/boot/efi")
        self.assertFalse(esp.format)
        region = next(r for r in plan.free_regions() if r[1] > 50000)
        plan.create(region[0], region[1], "ext4", "/")
        self.assertEqual(plan.validate(), [])
        steps = plan.build_steps()
        cmds = argvs(steps)
        shrink = cmds.index("ntfsresize --force --size 119999M /dev/nvme0n1p3")
        table = cmds.index("sfdisk --no-reread --no-tell-kernel -N 3 /dev/nvme0n1")
        self.assertLess(shrink, table)
        self.assertEqual(steps[table][1], ",120000MiB\n")
        self.assertFalse(any("mklabel" in c or " rm " in c for c in cmds))
        fmt = argvs(plan.format_steps(skip=(plan.root(),)))
        self.assertFalse(any("p1" in c for c in fmt), "l'ESP Windows ne doit pas être reformatée")
        kinds = [(d, k) for d, k, _ in plan.describe()]
        self.assertIn((True, "op_shrink"), kinds)
        self.assertIn((False, "op_mount"), kinds)
        bios = next(p for p in plan.parts if p.fstype == "bios_grub")
        self.assertEqual(bios.number, 5)
        self.assertEqual(plan.root().number, 6)

    def test_delete_frees_number_and_grow(self):
        plan = windows_disk()
        plan.delete(plan.parts[3])
        windows = plan.parts[2]
        self.assertGreater(plan.max_size(windows), 200000 + 600)
        plan.resize(windows, 200500)
        cmds = argvs(plan.build_steps())
        rm = cmds.index("parted -s /dev/nvme0n1 -- rm 4")
        grow = cmds.index("sfdisk --no-reread --no-tell-kernel -N 3 /dev/nvme0n1")
        fs = cmds.index("ntfsresize --force /dev/nvme0n1p3")
        self.assertLess(rm, grow)
        self.assertLess(grow, fs)
        start, size = plan.largest_free()
        new = plan.create(start, 1024, "ext4", "/data")
        self.assertEqual(new.number, 4)

    def test_validation_errors(self):
        plan = wp.parse_disk("/dev/sda", 64 * GIB, None, None)
        plan.new_partition_table("gpt")
        start, size = plan.largest_free()
        plan.create(start, 4096, "xfs", "/")
        keys = {k for k, _ in plan.validate()}
        self.assertEqual(keys, {"err_root_format", "err_root_small", "err_no_esp",
                                "err_no_bios_grub"})
        start, size = plan.largest_free()
        plan.create(start, 1024, "ext4", "/home")
        start, size = plan.largest_free()
        with self.assertRaises(wp.PlanError):
            plan.create(start, size + 1, "ext4")
        p = plan.create(start, 512, "ext4", "/home")
        self.assertIn("err_mount_dup", {k for k, _ in plan.validate()})
        plan.configure(p, mountpoint="/boot")
        self.assertIn("err_boot_separate", {k for k, _ in plan.validate()})

    def test_msdos_limits(self):
        plan = wp.parse_disk("/dev/sda", 64 * GIB, None, None)
        plan.new_partition_table("msdos")
        for _ in range(4):
            start, _size = plan.largest_free()
            plan.create(start, 1024, "ext4")
        start, _size = plan.largest_free()
        with self.assertRaises(wp.PlanError) as ctx:
            plan.create(start, 1024, "ext4")
        self.assertEqual(ctx.exception.key, "err_too_many")
        cmds = argvs(plan.build_steps())
        self.assertIn("parted -s /dev/sda -- mkpart primary ext4 1MiB 1025MiB", cmds)

    def test_existing_unresizable_needs_format(self):
        plan = wp.parse_disk("/dev/sda", 64 * GIB, {"partitiontable": {
            "label": "gpt", "sectorsize": SS, "partitions": [
                {"node": "/dev/sda1", "start": 2048, "size": mib_to_sectors(20000),
                 "type": "0FC63DAF-8483-4772-8E79-3D69D8477DE4"}]}},
            {"blockdevices": [{"name": "sda", "children": [
                {"name": "sda1", "fstype": "xfs", "label": "", "mountpoint": None}]}]})
        part = plan.parts[0]
        with self.assertRaises(wp.PlanError):
            plan.resize(part, 10000)
        plan.configure(part, format=True, fstype="ext4", mountpoint="/")
        plan.resize(part, 15000)
        self.assertTrue(part.resized())
        plan.configure(part, format=False)
        self.assertEqual(part.fstype, "xfs")
        self.assertIn("err_cannot_shrink", {k for k, _ in plan.validate()})

    def test_fstab(self):
        plan = windows_disk()
        plan.new_partition_table("gpt")
        plan.ensure_boot_partitions()
        start, _ = plan.largest_free()
        plan.create(start, 4096, "swap")
        start, _ = plan.largest_free()
        plan.create(start, 20480, "ext4", "/")
        start, size = plan.largest_free()
        plan.create(start, size, "ext4", "/home")
        fstab = plan.fstab(lambda path: "UUID-" + path.rsplit("p", 1)[-1])
        lines = [l for l in fstab.splitlines() if not l.startswith("#")]
        self.assertEqual(lines[0], "UUID=UUID-4 / ext4 defaults 0 1")
        self.assertIn("UUID=UUID-3 none swap sw 0 0", lines)
        self.assertIn("UUID=UUID-2 /boot/efi vfat umask=0077 0 2", lines)
        self.assertIn("UUID=UUID-5 /home ext4 defaults 0 2", lines)
        luks = plan.fstab(lambda path: "X", root_luks=True)
        self.assertIn("/dev/mapper/winlux_crypt / ext4 defaults 0 1", luks)

    def test_translations_complete(self):
        keys = set(wp.TRANSLATIONS["fr_FR"])
        for lang, table in wp.TRANSLATIONS.items():
            self.assertEqual(set(table), keys, lang)


class ScreenTests(unittest.TestCase):
    def test_common_screens(self):
        cases = [
            ((1366, 768, 0, 0), 1.0),
            ((1920, 1080, 344, 194), 1.25),
            ((1920, 1080, 531, 299), 1.0),
            ((1920, 1080, 0, 0), 1.25),
            ((2560, 1440, 597, 336), 1.0),
            ((2560, 1440, 0, 0), 1.5),
            ((3840, 2160, 597, 336), 1.5),
            ((3840, 2160, 344, 194), 2.5),
            ((3840, 2160, 0, 0), 2.5),
            ((1280, 720, 0, 0), 1.0),
        ]
        for args, expected in cases:
            self.assertEqual(ws.compute_scale(*args), expected, args)

    def test_small_screens_shrink(self):
        self.assertEqual(ws.compute_scale(1024, 600), 0.85)
        self.assertEqual(ws.compute_scale(800, 600), 0.7)
        self.assertLessEqual(ws.compute_scale(1024, 768), 1.0)

    def test_bogus_edid_ignored(self):
        self.assertEqual(ws.compute_scale(1920, 1080, 40, 30), 1.25)
        self.assertEqual(ws.compute_scale(1920, 1080, 1600, 900), 1.25)
        # Taille synthétique à 96 ppp (pas d'EDID) : on se base sur la résolution.
        self.assertEqual(ws.compute_scale(3840, 2160, 1016, 571), 2.5)

    def test_override(self):
        self.assertEqual(ws.scale_override({"WINLUX_UI_SCALE": "2"}, "/nonexistent"), 2.0)
        self.assertIsNone(ws.scale_override({}, "/nonexistent"))
        self.assertEqual(ws.compute_scale(800, 600, override=1.5), 1.5)

    def test_scale_css(self):
        css = b".a { padding: 24px 16px; color: #0078D4; border: 1px solid; font-size: 13px; }"
        out = ws.scale_css(css, 2.0)
        self.assertIn(b"padding: 48px 32px", out)
        self.assertIn(b"border: 2px solid", out)
        self.assertIn(b"font-size: 26px", out)
        self.assertIn(b"#0078D4", out)
        self.assertIn(b"1px", ws.scale_css(b"border: 1px;", 0.7))
        self.assertEqual(ws.scale_css(css, 1.0), css)


if __name__ == "__main__":
    unittest.main(verbosity=2)
