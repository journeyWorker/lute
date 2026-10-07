"""Derive a removed-surface invalid artifact from its freshly compiled source.

Usage: python3 conformance/invalid/derive.py <fixture-name> <artifact.json>

Each `invalid/removed-*` fixture's `artifact.json` is `lute compile
source.lute` output with exactly one 0.36 spelling put back, so `lute run`
must refuse it (dsl 0.37.0 §4 `E-IR-REMOVED-FIELD`, or an unknown command
kind). The mutation is applied in place; `conformance/regenerate.sh` runs it
right after the compile.
"""

import json
import sys


def derive(name, artifact):
    commands = artifact["commands"]
    if name == "removed-field-addr":
        first = commands[0]
        commands[0] = {("addr" if k == "position" else k): v for k, v in first.items()}
    elif name == "removed-field-timing":
        camera = next(c for c in commands if c["kind"] == "camera")
        camera.update(camera.pop("timing"))
    elif name == "removed-field-envelope":
        artifact = {
            ("capabilityVersion" if k == "capabilitySnapshot" else k): v
            for k, v in artifact.items()
        }
    elif name == "removed-field-injected":
        injected = next(c for c in commands if "provenance" in c)
        injected["provenance"]["injected"] = True
    elif name == "removed-kind-sprite":
        next(c for c in commands if c["kind"] == "actor")["kind"] = "sprite"
    else:
        sys.exit(f"derive.py: no derivation for {name!r}")
    return artifact


def main():
    name, path = sys.argv[1], sys.argv[2]
    with open(path, encoding="utf-8") as f:
        artifact = json.load(f)
    artifact = derive(name, artifact)
    with open(path, "w", encoding="utf-8") as f:
        f.write(json.dumps(artifact, indent=2, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
