# #3053 : Paramètres › À propos montre le temps du dernier démarrage tel que
# l'init l'a mesuré (/proc/uptime noté à chaque étape), jamais une valeur
# inventée, et les quatre caractéristiques du PC détectées.
from importlib.machinery import SourceFileLoader as _Loader_3053
import importlib.util as _util_3053
_root_3053 = Path(tempfile.mkdtemp(prefix="campagne-3053-"))
(_root_3053 / "run/winlux").mkdir(parents=True)
(_root_3053 / "var/lib/winlux").mkdir(parents=True)
(_root_3053 / "run/winlux/boot-times").write_text("init 1.5\ndrivers 4.9\nxorg 7.1\ndesktop 8.42\n")
(_root_3053 / "var/lib/winlux/boot-history").write_text("1 12000\n2 8420\n")
_saved_3053 = os.environ.get("WINLUX_BOOT_ROOT")
os.environ["WINLUX_BOOT_ROOT"] = str(_root_3053)
try:
    _loader_3053 = _Loader_3053("campagne_3053", str(SCRIPTS / "winlux-hardware-page.py"))
    _hw_3053 = _util_3053.module_from_spec(_util_3053.spec_from_loader("campagne_3053", _loader_3053))
    _loader_3053.exec_module(_hw_3053)
finally:
    if _saved_3053 is None: os.environ.pop("WINLUX_BOOT_ROOT", None)
    else: os.environ["WINLUX_BOOT_ROOT"] = _saved_3053
_boot_3053 = _hw_3053.boot_facts()
(_root_3053 / "run/winlux/boot-times").write_text("init 1.5\ndrivers 4.9\n")
_fini_3053 = _hw_3053.boot_facts()
_tuiles_3053 = [t[0] for t in _hw_3053.highlights({"pc_cpu_model": "AMD Ryzen 5 5600X 6-Core Processor", "pc_cpu_cores": "6",
                                                    "pc_cpu_threads": "12", "pc_memory_kb": "16000000", "pc_disk_count": "1",
                                                    "pc_disk0_kind": "nvme", "pc_disk0_size": "1000000000000"})]
print(f"      démarrage : {_boot_3053} ; pas fini : {_fini_3053} ; tuiles : {_tuiles_3053}")
ok(_boot_3053.get("total_ms") == 8420 and _boot_3053.get("previous_ms") == 12000 and _boot_3053.get("count") == 2
   and sum(ms for _, ms in _boot_3053["phases"]) == 8420 and _fini_3053 == {}
   and _tuiles_3053 == ["cpu", "memory", "drive"],
   "À propos de ce PC : démarrage mesuré par l'init (rien avant le bureau), processeur, mémoire et stockage détectés (#3053)")
