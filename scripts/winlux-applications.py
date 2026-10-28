#!/usr/bin/python3
"""XDG launchers for real installed browsers, registered Windows apps and PEs.

No shell commands from registry/shortcut files are evaluated. Windows arguments
are encoded as separate Desktop Entry Exec arguments. Only our own generated
launchers are replaced or removed.
"""
import ctypes
import fcntl
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys

HOME = Path.home()
DATA = Path(os.environ.get("XDG_DATA_HOME", HOME / ".local/share"))
APPS = DATA / "applications"
STATE = DATA / "winlux"
EXE_LAUNCHER = "/usr/bin/winlux-launch-exe"

def escaped(value):
    return str(value).replace("\\", "\\\\").replace("\n", "\\n").replace("\r", "\\r")

def argument(value):
    value = str(value).replace("%", "%%")
    for char in ["\\", '"', "`", "$"]:
        value = value.replace(char, "\\" + char)
    # Exec has both Desktop Entry value escaping and command-line quoting.
    return escaped('"' + value + '"')

def save(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.is_file() and not path.is_symlink() and path.read_text(errors="replace") == text:
        return
    temporary = path.with_name(path.name + f".tmp.{os.getpid()}")
    with temporary.open("x", encoding="utf-8") as stream:
        stream.write(text)
    temporary.chmod(0o644)
    temporary.replace(path)

def managed_desktop(path, text):
    if path.is_file():
        current = path.read_text(errors="replace")
        marker = next((line.split("=", 1)[1] for line in current.splitlines() if line.startswith("X-Winlux-Generated-Hash=")), None)
        original = "\n".join(line for line in current.splitlines() if not line.startswith("X-Winlux-Generated-Hash=")) + "\n"
        if marker != hashlib.sha256(original.encode()).hexdigest():
            return  # The user edited or hid this launcher: retain their changes.
    digest = hashlib.sha256(text.encode()).hexdigest()
    text = text.replace("[Desktop Entry]\n", f"[Desktop Entry]\nX-Winlux-Generated-Hash={digest}\n", 1)
    save(path, text)

def windows_path(value, home):
    value = value.strip().strip('"').replace("\\", "/")
    if value[:2].lower() == "c:":
        return home / value[2:].lstrip("/")
    if value[:2].lower() == "z:":
        return Path("/") / value[2:].lstrip("/")
    return Path(value) if value.startswith("/") else home / value

def pe_icon(path):
    """Rebuild the first resource icon as ICO; do not execute the PE."""
    data = path.read_bytes() if path.stat().st_size <= 256 * 1024 * 1024 else b""
    if len(data) < 64 or data[:2] != b"MZ":
        return None
    def u16(offset): return struct.unpack_from("<H", data, offset)[0]
    def u32(offset): return struct.unpack_from("<I", data, offset)[0]
    pe = u32(60)
    if data[pe:pe + 4] != b"PE\0\0" or u16(pe + 22) & 0x2000:
        return None
    optional = pe + 24
    directory = optional + (112 if u16(optional) == 0x20b else 96)
    rva = u32(directory + 16)
    sections = optional + u16(pe + 20)
    def file_offset(address):
        for index in range(min(u16(pe + 6), 96)):
            section = sections + index * 40
            start = u32(section + 12)
            if start <= address < start + max(u32(section + 8), u32(section + 16)):
                offset = u32(section + 20) + address - start
                if offset < len(data): return offset
        raise ValueError("Resource outside PE sections")
    base = file_offset(rva)
    def children(relative):
        offset = base + relative
        count = u16(offset + 12) + u16(offset + 14)
        if count > 4096: raise ValueError("Too many resource entries")
        return [(u32(offset + 16 + i * 8), u32(offset + 20 + i * 8)) for i in range(count)]
    def resources(kind):
        found = {}
        for resource_type, target in children(0):
            if resource_type != kind or not target & 0x80000000: continue
            for name, branch in children(target & 0x7fffffff):
                if not branch & 0x80000000: continue
                for _, leaf in children(branch & 0x7fffffff):
                    if leaf & 0x80000000: continue
                    address = base + leaf
                    start, size = file_offset(u32(address)), u32(address + 4)
                    if size > 4 * 1024 * 1024 or start + size > len(data): raise ValueError("Invalid icon size")
                    found[name] = data[start:start + size]
                    break
        return found
    groups, images = resources(14), resources(3)
    if not groups: return None
    group = next(iter(groups.values()))
    count = struct.unpack_from("<H", group, 4)[0]
    if not 0 < count <= 64 or len(group) < 6 + 14 * count: return None
    entries, payload, offset = [], [], 6 + 16 * count
    for i in range(count):
        entry = group[6 + i * 14:20 + i * 14]
        image = images[struct.unpack_from("<H", entry, 12)[0]]
        entries.append(entry[:8] + struct.pack("<II", len(image), offset))
        payload.append(image)
        offset += len(image)
    return struct.pack("<HHH", 0, 1, count) + b"".join(entries + payload)

def valid_exe(path):
    try:
        if not path.is_file(): return False
        with path.open("rb") as stream:
            header = stream.read(64)
            if len(header) < 64 or header[:2] != b"MZ": return False
            offset = struct.unpack_from("<I", header, 60)[0]
            if offset > 16 * 1024 * 1024: return False
            stream.seek(offset)
            header = stream.read(24)
            return len(header) == 24 and header[:4] == b"PE\0\0" and not struct.unpack_from("<H", header, 22)[0] & 0x2000
    except (OSError, ValueError, struct.error): return False

def icon_for(path, identity):
    output = HOME / ".cache/winlux/icons" / (identity + ".ico")
    try:
        stamp = output.with_suffix(".stamp")
        metadata = f"{path.stat().st_mtime_ns}:{path.stat().st_size}"
        if stamp.is_file() and stamp.read_text() == metadata:
            return str(output) if output.is_file() else "application-x-executable"
        if output.is_file() and output.stat().st_mtime_ns >= path.stat().st_mtime_ns:
            return str(output)
        icon = pe_icon(path)
        output.parent.mkdir(parents=True, exist_ok=True)
        stamp.write_text(metadata)
        if icon:
            output.write_bytes(icon)
            return str(output)
    except (OSError, ValueError, struct.error, KeyError): pass
    return "application-x-executable"

def window_icons():
    """Publish extracted PE icons to EWMH, where the native tasklist reads them."""
    try:
        windows = subprocess.check_output(["wmctrl", "-lp"], text=True, timeout=2)
    except (OSError, subprocess.SubprocessError): return
    cache = Path("/run/winlux/window-icons.json")
    try: known = json.loads(cache.read_text())
    except (OSError, ValueError): known = {}
    current = {}
    for line in windows.splitlines():
        fields = line.split(None, 4)
        if len(fields) < 3: continue
        window, pid = fields[0], fields[2]
        if not pid.isdigit() or pid == "0": continue
        try:
            args = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
            if Path(os.fsdecode(args[0])).name != "winlux": continue
            target = next((Path(os.fsdecode(arg)) for arg in args[1:] if arg.lower().endswith(b".exe")), None)
            if target is None or not target.is_file(): continue
            identity = hashlib.sha256(str(target).encode()).hexdigest()[:24]
            key = f"{pid}:{target.stat().st_mtime_ns}"
            current[window] = key
            if known.get(window) == key: continue
            icon = icon_for(target, identity)
            if icon == "application-x-executable":
                environment = Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
                mode = next((item.split(b"=", 1)[1].decode() for item in environment if item.startswith(b"WINLUX_SHELL_APP=")), "")
                if mode not in ("explorer", "settings"): continue
                icon = f"/usr/share/winlux/native/icons/{mode}.svg"
            pix = ctypes.CDLL("libgdk_pixbuf-2.0.so.0")
            obj = ctypes.CDLL("libgobject-2.0.so.0")
            pix.gdk_pixbuf_new_from_file_at_scale.argtypes = [ctypes.c_char_p, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
            pix.gdk_pixbuf_new_from_file_at_scale.restype = ctypes.c_void_p
            pointer = pix.gdk_pixbuf_new_from_file_at_scale(os.fsencode(icon), 48, 48, 1, None)
            if not pointer: continue
            try:
                values = []
                for name in ["width", "height", "rowstride", "n_channels"]:
                    fn = getattr(pix, "gdk_pixbuf_get_" + name)
                    fn.argtypes, fn.restype = [ctypes.c_void_p], ctypes.c_int
                    values.append(fn(pointer))
                width, height, stride, channels = values
                pix.gdk_pixbuf_get_pixels.argtypes, pix.gdk_pixbuf_get_pixels.restype = [ctypes.c_void_p], ctypes.c_void_p
                pixels = ctypes.string_at(pix.gdk_pixbuf_get_pixels(pointer), stride * (height - 1) + width * channels)
                argb = [width, height]
                for y in range(height):
                    for x in range(width):
                        offset = y * stride + x * channels
                        r, g, b = pixels[offset:offset + 3]
                        alpha = pixels[offset + 3] if channels == 4 else 255
                        argb.append((alpha << 24) | (r << 16) | (g << 8) | b)
                publish_icon(window, argb)
            finally:
                obj.g_object_unref.argtypes = [ctypes.c_void_p]
                obj.g_object_unref(pointer)
        except (OSError, ValueError, StopIteration, subprocess.SubprocessError) as error:
            current.pop(window, None)
            detail = getattr(error, "stderr", b"")
            if isinstance(detail, bytes): detail = detail.decode(errors="replace")
            print(f"Window icon {window}: {type(error).__name__} {detail[:200]}", file=sys.stderr)
            continue
    cache.parent.mkdir(parents=True, exist_ok=True)
    cache.write_text(json.dumps(current))

def publish_icon(window, values):
    # Xlib expects native unsigned longs for a 32-bit CARDINAL property on LP64.
    xlib = ctypes.CDLL("libX11.so.6")
    xlib.XOpenDisplay.argtypes, xlib.XOpenDisplay.restype = [ctypes.c_char_p], ctypes.c_void_p
    xlib.XInternAtom.argtypes, xlib.XInternAtom.restype = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int], ctypes.c_ulong
    xlib.XChangeProperty.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_void_p, ctypes.c_int]
    xlib.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
    xlib.XCloseDisplay.argtypes = [ctypes.c_void_p]
    xlib.XSetErrorHandler.argtypes, xlib.XSetErrorHandler.restype = [ctypes.c_void_p], ctypes.c_void_p
    display = xlib.XOpenDisplay(None)
    if not display: raise OSError("No X display for icon")
    errors = []
    callback_type = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.c_void_p)
    def ignore_closed_window(_display, _event):
        errors.append(True)
        return 0
    callback = callback_type(ignore_closed_window)
    previous = xlib.XSetErrorHandler(ctypes.cast(callback, ctypes.c_void_p))
    try:
        atom = xlib.XInternAtom(display, b"_NET_WM_ICON", 0)
        pixels = (ctypes.c_ulong * len(values))(*values)
        xlib.XChangeProperty(display, int(window, 16), atom, 6, 32, 0, ctypes.cast(pixels, ctypes.c_void_p), len(values))
        xlib.XSync(display, 0)
    finally:
        xlib.XCloseDisplay(display)
        xlib.XSetErrorHandler(previous)
    if errors: raise OSError("Window closed while setting icon")

