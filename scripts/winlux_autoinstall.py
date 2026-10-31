"""Fichier de réponse Winlux (installation silencieuse), en YAML ou JSON.

Seul un compte local est pris en charge : aucune clé de compte en ligne n'est
acceptée, une clé inconnue est une erreur plutôt qu'un réglage ignoré.
"""
import json
import os
import re

SEARCH_PATHS = [
    "/run/winlux/live/media/winlux-autoinstall.yaml",
    "/run/winlux/live/media/winlux-autoinstall.yml",
    "/run/winlux/live/media/winlux-autoinstall.json",
    "/etc/winlux/autoinstall.yaml",
    "/etc/winlux/autoinstall.json",
]

LANGUAGES = {"fr_FR": "fr", "en_US": "us", "de_DE": "de", "es_ES": "es", "it_IT": "it", "pt_BR": "br"}
KEYBOARDS = {"fr", "us", "gb", "de", "es", "it", "br", "ch", "be", "ca"}
TOP_KEYS = {"version", "unattended", "language", "keyboard", "timezone", "license_accepted",
            "partitioning", "account", "packages", "reboot"}
PARTITION_KEYS = {"disk", "mode"}
ACCOUNT_KEYS = {"hostname", "username", "fullname", "password", "password_hash", "autologin"}

# Pages de l'installateur, dans l'ordre (identique à STEP_KEYS de winlux-installer.py).
# Le type d'installation se choisit sur la page des disques.
STEP_KEYS = ["language", "keyboard", "license", "network", "disk", "import", "encryption",
             "benchmark", "profile", "account", "migration", "summary", "progress", "done"]
(STEP_LANGUAGE, STEP_KEYBOARD, STEP_LICENSE, STEP_NETWORK, STEP_DISK, STEP_IMPORT,
 STEP_ENCRYPTION, STEP_BENCHMARK, STEP_PROFILE, STEP_ACCOUNT, STEP_MIGRATION,
 STEP_SUMMARY) = range(12)


class AnswerFileError(ValueError):
    pass


def _scalar(text):
    text = text.strip()
    if len(text) >= 2 and text[0] == text[-1] and text[0] in "\"'":
        return text[1:-1]
    if text.startswith("[") and text.endswith("]"):
        inner = text[1:-1].strip()
        return [_scalar(item) for item in inner.split(",")] if inner else []
    lowered = text.lower()
    if lowered in ("true", "yes", "on"):
        return True
    if lowered in ("false", "no", "off"):
        return False
    if lowered in ("null", "~", ""):
        return None
    if re.fullmatch(r"-?\d+", text):
        return int(text)
    return text


def _strip_comment(line):
    quote = None
    for index, char in enumerate(line):
        if quote:
            if char == quote:
                quote = None
        elif char in "\"'":
            quote = char
        elif char == "#" and (index == 0 or line[index - 1].isspace()):
            return line[:index]
    return line


def parse_simple_yaml(text):
    """Sous-ensemble YAML : mappings imbriqués par indentation, listes « - x » et [a, b]."""
    lines = []
    for number, raw in enumerate(text.splitlines(), 1):
        if "\t" in raw[: len(raw) - len(raw.lstrip())]:
            raise AnswerFileError(f"ligne {number} : tabulation interdite dans l'indentation")
        line = _strip_comment(raw).rstrip()
        if line.strip() and line.strip() != "---":
            lines.append((number, len(line) - len(line.lstrip()), line.strip()))

    def block(index, indent):
        if index < len(lines) and lines[index][2].startswith("- "):
            items = []
            while index < len(lines) and lines[index][1] == indent and lines[index][2].startswith("- "):
                items.append(_scalar(lines[index][2][2:]))
                index += 1
            return items, index
        mapping = {}
        while index < len(lines) and lines[index][1] == indent:
            number, _, content = lines[index]
            if ":" not in content:
                raise AnswerFileError(f"ligne {number} : « clé: valeur » attendu")
            key, _, value = content.partition(":")
            key = key.strip()
            if key in mapping:
                raise AnswerFileError(f"ligne {number} : clé en double « {key} »")
            index += 1
            if value.strip():
                mapping[key] = _scalar(value)
            elif index < len(lines) and lines[index][1] > indent:
                mapping[key], index = block(index, lines[index][1])
            elif index < len(lines) and lines[index][1] == indent and lines[index][2].startswith("- "):
                mapping[key], index = block(index, indent)
            else:
                mapping[key] = None
        if index < len(lines) and lines[index][1] > indent:
            raise AnswerFileError(f"ligne {lines[index][0]} : indentation inattendue")
        return mapping, index

    if not lines:
        return {}
    result, index = block(0, lines[0][1])
    if index != len(lines):
        raise AnswerFileError(f"ligne {lines[index][0]} : indentation incohérente")
    return result


