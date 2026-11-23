# #1935 : un écran tactile est reconnu à INPUT_PROP_DIRECT et à ses axes
# absolus, pas à son nom : « silead_ts » (tablettes Atom) n'a ni « Touchscreen »
# ni « Digitizer ». Un pavé tactile (INPUT_PROP_POINTER) n'en est pas un.
_hw_1935 = (SCRIPTS / "winlux-hardware.sh").read_text(encoding="utf-8")
_debut_1935 = _hw_1935.find("is_touchscreen() {")
_fn_1935 = _hw_1935[_debut_1935:_hw_1935.index("\n}\n", _debut_1935) + 3] if _debut_1935 >= 0 else "is_touchscreen() { return 2; }\n"
_cas_1935 = {
    # nom, properties, capabilities/abs, attendu
    "silead_ts": ("silead_ts", "2", "6608000 3", True),       # DIRECT + ABS_X + ABS_MT_POSITION_X
    "Goodix": ("Goodix Capacitive TouchScreen", "0", "0", True),  # secours par le nom
    "pavé": ("SYNA3602:00 Touchpad", "5", "6608000 3", False),  # 0x5 = POINTER|BUTTONPAD, sans DIRECT
    "clavier": ("AT Translated Set 2 keyboard", "0", "0", False),
    "stylet": ("Wacom HID 5256 Pen", "2", "1000003", True),
}
_vus_1935 = {}
with tempfile.TemporaryDirectory() as _d_1935:
    for _k, (_nom, _props, _abs, _att) in _cas_1935.items():
        _dev = Path(_d_1935) / _k
        (_dev / "capabilities").mkdir(parents=True)
        (_dev / "name").write_text(_nom + "\n")
        (_dev / "properties").write_text(_props + "\n")
        (_dev / "capabilities" / "abs").write_text(_abs + "\n")
        _r = subprocess.run(["sh", "-c", _fn_1935 + 'is_touchscreen "$1"', "sh", str(_dev)])
        _vus_1935[_k] = _r.returncode == 0
for _k, _v in _vus_1935.items():
    print(f"      {_k} : {'écran tactile' if _v else 'non'}")
ok(all(_vus_1935[k] == v[3] for k, v in _cas_1935.items()),
   "Écran tactile reconnu à INPUT_PROP_DIRECT, pas seulement à son nom (#1935)")