def launcher(path, name, profile="desktop", extra=()):
    path = path.resolve()
    if not valid_exe(path): return None
    identity = hashlib.sha256((str(path) + "\0" + profile + "\0".join(extra)).encode()).hexdigest()[:24]
    filename = f"winlux-pe-{identity}.desktop"
    icon = icon_for(path, identity)
    command = " ".join(argument(value) for value in [EXE_LAUNCHER, profile, path, *extra])
    name = name.replace("\n", " ").replace("\r", " ").replace("\0", " ")[:200]
    managed_desktop(APPS / filename, f"[Desktop Entry]\nType=Application\nName={escaped(name)}\nExec={command}\nIcon={escaped(icon)}\nTerminal=false\nCategories=Utility;WinluxWindows;\nX-Winlux-Executable={escaped(path)}\nActions=Location;\n\n[Desktop Action Location]\nName=Ouvrir l’emplacement\nExec=/usr/bin/winlux-shell-app explorer {argument(path.parent)}\n")
    return filename

def records(home):
    path = home / ".local/share/winlux/launchers.json"
    try:
        if path.stat().st_size > 4 * 1024 * 1024: return []
        value = json.loads(path.read_text())
        return value if isinstance(value, list) else []
    except (OSError, ValueError): return []

def register(path):
    path = path.resolve()
    if not valid_exe(path): return
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / "launchers.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        entries = [item for item in records(HOME) if isinstance(item, dict) and item.get("path") != str(path)]
        entries.append({"path": str(path), "name": path.stem})
        save(STATE / "launchers.json", json.dumps(entries[-4096:], ensure_ascii=False))

