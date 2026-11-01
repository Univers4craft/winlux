#!/usr/bin/env python3
"""Mesure ce qui manque à Winlux pour charger de vrais logiciels Windows.

Usage :
    winlux --exports > exports.tsv
    python3 scripts/compat-imports.py [--fourni=MOTIF,…] exports.tsv DOSSIER [DOSSIER…]

Chaque DOSSIER est un logiciel (ou un paquet) décompressé : ses .exe, .dll,
.pyd et .node sont lus avec pefile. Pour chacun, l'outil classe les imports :

- DLL livrée : présente dans le même dossier logiciel, chargée telle quelle ;
- DLL intégrée : fournie par Winlux, on vérifie alors chaque fonction ;
- DLL absente : ni intégrée ni livrée. Import direct = le programme ne
  démarre pas ; import différé (delay-load) = échec au premier appel.

La sortie Markdown donne, par logiciel, les DLL absentes et les fonctions
manquantes, puis deux classements globaux pondérés par le nombre de
logiciels touchés. Cela mesure le chargement, pas le comportement : une
fonction présente peut être un bouchon.
"""

import collections
import fnmatch
import os
import sys

import pefile

PE_EXT = (".exe", ".dll", ".pyd", ".node")

# Motifs (fnmatch) de DLL supposées livrées par l'écosystème de l'appli,
# par exemple « --fourni=python3*.dll » pour des modules Python.
ASSUMED = []


def normalize(dll: str) -> str:
    """Miroir de runtime/src/dlls/mod.rs::normalize."""
    base = dll.lower().replace("/", "\\").rsplit("\\", 1)[-1]
    if base.endswith(".dll"):
        base = base[:-4]
    if base.startswith("api-ms-win-crt-") or base in ("vcruntime140", "vcruntime140_1"):
        return "ucrtbase"
    if base.startswith("api-ms-win-core-com-l1-") or base.startswith("api-ms-win-core-winrt-"):
        return "ole32"
    if base.startswith("api-ms-win-core-localregistry-l1-"):
        return "advapi32"
    if base.startswith("api-ms-win-core-rtlsupport-l1-"):
        return "ntdll"
    if base.startswith("api-ms-win-power-"):
        return "powrprof"
    if (
        base.startswith("api-ms-win-core-")
        or base.startswith("api-ms-win-downlevel-kernel32-l2-")
        or base == "kernelbase"
    ):
        return "kernel32"
    if base.startswith("api-ms-win-security-base-l1-") or base.startswith(
        "api-ms-win-eventing-provider-l1-"
    ):
        return "advapi32"
    if base.startswith("msvcr") and base != "msvcrt":
        return "msvcrt"
    if base == "iertutil":
        return "comctl32"
    if base == "xaudio2_9redist":
        return "xaudio2_9"
    if base.startswith("xaudio2_") and base not in ("xaudio2_8", "xaudio2_9"):
        return "xaudio2_7"
    if base in ("winspool.drv", "winspool") or base.startswith("ext-ms-win-printer-winspool-"):
        return "winspool"
    if base.startswith("xinput1_") or base == "xinput9_1_0":
        return "xinput1_4"
    return base


def load_exports(path):
    exports = collections.defaultdict(set)
    with open(path, encoding="utf-8") as f:
        for line in f:
            dll, _, name = line.rstrip("\n").partition("\t")
            exports[dll].add(name)
    return exports


def lookup(exports, dll, func):
    if func in exports.get(dll, ()):
        return True
    if dll == "kernel32":
        return func in exports.get("ntdll", ()) or (
            func.startswith("Reg") and func in exports.get("advapi32", ())
        )
    return False


def pe_files(root):
    for dirpath, _, files in os.walk(root):
        for f in files:
            if f.lower().endswith(PE_EXT):
                yield os.path.join(dirpath, f)


