"""Derive the Zca oracle configuration from the pinned ACT4 Sail 0.10 example.

Mirrors sail_config.py, but enables exactly the existing DUT profile plus Zca:
I with Zicsr and Zifencei beyond the base, and Zca. Everything else that
sail_config.py would leave off stays off, so the oracle cannot accept an
instruction the simulator does not implement. Zcd and Zcmop stay disabled
(contract D2: no Zcmop meaning; no F/D, so no Zcd).
"""
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
# V uses an enum, not the boolean `supported` field used by other ISAs.
# Run 34617153819 rejected the inherited Full setting with F/D/Zicsr off.
config["extensions"]["V"]["support_level"] = "Disabled"
# The existing ruscv profile is I + Zicsr + Zifencei; U4 adds only Zca.
for name in ("Zicsr", "Zifencei", "Zca"):
    config["extensions"][name]["supported"] = True
config["base"]["writable_misa"] = False
config["memory"]["pmp"]["count"] = 0
config["memory"]["pmp"]["usable_count"] = 0
pathlib.Path(".a5/config/sail.json").write_text(json.dumps(config, indent=2) + "\n")
enabled = sorted(name for name, value in config["extensions"].items()
                 if (value.get("supported") if isinstance(value, dict) else False))
pathlib.Path(".a5/config/sail-enabled.json").write_text(
    json.dumps({"enabled_extensions": enabled}, indent=2) + "\n")
if enabled != ["Zca", "Zicsr", "Zifencei"]:
    raise SystemExit(f"unexpected enabled extensions: {enabled}")