def installed_registry(home):
    path = home / ".local/share/winlux/registry.tsv"
    try:
        if path.stat().st_size > 16 * 1024 * 1024: return []
        groups = {}
        for line in path.read_text().splitlines():
            parts = line.split("\t")
            if len(parts) != 5 or parts[0] != "V" or parts[3] not in ("1", "2"): continue
            try:
                key, name = (bytes.fromhex(value).decode("utf-8") for value in parts[1:3])
                raw = bytes.fromhex(parts[4])
                value = raw.decode("utf-16-le").rstrip("\0") if len(raw) % 2 == 0 and b"\0" in raw else raw.rstrip(b"\0").decode("cp1252")
            except (ValueError, UnicodeError): continue
            if "\\uninstall\\" not in key.lower(): continue
            groups.setdefault(key, {})[name.lower()] = value
        entries = []
        for values in groups.values():
            name = values.get("displayname")
            icon = values.get("displayicon", "").rsplit(",", 1)[0]
            if name and icon:
                target = windows_path(icon, home)
                if valid_exe(target): entries.append((target, name))
        return entries
    except (OSError, ValueError, UnicodeError): return []

def command_arguments(command):
    """CommandLineToArgvW-style quotes/backslashes, without invoking a shell."""
    args, current, quoted, index, started = [], [], False, 0, False
    while index < len(command):
        char = command[index]
        if char in " \t" and not quoted:
            if started: args.append("".join(current)); current, started = [], False
            index += 1
            continue
        if char == "\\":
            end = index
            while end < len(command) and command[end] == "\\": end += 1
            count = end - index
            if end < len(command) and command[end] == '"':
                current.extend("\\" * (count // 2))
                if count % 2: current.append('"')
                else: quoted = not quoted
                index = end + 1
            else: current.extend("\\" * count); index = end
        elif char == '"': quoted = not quoted; index += 1
        else: current.append(char); index += 1
        started = True
    if started: args.append("".join(current))
    return args

def shortcuts(home):
    roots = [home / "AppData/Roaming/Microsoft/Windows/Start Menu/Programs",
             home / "ProgramData/Microsoft/Windows/Start Menu/Programs", home / "Start Menu/Programs"]
    count = 0
    for root in roots:
        if not root.is_dir(): continue
        for path in root.rglob("*.lnk"):
            count += 1
            if count > 4096: return
            try:
                if path.stat().st_size > 1024 * 1024: continue
                data = path.read_bytes()
                if len(data) < 76 or struct.unpack_from("<I", data)[0] != 76: continue
                flags = struct.unpack_from("<I", data, 20)[0]
                cursor, target, extra = 76, None, []
                if flags & 1: cursor += 2 + struct.unpack_from("<H", data, cursor)[0]
                if flags & 2:
                    size, header = struct.unpack_from("<II", data, cursor)
                    offset = struct.unpack_from("<I", data, cursor + 28)[0] if header >= 36 else 0
                    if offset:
                        raw = data[cursor + offset:cursor + size]
                        end = next((i for i in range(0, len(raw) - 1, 2) if raw[i:i + 2] == b"\0\0"), len(raw))
                        target = raw[:end].decode("utf-16-le")
                    else:
                        offset = struct.unpack_from("<I", data, cursor + 16)[0]
                        if offset: target = data[cursor + offset:cursor + size].split(b"\0", 1)[0].decode("cp1252")
                    cursor += size
                for bit in [4, 8, 16, 32, 64]:
                    if not flags & bit: continue
                    length = struct.unpack_from("<H", data, cursor)[0]
                    cursor += 2
                    width = 2 if flags & 128 else 1
                    raw = data[cursor:cursor + length * width]
                    if len(raw) != length * width: raise ValueError("Truncated shortcut")
                    value = raw.decode("utf-16-le" if width == 2 else "cp1252")
                    cursor += length * width
                    if bit == 8 and not target: target = value
                    if bit == 32: extra = command_arguments(value)
                if target:
                    executable = windows_path(target, home)
                    if valid_exe(executable): yield executable, path.stem, extra
            except (OSError, ValueError, UnicodeError, struct.error): continue

def browsers():
    values = [("edge", "Microsoft Edge", "opt/microsoft/msedge/microsoft-edge", "opt/microsoft/msedge/product_logo_48.png"),
              ("chrome", "Google Chrome", "opt/google/chrome/google-chrome", "opt/google/chrome/product_logo_48.png"),
              ("firefox", "Mozilla Firefox", "usr/lib/firefox/firefox", "usr/lib/firefox/browser/chrome/icons/default/default48.png")]
    keep = set()
    for key, name, binary, icon in values:
        root = Path("/opt/winlux/browsers") / key / "root"
        if (root / binary).is_file():
            filename = f"winlux-browser-{key}.desktop"
            keep.add(filename)
            managed_desktop(APPS / filename, f"[Desktop Entry]\nType=Application\nName={name}\nExec=/usr/bin/winlux-browsers --launch {key} %U\nIcon={root / icon}\nTerminal=false\nCategories=Network;WebBrowser;\n")
    return keep

def refresh():
    APPS.mkdir(parents=True, exist_ok=True)
    keep = browsers()
    homes = [(HOME, "desktop")]
    if os.getuid() == 0 and Path("/home/winlux-web").is_dir(): homes.append((Path("/home/winlux-web"), "browser"))
    for home, profile in homes:
        entries = [(Path(item["path"]), item.get("name", Path(item["path"]).stem)) for item in records(home)
                   if isinstance(item, dict) and isinstance(item.get("path"), str) and isinstance(item.get("name", ""), str)]
        entries += installed_registry(home)
        for target, name in entries:
            filename = launcher(target, name, profile)
            if filename: keep.add(filename)
        for target, name, extra in shortcuts(home):
            filename = launcher(target, name, profile, extra)
            if filename: keep.add(filename)
    for pattern in ["winlux-pe-*.desktop", "winlux-browser-*.desktop"]:
        for path in APPS.glob(pattern):
            if path.name not in keep: path.unlink()

def main():
    if len(sys.argv) > 1 and sys.argv[1] == "window-icons":
        window_icons()
        return
    if len(sys.argv) > 2 and sys.argv[1] == "register": register(Path(sys.argv[2]))
    elif len(sys.argv) > 1 and sys.argv[1] != "refresh": raise SystemExit(2)
    refresh()

if __name__ == "__main__": main()