def load_text(text, name):
    if name.endswith(".json"):
        try:
            data = json.loads(text)
        except json.JSONDecodeError as error:
            raise AnswerFileError(f"JSON invalide : {error}") from None
    else:
        try:
            import yaml
            data = yaml.safe_load(text)
        except ImportError:
            data = parse_simple_yaml(text)
        except Exception as error:
            raise AnswerFileError(f"YAML invalide : {error}") from None
    if data is None:
        data = {}
    if not isinstance(data, dict):
        raise AnswerFileError("le fichier doit contenir un dictionnaire de réglages")
    return data


def _check_keys(section, allowed, where):
    unknown = sorted(set(section) - allowed)
    if unknown:
        raise AnswerFileError(f"{where} : clé(s) non prise(s) en charge : {', '.join(unknown)}")


def _bool(value, where):
    if not isinstance(value, bool):
        raise AnswerFileError(f"{where} : true ou false attendu")
    return value


def validate(data):
    """Retourne (réglages, étapes préconfigurées) ; lève AnswerFileError."""
    _check_keys(data, TOP_KEYS, "fichier de réponse")
    if data.get("version", 1) != 1:
        raise AnswerFileError("version : seule la version 1 est prise en charge")
    settings = {"unattended": False, "reboot": False, "packages": []}
    steps = set()
    if "unattended" in data:
        settings["unattended"] = _bool(data["unattended"], "unattended")
    if "reboot" in data:
        settings["reboot"] = _bool(data["reboot"], "reboot")
    if "language" in data:
        if data["language"] not in LANGUAGES:
            raise AnswerFileError(f"language : valeurs possibles {', '.join(LANGUAGES)}")
        settings["language"] = data["language"]
        settings["keyboard"] = LANGUAGES[data["language"]]
        steps.add(STEP_LANGUAGE)
    if "keyboard" in data:
        if data["keyboard"] not in KEYBOARDS:
            raise AnswerFileError(f"keyboard : valeurs possibles {', '.join(sorted(KEYBOARDS))}")
        settings["keyboard"] = data["keyboard"]
        steps.add(STEP_KEYBOARD)
    elif STEP_LANGUAGE in steps:
        steps.add(STEP_KEYBOARD)
    if "timezone" in data:
        timezone = data["timezone"]
        if not isinstance(timezone, str) or not re.fullmatch(r"[A-Za-z_]+(/[A-Za-z0-9_+-]+){0,2}", timezone):
            raise AnswerFileError("timezone : nom de zone attendu, par exemple Europe/Paris")
        settings["timezone"] = timezone
    if "license_accepted" in data:
        if _bool(data["license_accepted"], "license_accepted"):
            settings["license_accepted"] = True
            steps.add(STEP_LICENSE)
    partitioning = data.get("partitioning")
    if partitioning is not None:
        if not isinstance(partitioning, dict):
            raise AnswerFileError("partitioning : dictionnaire attendu")
        _check_keys(partitioning, PARTITION_KEYS, "partitioning")
        mode = partitioning.get("mode", "full")
        if mode != "full":
            raise AnswerFileError("partitioning.mode : seul « full » (disque entier effacé) est disponible")
        settings["install_type"] = "full"
        disk = partitioning.get("disk")
        if disk is not None:
            if not isinstance(disk, str) or not (disk == "auto" or re.fullmatch(r"/dev/[A-Za-z0-9/_-]+", disk)):
                raise AnswerFileError("partitioning.disk : « auto » ou chemin /dev/... attendu")
            settings["disk_request"] = disk
            steps.add(STEP_DISK)
    account = data.get("account")
    if account is not None:
        if not isinstance(account, dict):
            raise AnswerFileError("account : dictionnaire attendu")
        _check_keys(account, ACCOUNT_KEYS, "account (compte local uniquement)")
        username = account.get("username")
        if username is not None:
            if not isinstance(username, str) or len(username) < 2 or not re.fullmatch(r"[a-z_][a-z0-9_-]{0,31}", username) or username == "root":
                raise AnswerFileError("account.username : minuscules, chiffres, - ou _, 2 à 32 caractères, pas root")
            settings["username"] = username
        for key in ("hostname", "fullname"):
            if key in account:
                if not isinstance(account[key], str) or ":" in account[key] or "\n" in account[key]:
                    raise AnswerFileError(f"account.{key} : texte sans « : » attendu")
                settings[key] = account[key]
        if "hostname" in settings and not re.fullmatch(r"[A-Za-z0-9]([A-Za-z0-9-]{0,61}[A-Za-z0-9])?", settings["hostname"]):
            raise AnswerFileError("account.hostname : lettres, chiffres et -, 63 caractères maximum")
        if "password" in account and "password_hash" in account:
            raise AnswerFileError("account : password ou password_hash, pas les deux")
        if "password" in account:
            if not isinstance(account["password"], str) or not account["password"]:
                raise AnswerFileError("account.password : texte non vide attendu")
            settings["password"] = account["password"]
        if "password_hash" in account:
            hashed = account["password_hash"]
            if not isinstance(hashed, str) or not re.fullmatch(r"\$(6|y|5)\$[./A-Za-z0-9$=]+", hashed):
                raise AnswerFileError("account.password_hash : empreinte crypt attendue ($6$..., openssl passwd -6)")
            settings["password_hash"] = hashed
        if "autologin" in account:
            settings["autologin"] = _bool(account["autologin"], "account.autologin")
        if "username" in settings and ("password" in settings or "password_hash" in settings):
            steps.add(STEP_ACCOUNT)
    packages = data.get("packages")
    if packages is not None:
        if not isinstance(packages, list):
            raise AnswerFileError("packages : liste attendue")
        for package in packages:
            if not isinstance(package, str) or not re.fullmatch(r"[a-z0-9][a-z0-9+.-]+", package):
                raise AnswerFileError(f"packages : nom de paquet invalide : {package!r}")
        settings["packages"] = list(dict.fromkeys(packages))
    if settings["unattended"]:
        missing = {STEP_LANGUAGE: "language", STEP_LICENSE: "license_accepted: true",
                   STEP_DISK: "partitioning.disk", STEP_ACCOUNT: "account.username + password"}
        absent = [label for step, label in missing.items() if step not in steps]
        if absent:
            raise AnswerFileError("unattended : réglage(s) manquant(s) : " + ", ".join(absent))
        # Sans clic : pas de réseau à configurer, pas d'import Windows, pas de chiffrement,
        # pas de benchmark (profil par défaut) ni de transfert depuis un autre PC.
        steps.update({STEP_NETWORK, STEP_IMPORT, STEP_ENCRYPTION, STEP_BENCHMARK, STEP_PROFILE,
                      STEP_MIGRATION, STEP_SUMMARY})
    return settings, steps


def resolve_disk(request, disks):
    """Associe partitioning.disk à un disque détecté, ou None."""
    if request == "auto":
        return disks[0] if len(disks) == 1 else None
    real = os.path.realpath(request)
    for disk in disks:
        if disk["path"] in (request, real):
            return disk
    return None


def find_answer_file(cmdline_path="/proc/cmdline", environ=os.environ):
    explicit = environ.get("WINLUX_AUTOINSTALL")
    if not explicit:
        try:
            with open(cmdline_path) as handle:
                for argument in handle.read().split():
                    if argument.startswith("winlux.autoinstall="):
                        explicit = argument.split("=", 1)[1]
        except OSError:
            pass
    if explicit:
        if not os.path.isfile(explicit):
            raise AnswerFileError(f"fichier de réponse introuvable : {explicit}")
        return explicit
    for path in SEARCH_PATHS:
        if os.path.isfile(path):
            return path
    return None


def load(path):
    with open(path, encoding="utf-8") as handle:
        return validate(load_text(handle.read(), path))
