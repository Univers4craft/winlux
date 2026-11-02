#!/usr/bin/env python3
"""Génère les tables de signatures d'OpenGL et de Vulkan utilisées par les
ponts Windows x64 → System V (runtime/src/dlls/hostcall.rs).

Les fichiers gl.xml et vk.xml viennent du registre Khronos (par exemple le
paquet PyPI glad2, dossier glad/files). Utilisation :

    python3 scripts/gen-host-signatures.py gl.xml vk.xml runtime/src/dlls/sigs

Chaque ligne de sortie : « nom codes », un code par argument :
  p pointeur ou entier 64 bits   i entier 32 bits
  b entier 8 bits non signé       c entier 8 bits signé
  h entier 16 bits non signé      s entier 16 bits signé
  f flottant 32 bits              d flottant 64 bits
  A VkAllocationCallbacks* (remplacé par NULL)
« - » quand la fonction n'a aucun argument. Une ligne « * nom » marque les
fonctions d'OpenGL 1.0/1.1, exportées directement par opengl32.dll.
"""
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

GL_TYPES = {
    "GLfloat": "f", "GLclampf": "f", "GLdouble": "d", "GLclampd": "d",
    "GLboolean": "b", "GLubyte": "b", "GLbyte": "c", "GLchar": "c",
    "GLcharARB": "c", "GLshort": "s", "GLushort": "h", "GLhalf": "h",
    "GLhalfNV": "h", "GLhalfARB": "h",
    "GLint64": "p", "GLuint64": "p", "GLint64EXT": "p", "GLuint64EXT": "p",
    "GLsizeiptr": "p", "GLintptr": "p", "GLsizeiptrARB": "p", "GLintptrARB": "p",
    "GLsync": "p", "GLeglImageOES": "p", "GLeglClientBufferEXT": "p",
    "GLvdpauSurfaceNV": "p", "GLDEBUGPROC": "p", "GLDEBUGPROCARB": "p",
    "GLDEBUGPROCKHR": "p", "GLDEBUGPROCAMD": "p", "GLVULKANPROCNV": "p",
    "GLhandleARB": "i", "GLfixed": "i", "GLclampx": "i",
}


def gl_code(param):
    text = "".join(param.itertext())
    ptype = param.findtext("ptype")
    if "*" in text or "[" in text:
        return "p"
    if ptype is None:
        raise SystemExit(f"type GL inconnu : {text!r}")
    return GL_TYPES.get(ptype, "i")


def gen_gl(path):
    root = ET.parse(path).getroot()
    sigs = {}
    for cmd in root.find("commands"):
        name = cmd.find("proto").findtext("name")
        sigs[name] = "".join(gl_code(p) for p in cmd.findall("param")) or "-"
    core = set()
    for feat in root.iter("feature"):
        if feat.get("api") == "gl" and feat.get("number") in ("1.0", "1.1"):
            for c in feat.iter("command"):
                core.add(c.get("name"))
    lines = []
    for name in sorted(sigs):
        lines.append(("* " if name in core else "") + f"{name} {sigs[name]}")
    return lines


def gen_vk(path):
    root = ET.parse(path).getroot()
    kinds = {}
    for t in root.find("types"):
        cat = t.get("category")
        name = t.get("name") or t.findtext("name")
        if not name:
            continue
        if t.get("alias"):
            kinds[name] = ("alias", t.get("alias"))
            continue
        if cat == "handle":
            kinds[name] = ("scalar", "p")
        elif cat in ("enum", "bitmask"):
            under = t.findtext("type") or "VkFlags"
            kinds[name] = ("scalar", "p" if under in ("VkFlags64",) else "i")
        elif cat == "basetype":
            under = t.findtext("type")
            kinds[name] = ("scalar", "p" if under in ("uint64_t", "VkFlags64") or name.endswith("64") else "i")
        elif cat in ("struct", "union"):
            kinds[name] = ("struct", None)
        elif cat == "funcpointer":
            kinds[name] = ("scalar", "p")
    base = {
        "uint32_t": "i", "int32_t": "i", "uint64_t": "p", "int64_t": "p",
        "size_t": "p", "float": "f", "double": "d", "uint8_t": "b",
        "int": "i", "VkBool32": "i", "VkFlags": "i", "VkFlags64": "p",
        "VkDeviceSize": "p", "VkDeviceAddress": "p", "VkSampleMask": "i",
    }

    def resolve(t):
        seen = 0
        while t in kinds and kinds[t][0] == "alias" and seen < 10:
            t = kinds[t][1]
            seen += 1
        return t

    def code(param):
        text = "".join(param.itertext())
        t = param.findtext("type")
        if "*" in text or "[" in text:
            return "A" if t == "VkAllocationCallbacks" else "p"
        if t in base:
            return base[t]
        r = resolve(t)
        if r in base:
            return base[r]
        k = kinds.get(r)
        if k is None:
            return "p"  # types de plateforme (Display*, HWND…) : pointeurs
        if k[0] == "struct":
            raise SystemExit(f"structure passée par valeur : {t}")
        return k[1]

    sigs = {}
    aliases = {}
    for cmd in root.find("commands"):
        api = cmd.get("api")
        if api and "vulkan" not in api.split(","):
            continue
        if cmd.get("alias"):
            aliases[cmd.get("name")] = cmd.get("alias")
            continue
        name = cmd.find("proto").findtext("name")
        params = [p for p in cmd.findall("param")
                  if not p.get("api") or "vulkan" in p.get("api").split(",")]
        sigs[name] = "".join(code(p) for p in params) or "-"
    for a, target in aliases.items():
        while target in aliases:
            target = aliases[target]
        if target in sigs:
            sigs[a] = sigs[target]
    core = set()
    for feat in root.iter("feature"):
        if "vulkan" in (feat.get("api") or "").split(","):
            for c in feat.iter("command"):
                core.add(c.get("name"))
    return [("* " if n in core else "") + f"{n} {sigs[n]}" for n in sorted(sigs)]


def main():
    gl, vk, out = sys.argv[1:4]
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    head = "# Généré par scripts/gen-host-signatures.py, ne pas modifier à la main.\n"
    (out / "gl.txt").write_text(head + "\n".join(gen_gl(gl)) + "\n")
    (out / "vk.txt").write_text(head + "\n".join(gen_vk(vk)) + "\n")


if __name__ == "__main__":
    main()
