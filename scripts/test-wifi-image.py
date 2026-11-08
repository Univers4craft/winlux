#!/usr/bin/python3
"""Wi-Fi chiffré dans l'image (v0.2.17, PC de damien : mot de passe « refusé »).

L'image ne contenait aucun module crypto du noyau : sans ccm, mac80211 ne
pose pas la clé CCMP, la poignée de main WPA2 échoue et NetworkManager
redemande le mot de passe. regulatory.db manquait aussi, et 54 pilotes Wi-Fi
(SDIO des tablettes, USB, staging). Vérifie :
- build-live-iso.sh copie crypto, la pile 802.11, tous les pilotes Wi-Fi et
  staging, puis appelle check-image-wifi.sh ;
- wireless-regdb fait partie des paquets ;
- check-image-wifi.sh échoue sans ccm, sans entrée depmod ou sans
  regulatory.db, et passe quand tout y est.
"""
import os
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
failures = []


def check(cond, message):
    if not cond:
        failures.append(message)


build = (HERE / "build-live-iso.sh").read_text()
loop = next((line for line in build.splitlines() if line.startswith("for directory in sound")), "")
for directory in ("crypto", "arch/x86/crypto", "lib/crypto", "net/wireless", "net/mac80211", "drivers/net/wireless",
                  "drivers/staging", "net/netfilter", "net/ipv4/netfilter", "net/ipv6/netfilter", "net/bridge",
                  "drivers/net/usb", "net/bluetooth"):
    check(f" {directory}" in loop, f"build-live-iso.sh ne copie pas kernel/{directory}")
check('scripts/check-image-wifi.sh" "$stage" "$kernel_release"' in build, "build-live-iso.sh n'appelle pas check-image-wifi.sh")
check(build.index("check-image-wifi.sh") > build.index("stage-desktop-components.sh"),
      "check-image-wifi.sh doit passer après les firmwares (regulatory.db)")
check('"wireless-regdb"' in (HERE / "prepare-desktop-packages.py").read_text(), "paquet wireless-regdb absent")

release = "7.0.0-42-generic"
tmp = Path(tempfile.mkdtemp(prefix="winlux-wifi-"))
host = tmp / "host"
NAMES = {"ccm": "crypto", "cmac": "crypto", "michael_mic": "crypto", "gcm": "crypto",
         "mac80211": "net/mac80211", "cfg80211": "net/wireless",
         "brcmfmac-cyw": "drivers/net/wireless/broadcom/brcm80211/brcmfmac/cyw",
         "r8723bs": "drivers/staging/rtl8723bs", "libarc4": "lib/crypto",
         "nf_tables": "net/netfilter", "ipheth": "drivers/net/usb"}
for name, directory in NAMES.items():
    (host / release / "kernel" / directory).mkdir(parents=True, exist_ok=True)
    (host / release / "kernel" / directory / f"{name}.ko.zst").write_bytes(b"x")


VPN_TOOLS = ("usr/sbin/openvpn", "usr/lib/NetworkManager/VPN/nm-openvpn-service.name", "usr/sbin/dnsmasq",
             "usr/sbin/nft", "usr/sbin/iw", "usr/bin/winlux-vpn", "usr/bin/winlux-hotspot")


def stage(vpn_tools=VPN_TOOLS, with_ccm=True, depmod=True, regdb=True, nf=True, supplicant=b"SAE PEAP TTLS MSCHAPV2 WPS", certs=True,
          roots=150, conf=True, hashed=True, tools=("usr/sbin/update-ca-certificates", "usr/bin/openssl", "usr/bin/certutil",
                                       "usr/bin/winlux-certificates")):
    root = Path(tempfile.mkdtemp(dir=tmp))
    modules = root / "lib/modules" / release
    for name, directory in NAMES.items():
        if (name == "ccm" and not with_ccm) or (name == "nf_tables" and not nf):
            continue
        (modules / "kernel" / directory).mkdir(parents=True, exist_ok=True)
        (modules / "kernel" / directory / f"{name}.ko").write_bytes(b"x")
    (modules / "modules.dep").write_text(("kernel/crypto/ccm.ko:\n" if depmod else "kernel/crypto/cmac.ko:\n")
                                         + "kernel/net/netfilter/nf_tables.ko:\n")
    for binary in ("usr/sbin/NetworkManager", "usr/bin/nmcli", *vpn_tools):
        (root / binary).parent.mkdir(parents=True, exist_ok=True)
        (root / binary).write_bytes(b"\x7fELF")
    if supplicant is not None:
        (root / "usr/sbin/wpa_supplicant").write_bytes(b"\x7fELF\0" + supplicant)
    if certs:
        (root / "etc/ssl/certs").mkdir(parents=True)
        (root / "etc/ssl/certs/ca-certificates.crt").write_text("-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----\n" * roots)
        mozilla = root / "usr/share/ca-certificates/mozilla"
        mozilla.mkdir(parents=True)
        for index in range(roots): (mozilla / f"Racine_{index}.crt").write_text("x")
        if hashed:
            for index in range(roots): (root / f"etc/ssl/certs/{index:08x}.0").write_text("x")
        if conf:
            (root / "etc/ca-certificates.conf").write_text("".join(f"mozilla/Racine_{i}.crt\n" for i in range(roots)))
    for binary in tools:
        (root / binary).parent.mkdir(parents=True, exist_ok=True)
        (root / binary).write_bytes(b"\x7fELF")
    (root / "usr/lib/firmware").mkdir(parents=True)
    os.symlink("../usr/lib/firmware", root / "lib/firmware")
    if regdb:
        (root / "usr/lib/firmware/regulatory.db").write_bytes(b"RGDB")
    return root


