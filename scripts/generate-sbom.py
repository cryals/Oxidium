#!/usr/bin/env python3
"""Generate a CycloneDX 1.5 JSON SBOM directly from `cargo metadata --locked`."""

import datetime
import json
import subprocess
import sys
import uuid
from pathlib import Path


def main() -> int:
    out_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("dist/oxidium-sbom.cdx.json")
    out_path.parent.mkdir(parents=True, exist_ok=True)

    raw = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        text=True,
        encoding="utf-8",
    )
    meta = json.loads(raw)

    components = []
    seen = set()
    app_version = "0.1.3"
    for pkg in meta.get("packages", []):
        name = pkg["name"]
        version = pkg["version"]
        if name == "student-journal":
            app_version = version
        key = (name, version)
        if key in seen:
            continue
        seen.add(key)

        source = pkg.get("source") or ""
        is_workspace = source == ""
        purl = f"pkg:cargo/{name}@{version}"
        comp = {
            "type": "application" if name in ("student-journal", "student-journal-mcp") else "library",
            "bom-ref": purl,
            "name": name,
            "version": version,
            "purl": purl,
            "scope": "required",
        }
        if pkg.get("license"):
            comp["licenses"] = [{"expression": pkg["license"]}]
        if pkg.get("description"):
            comp["description"] = pkg["description"].strip()
        if pkg.get("repository"):
            comp["externalReferences"] = [{"type": "vcs", "url": pkg["repository"]}]
        if is_workspace:
            comp["group"] = "io.oxidium"
        components.append(comp)

    components.sort(key=lambda c: (c["name"], c["version"]))

    sbom = {
        "bomFormat": "CycloneDX",
        "specVersion": "1.5",
        "serialNumber": f"urn:uuid:{uuid.uuid4()}",
        "version": 1,
        "metadata": {
            "timestamp": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "component": {
                "type": "application",
                "name": "Oxidium",
                "version": app_version,
                "description": "Personal local-first Student Journal with native Slint UI, SQLite, and MCP server",
                "licenses": [{"license": {"id": "MIT"}}],
                "externalReferences": [
                    {"type": "vcs", "url": "https://github.com/cryals/Oxidium"}
                ],
            },
        },
        "components": components,
    }

    out_path.write_text(json.dumps(sbom, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Generated CycloneDX 1.5 SBOM with {len(components)} components -> {out_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
