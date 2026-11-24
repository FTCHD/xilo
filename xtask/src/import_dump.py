"""Dumps SOLIXMQTTMAP and the product names from anker-solix-api as JSON."""

import importlib.util
import json
import re
import sys
import types

src, out = sys.argv[1], sys.argv[2]
pkg = types.ModuleType("anker_solix_api")
pkg.__path__ = [src]
sys.modules["anker_solix_api"] = pkg


def load(name):
    spec = importlib.util.spec_from_file_location(f"anker_solix_api.{name}", f"{src}/{name}.py")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def clean(value):
    if isinstance(value, dict):
        return {str(k): clean(v) for k, v in value.items()}
    if isinstance(value, (list, tuple, set)):
        return [clean(v) for v in value]
    if isinstance(value, bytes):
        return value.hex()
    if isinstance(value, (str, int, float, bool)) or value is None:
        return value
    return None


mqttmap = load("mqttmap")
names = {}
comment = None
for line in open(f"{src}/mqttmap.py", encoding="utf-8"):
    stripped = line.strip()
    if stripped.startswith("#"):
        comment = stripped.lstrip("# ").strip()
        continue
    match = re.match(r'^ {4}"([A-Z0-9]+)": \{', line)
    if match and comment:
        names[match.group(1)] = comment
    if stripped:
        comment = None

json.dump({"names": names, "map": clean(mqttmap.SOLIXMQTTMAP)}, open(out, "w"), indent=1)
