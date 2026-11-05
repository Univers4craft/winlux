#!/usr/bin/python3
"""Finitions de l'installateur relevées en VM sur la v0.2.15.

- pas de veille d'écran pendant l'installation (xset -dpms, économiseur) ;
- accent violet Prisme sur la barre des partitions et les boutons radio ;
- la note de performance n'apparaît qu'une fois la mesure complète ;
- Paramètres > Mises à jour ne parle plus de « VM ».
"""
import ast
from pathlib import Path
import types

project = Path(__file__).resolve().parent.parent
source = (project / "scripts/winlux-installer.py").read_text()
tree = ast.parse(source)

# Veille : coupée au début, rétablie à la fin (succès comme erreur).
assert "self.keep_awake(True)" in source.split("def start_installation", 1)[1].split("def ", 1)[0]
assert "self.keep_awake(False)" in source.split("def show_done", 1)[1].split("def ", 1)[0]
assert "self.keep_awake(False)" in source.split("def install_error", 1)[1].split("\ndef ", 1)[0]
for argv in ('"xset", "-dpms"', '"xset", "s", "off"', '"xscreensaver-command", "-deactivate"', '"xset", "+dpms"'):
    assert argv in source, argv

# Réveil pendant la copie : un fil à part (pas un minuteur GLib) envoie les
# trois signaux jusqu'à la fin. En v0.2.16, l'économiseur démarrait encore.
import os, shutil, subprocess, tempfile, threading, time  # noqa: E401,E402
cls_k = next(n for n in tree.body if isinstance(n, ast.ClassDef) and any(
    isinstance(f, ast.FunctionDef) and f.name == "keep_awake" for f in n.body))
keep = next(f for f in cls_k.body if isinstance(f, ast.FunctionDef) and f.name == "keep_awake")
ns = {"shutil": shutil, "subprocess": subprocess, "threading": threading, "os": os}
exec(compile(ast.Module(body=[keep], type_ignores=[]), "installer", "exec"), ns)
with tempfile.TemporaryDirectory() as bindir:
    log = Path(bindir) / "log"
    for tool in ("xset", "xscreensaver-command", "xdotool", "xscreensaver"):
        script = Path(bindir) / tool
        script.write_text(f'#!/bin/sh\necho "{tool} $*" >>"{log}"\n')
        script.chmod(0o755)
    old_path = os.environ["PATH"]
    os.environ["PATH"] = bindir + os.pathsep + old_path
    try:
        inhibit_dir = Path(bindir) / "power-inhibit"
        app = types.SimpleNamespace(awake_interval=0.2, inhibit_dir=str(inhibit_dir), saver_log=str(Path(bindir) / "saver.log"))
        ns["keep_awake"](app, True)
        ns["keep_awake"](app, True)  # un seul fil même si appelé deux fois
        inhibit = inhibit_dir / f"{os.getpid()}-installer"
        assert inhibit.read_text().strip() == "0x80000003", "pas de demande d'éveil au démon d'alimentation"
        time.sleep(1.0)
        ns["keep_awake"](app, False)
        assert not inhibit.exists(), "demande d'éveil laissée après l'installation"
        time.sleep(0.3)
        calls = log.read_text().splitlines()
        time.sleep(0.5)
        assert log.read_text().splitlines() == calls, "le fil continue après la fin"
    finally:
        os.environ["PATH"] = old_path
    assert calls[:4] == ["xset s off", "xset -dpms", "xscreensaver-command -deactivate", "xscreensaver-command -exit"], calls
    assert calls.count("xscreensaver-command -exit") == 1, calls
    assert calls[-1] == "xscreensaver -no-splash", "économiseur non relancé après l'installation"
    calls = calls[:-1]
    pokes = [c for c in calls if c.startswith("xdotool mousemove_relative -- 1 0")]
    assert 3 <= len(pokes) <= 7, calls
    assert calls.count("xset s reset") == len(pokes) and calls[-2:] == ["xset s default", "xset +dpms"], calls
    print("installation : économiseur arrêté puis relancé, demande d'éveil au démon d'alimentation")

# Couleurs : plus de bleu Windows pour ext4, accent Prisme sur les contrôles.
colors = next(n.value for n in tree.body if isinstance(n, ast.Assign) and getattr(n.targets[0], "id", "") == "PART_COLORS")
colors = ast.literal_eval(colors)
assert colors["ext4"][:3] == (0.416, 0.239, 0.941), colors["ext4"]
assert all(rgb[:3] != (0.0, 0.47, 0.83) for rgb in colors.values())
# La partition « Winlux » que l'installateur va créer : violette aussi (v0.2.16).
assert colors["winlux"][:3] == (0.416, 0.239, 0.941), colors["winlux"]
assert all(rgb[2] < 0.9 or rgb[0] > 0.3 for rgb in colors.values()), colors  # aucun bleu
css = next(n.value.value for n in tree.body if isinstance(n, ast.Assign) and getattr(n.targets[0], "id", "") == "CSS")
assert b"@define-color accent #6A3DF0;" in css and b"radio:checked" in css and b"check:checked" in css

