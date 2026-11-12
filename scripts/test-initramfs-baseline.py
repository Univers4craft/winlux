#!/usr/bin/env python3
"""#2146 : le petit initramfs démarre sur un processeur sans AVX.

Le runner ubuntu-26.04 installe des paquets amd64v3 (AVX2). build-live-root.sh
copiait busybox, cryptsetup et leurs bibliothèques depuis cette machine :
« invalid opcode » dans busybox dès /init, panique en boucle sur Atom, Celeron
ou Core 1re génération. Ils viennent maintenant de la racine native ($stage),
et check-initramfs-baseline.sh lance chaque exécutable sous qemu-x86_64 -cpu
Nehalem avant d'emballer l'initramfs.
"""
import shutil
import subprocess
import tempfile
from pathlib import Path

project = Path(__file__).resolve().parent.parent
root_script = (project / "scripts/build-live-root.sh").read_text()
code = "\n".join(line for line in root_script.splitlines() if not line.lstrip().startswith("#"))
# Rien de la machine de construction dans l'initramfs.
for host in ("cp /usr/bin/busybox", "/usr/bin/busybox --list", "ldd \"$cryptsetup\"", "command -v cryptsetup", "ldconfig -p"):
    assert host not in code, f"build-live-root.sh copie encore depuis l'hôte : {host}"
assert 'busybox="$stage/usr/bin/busybox"' in code
assert '"$loader" --library-path "$native_libs" --list "$cryptsetup"' in code
assert "bibliothèque hors de la racine native" in code
assert 'scripts/check-initramfs-baseline.sh" "$tiny"' in code
assert code.index("check-initramfs-baseline.sh") < code.index("cpio --null --create")
release = (project / ".github/workflows/release.yml").read_text()
assert "qemu-user" in release and "scripts/check-initramfs-baseline.sh" in release

check = project / "scripts/check-initramfs-baseline.sh"
compiler = shutil.which("cc") or shutil.which("gcc")
if shutil.which("qemu-x86_64") and compiler:
    with tempfile.TemporaryDirectory() as temp:
        temp = Path(temp)
        results = {}
        for name, body in (("ordinaire", "return 0;"),
                           ("avx", '__asm__ volatile("vpxor %%xmm0, %%xmm0, %%xmm0" ::: "xmm0"); return 0;')):
            source = temp / f"{name}.c"
            source.write_text(f"int main(void) {{ {body} }}\n")
            tree = temp / name / "bin"
            tree.mkdir(parents=True)
            subprocess.run([compiler, "-static", "-O0", "-o", str(tree / "outil"), str(source)], check=True)
            results[name] = subprocess.run(["sh", str(check), str(tree.parent)], capture_output=True, text=True)
        assert results["ordinaire"].returncode == 0, results["ordinaire"].stderr
        assert results["avx"].returncode != 0 and "plante sans AVX" in results["avx"].stderr, results["avx"]
    print("test-initramfs-baseline : OK (binaires de la racine native ; AVX refusé sous qemu-x86_64 -cpu Nehalem)")
else:
    print("test-initramfs-baseline : OK (vérifications statiques ; qemu-x86_64 ou cc absent)")
