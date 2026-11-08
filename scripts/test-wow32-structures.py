#!/usr/bin/env python3
"""Inventaire des structures #[repr(C)] à champs de taille pointeur (#232).

Une structure Windows qui contient un pointeur, un HANDLE ou un usize n'a
pas la même taille pour un programme x86 (4 octets par pointeur) que pour
le runtime 64 bits. Si la passerelle wow32 l'écrit telle quelle dans la
mémoire d'un appelant 32 bits, elle déborde (WSAStartup, #779).

Ce test relève toutes ces structures dans runtime/src/dlls et exige que
chacune figure dans docs/compat/wow32-structures.tsv avec un état :
  x86        convertie pour les appelants 32 bits, preuve = test qui le vérifie ;
  interne    n'atteint jamais la mémoire d'un programme (Vulkan, tests…) ;
  a-verifier pas encore relue.
Une structure nouvelle, renommée ou disparue fait échouer le test, pour que
la question de sa taille x86 soit posée au moment où elle apparaît.
Option --generer : réécrit l'inventaire en gardant les états connus.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DLLS = ROOT / "runtime" / "src" / "dlls"
TSV = ROOT / "docs" / "compat" / "wow32-structures.tsv"
STRUCT = re.compile(
    r"#\[repr\(C[^\]]*\)\]\s*(?:#\[[^\]]*\]\s*)*(?:pub(?:\([^)]*\))?\s+)?"
    r"struct\s+(\w+)\s*\{(.*?)\n\s*\}",
    re.S,
)
POINTER = re.compile(r"\busize\b|\bisize\b|\*mut\b|\*const\b|\bHANDLE\b|\bHWND\b|Option<")
STATES = {"x86", "interne", "a-verifier"}
HEADER = (
    "# Structures #[repr(C)] à champs de taille pointeur dans runtime/src/dlls (#232).\n"
    "# Vérifié par scripts/test-wow32-structures.py ; une ligne par structure.\n"
    "# fichier<TAB>structure<TAB>état (x86, interne, a-verifier)<TAB>preuve ou remarque\n"
)


def found():
    out = set()
    for f in sorted(DLLS.rglob("*.rs")):
        text = f.read_text(encoding="utf-8")
        for m in STRUCT.finditer(text):
            if POINTER.search(m.group(2)):
                out.add((f.relative_to(DLLS).as_posix(), m.group(1)))
    return out


def listed():
    rows = {}
    errors = []
    for n, line in enumerate(TSV.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        cols = line.split("\t")
        if len(cols) != 4:
            errors.append(f"ligne {n} : 4 colonnes attendues, {len(cols)} trouvées")
            continue
        key = (cols[0], cols[1])
        if key in rows:
            errors.append(f"ligne {n} : {cols[0]} {cols[1]} en double")
        if cols[2] not in STATES:
            errors.append(f"ligne {n} : état « {cols[2]} » inconnu")
        if cols[2] == "x86" and not cols[3].strip():
            errors.append(f"ligne {n} : {cols[1]} marquée x86 sans preuve")
        rows[key] = (cols[2], cols[3])
    return rows, errors


def main():
    have = found()
    if "--generer" in sys.argv:
        old = listed()[0] if TSV.exists() else {}
        lines = [HEADER]
        for key in sorted(have):
            state, note = old.get(key, ("a-verifier", ""))
            lines.append(f"{key[0]}\t{key[1]}\t{state}\t{note}\n")
        TSV.write_text("".join(lines), encoding="utf-8")
        print(f"{len(have)} structures écrites dans {TSV.relative_to(ROOT)}")
        return 0
    rows, errors = listed()
    for key in sorted(have - rows.keys()):
        errors.append(
            f"{key[0]} : structure {key[1]} absente de l'inventaire ; quelle est sa taille x86 ? "
            "(ajoutez-la, puis scripts/test-wow32-structures.py --generer)"
        )
    for key in sorted(rows.keys() - have):
        errors.append(f"{key[0]} : {key[1]} n'existe plus, retirez-la de l'inventaire")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    count = {s: sum(1 for v in rows.values() if v[0] == s) for s in sorted(STATES)}
    print(
        f"Structures wow32 : {len(rows)} inventoriées, {count['x86']} converties x86, "
        f"{count['interne']} internes, {count['a-verifier']} à vérifier"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