# Note de performance : seulement une mesure complète.
cls = next(n for n in tree.body if isinstance(n, ast.ClassDef) and any(
    isinstance(f, ast.FunctionDef) and f.name == "benchmark_complete" for f in n.body))
func = next(f for f in cls.body if isinstance(f, ast.FunctionDef) and f.name == "benchmark_complete")
namespace = {}
exec(compile(ast.Module(body=[func], type_ignores=[]), "installer", "exec"), namespace)
complete = namespace["benchmark_complete"]
full = {"results": {"cpu": {"score": 6.1}, "ram": {"score": 5.0}, "gpu": {"score": 2.0}}, "base_score": 2.0}
assert complete(types.SimpleNamespace(bench_state="done", benchmark=full))
assert not complete(types.SimpleNamespace(bench_state="running", benchmark=full))
assert not complete(types.SimpleNamespace(bench_state="done", benchmark=None))
assert not complete(types.SimpleNamespace(bench_state="done", benchmark={"results": {"cpu": {"score": 2.0}}, "base_score": 2.0}))
failed = {"results": {**full["results"], "gpu": {"score": 1.0, "error": "x"}}, "base_score": 1.0}
assert not complete(types.SimpleNamespace(bench_state="done", benchmark=failed))
assert source.count('"summary_benchmark_running":') == 6
assert "self.benchmark_complete()" in source
# Note globale = la plus basse : le récapitulatif nomme le composant qui la fixe.
assert source.count('"summary_benchmark_limited":') == 6
assert 'tr("summary_benchmark_limited", score=score, part=tr("bench_" + weakest))' in source
# Page Performances aussi : la grande note dit quel composant la fixe.
assert source.count('"bench_limited_by":') == 6
assert 'tr("bench_limited_by", part=tr("bench_" + weakest))' in source
weak = next(f for f in cls.body if isinstance(f, ast.FunctionDef) and f.name == "benchmark_weakest")
exec(compile(ast.Module(body=[weak], type_ignores=[]), "installer", "exec"), namespace)
assert namespace["benchmark_weakest"](types.SimpleNamespace(benchmark=full)) == "gpu"
same = {"results": {"cpu": {"score": 5.0}, "gpu": {"score": 5.0}}}
assert namespace["benchmark_weakest"](types.SimpleNamespace(benchmark=same)) is None
assert namespace["benchmark_weakest"](types.SimpleNamespace(benchmark=None)) is None

settings = (project / "scripts/winlux-settings.py").read_text()
assert "Gardez la VM" not in settings and "Keep the VM running" not in settings
print("test-installer-finitions : OK (veille, accent Prisme, note complète, texte des mises à jour)")

# « Note de performance : 2,0 » sans explication (testeur VM) : échelle et sens.
import winlux_benchmark  # noqa: E402
helpers = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name in ("benchmark_level", "benchmark_text")] \
    if "tree" in dir() else [n for n in ast.parse(source).body
                             if isinstance(n, ast.FunctionDef) and n.name in ("benchmark_level", "benchmark_text")]
assert len(helpers) == 2, "benchmark_level / benchmark_text introuvables"
texts = {"summary_benchmark_scale": "{score} sur {top}", "bench_level_1": "usage léger", "bench_level_2": "courant",
         "bench_level_3": "confortable", "bench_level_4": "très rapide"}
scope = {"tr": lambda key, **kw: texts[key].format(**kw), "winlux_benchmark": winlux_benchmark}
exec(compile(ast.Module(body=helpers, type_ignores=[]), "installer", "exec"), scope)
assert scope["benchmark_text"](2.0) == "2,0 sur 9,9, usage léger", scope["benchmark_text"](2.0)
assert scope["benchmark_level"](4.9) == "bench_level_2" and scope["benchmark_level"](5.0) == "bench_level_3"
assert scope["benchmark_level"](8.7) == "bench_level_4"
for key in texts:
    assert source.count(f'"{key}":') == 6, f"{key} : pas en 6 langues"
assert 'score = benchmark_text(self.benchmark["base_score"])' in source
assert 'tr("bench_done") + "\\n" + benchmark_text(self.benchmark["base_score"])' in source
print("note de performance : « 2,0 sur 9,9, pour un usage léger… » et ce qui la limite, en 6 langues")