def run(root):
    return subprocess.run(["sh", str(HERE / "check-image-wifi.sh"), str(root), release],
                          env={**os.environ, "WINLUX_HOST_MODULES": str(host)},
                          capture_output=True, text=True)


result = run(stage())
check(result.returncode == 0, f"image complète refusée : {result.stderr}")
result = run(stage(with_ccm=False))
check(result.returncode != 0 and "ccm" in result.stderr, f"ccm absent non détecté : {result.stderr!r}")
result = run(stage(depmod=False))
check(result.returncode != 0 and "depmod" in result.stderr, f"ccm hors modules.dep non détecté : {result.stderr!r}")
result = run(stage(regdb=False))
check(result.returncode != 0 and "regulatory.db" in result.stderr, f"regulatory.db absent non détecté : {result.stderr!r}")
result = run(stage(nf=False))
check(result.returncode != 0 and "nf_tables" in result.stderr, f"pare-feu (nf_tables) absent non détecté : {result.stderr!r}")
result = run(stage(supplicant=None))
check(result.returncode != 0 and "wpa_supplicant" in result.stderr, f"wpa_supplicant absent non détecté : {result.stderr!r}")
result = run(stage(supplicant=b"PEAP TTLS MSCHAPV2 WPS"))
check(result.returncode != 0 and "SAE" in result.stderr, f"wpa_supplicant sans WPA3 (SAE) non détecté : {result.stderr!r}")
result = run(stage(certs=False))
check(result.returncode != 0 and "magasin racine absent" in result.stderr, f"certificats racine absents non détectés : {result.stderr!r}")
result = run(stage(roots=12))
check(result.returncode != 0 and "rachitique" in result.stderr, f"magasin rachitique (12 autorités) non détecté : {result.stderr!r}")
result = run(stage(conf=False))
check(result.returncode != 0 and "ca-certificates.conf" in result.stderr, f"ca-certificates.conf absent non détecté : {result.stderr!r}")
result = run(stage(hashed=False))
check(result.returncode != 0 and "empreinte" in result.stderr, f"/etc/ssl/certs sans liens par empreinte non détecté : {result.stderr!r}")
result = run(stage(tools=("usr/sbin/update-ca-certificates", "usr/bin/openssl", "usr/bin/winlux-certificates")))
check(result.returncode != 0 and "certutil" in result.stderr, f"certutil absent non détecté : {result.stderr!r}")
result = run(stage(vpn_tools=tuple(t for t in VPN_TOOLS if t != "usr/sbin/dnsmasq")))
check(result.returncode != 0 and "dnsmasq" in result.stderr, f"dnsmasq absent (point d'accès) non détecté : {result.stderr!r}")
result = run(stage(vpn_tools=tuple(t for t in VPN_TOOLS if "openvpn" not in t)))
check(result.returncode != 0 and "openvpn" in result.stderr, f"OpenVPN absent non détecté : {result.stderr!r}")
packages = (HERE / "prepare-desktop-packages.py").read_text()
for package in ("network-manager-openvpn", "openvpn", "dnsmasq-base", "nftables", "iw"):
    check(f'"{package}"' in packages, f"{package} absent des paquets (VPN, point d'accès)")
stage_script = (HERE / "stage-desktop-components.sh").read_text()
check(" winlux-vpn winlux-hotspot " in stage_script, "winlux-vpn et winlux-hotspot non installés dans l'image")
check("ca-certificates.conf" in stage_script, "l'image ne crée pas /etc/ca-certificates.conf")
check("openssl rehash" in stage_script, "l'image ne crée pas les liens par empreinte de /etc/ssl/certs")
check('"libnss3-tools"' in (HERE / "prepare-desktop-packages.py").read_text(), "certutil (libnss3-tools) absent des paquets")

if failures:
    for failure in failures:
        print("ÉCHEC :", failure)
    raise SystemExit(1)
print("OK : Wi-Fi dans l'image (WEP à WPA3, Entreprise, pilotes, regulatory.db, pare-feu, partage de connexion) et certificats complets")