def analyse(exports, root):
    shipped = {}
    for p in pe_files(root):
        shipped.setdefault(os.path.basename(p).lower(), p)
    shipped_norm = {normalize(n) for n in shipped}
    report = {
        "files": 0,
        "pe32": [],
        "missing_dll": collections.Counter(),
        "missing_dll_delay": collections.Counter(),
        "missing_func": collections.Counter(),
        "builtin_used": collections.Counter(),
    }
    for path in shipped.values():
        try:
            pe = pefile.PE(path, fast_load=True)
            pe.parse_data_directories(
                directories=[
                    pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_IMPORT"],
                    pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_DELAY_IMPORT"],
                ]
            )
        except Exception:
            continue
        report["files"] += 1
        if pe.FILE_HEADER.Machine != 0x8664:
            report["pe32"].append(os.path.relpath(path, root))
        for attr, delay in (("DIRECTORY_ENTRY_IMPORT", False), ("DIRECTORY_ENTRY_DELAY_IMPORT", True)):
            for entry in getattr(pe, attr, []):
                raw = entry.dll.decode("ascii", "replace")
                n = normalize(raw)
                # Comme le chargeur : une DLL intégrée passe avant la copie livrée.
                if n not in exports and (
                    raw.lower() in shipped
                    or n in shipped_norm
                    or any(fnmatch.fnmatch(raw.lower(), pat) for pat in ASSUMED)
                ):
                    continue
                if n not in exports:
                    key = raw.lower()
                    (report["missing_dll_delay"] if delay else report["missing_dll"])[key] += 1
                    continue
                report["builtin_used"][n] += 1
                for imp in entry.imports:
                    name = imp.name.decode("ascii", "replace") if imp.name else f"#{imp.ordinal}"
                    if not lookup(exports, n, name):
                        report["missing_func"][f"{n}!{name}"] += 1
    return report


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    args = sys.argv[1:]
    global ASSUMED
    while args and args[0].startswith("--fourni="):
        ASSUMED += [p.strip().lower() for p in args.pop(0)[9:].split(",") if p.strip()]
    exports = load_exports(args[0])
    roots = args[1:]
    glob_dll = collections.Counter()
    glob_dll_delay = collections.Counter()
    glob_func = collections.Counter()
    print("# Mesure des imports Winlux\n")
    print("| Logiciel | PE lus | 32 bits | DLL absentes (directes) | fonctions manquantes |")
    print("|---|---|---|---|---|")
    details = []
    for root in roots:
        r = analyse(exports, root)
        name = os.path.basename(os.path.normpath(root))
        print(
            f"| {name} | {r['files']} | {len(r['pe32'])} | "
            f"{len(r['missing_dll'])} | {len(r['missing_func'])} |"
        )
        glob_dll.update(r["missing_dll"].keys())
        glob_dll_delay.update(r["missing_dll_delay"].keys())
        glob_func.update(r["missing_func"].keys())
        details.append((name, r))
    print(
        f"\n{len(glob_dll)} DLL absentes en import direct, "
        f"{len(glob_func)} fonctions distinctes manquantes dans les DLL intégrées."
    )
    print("\n## DLL absentes, par nombre de logiciels touchés (import direct)\n")
    for dll, n in glob_dll.most_common():
        print(f"- {dll} : {n}")
    print("\n## DLL absentes en import différé\n")
    for dll, n in glob_dll_delay.most_common():
        print(f"- {dll} : {n}")
    print("\n## Fonctions manquantes dans les DLL intégrées (top 300)\n")
    for f, n in glob_func.most_common(300):
        print(f"- {f} : {n}")
    print("\n## Détail par logiciel\n")
    for name, r in details:
        print(f"### {name}\n")
        if r["pe32"]:
            print(f"- 32 bits : {len(r['pe32'])} fichiers (ex. {', '.join(r['pe32'][:3])})")
        print("- DLL absentes : " + (", ".join(f"{d} ({c})" for d, c in r["missing_dll"].most_common()) or "aucune"))
        print("- DLL absentes (différées) : " + (", ".join(r["missing_dll_delay"]) or "aucune"))
        per_dll = collections.Counter(f.split("!")[0] for f in r["missing_func"])
        print("- Fonctions manquantes par DLL : " + (", ".join(f"{d} {c}" for d, c in per_dll.most_common()) or "aucune"))
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
