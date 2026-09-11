"""Derive the oracle configuration from the pinned ACT4 Sail 0.10 example."""
import json
import pathlib
import re

source = pathlib.Path(".a5/upstream/config/spike/spike-rv64-max/sail.json")
config = json.loads(re.sub(r"//[^\n]*", "", source.read_text()))
config.pop("$schema", None)


def disable_optional(node):
    if isinstance(node, dict):
        for key, value in node.items():
            if key == "supported":
                node[key] = False
            else:
                disable_optional(value)


disable_optional(config["extensions"])
config["base"]["writable_misa"] = False
config["memory"]["pmp"]["count"] = 0
config["memory"]["pmp"]["usable_count"] = 0
pathlib.Path(".a5/config/sail.json").write_text(json.dumps(config, indent=2) + "\n")
