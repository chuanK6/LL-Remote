"""Compare every retained file, including test Data and original diagnostics."""
from pathlib import Path
import hashlib
import json
import os
import stat
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(__file__).resolve().parents[3]
history = Path(__file__).resolve().parent
plan = json.loads((root / "docs/workspace-cleanup-plan.json").read_text(encoding="utf-8"))
excluded = {item["path"] for item in plan["recommended"]}
outputs = {"docs/workspace-cleanup-preserved-before.json", "docs/workspace-cleanup-preservation.json",
           "docs/workspace-cleanup-execution.json", "docs/workspace-inventory.json", "docs/workspace-cleanup-plan.json",
           "docs/工作区清理清单.md", "docs/cleanup-history/2026-11-23/retained-before.json",
           "docs/cleanup-history/2026-11-23/retained-verification.json"}

def snapshot():
    result = {}
    def visit(path):
        relative = path.relative_to(root).as_posix()
        if relative in excluded or relative in outputs:
            return
        info = path.stat(follow_symlinks=False)
        if info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            result[relative] = {"link": os.readlink(path)}
        elif stat.S_ISDIR(info.st_mode):
            for entry in os.scandir(path):
                visit(Path(entry.path))
        elif stat.S_ISREG(info.st_mode):
            parts = {part.casefold() for part in path.relative_to(root).parts}
            if parts.intersection({"data", "config", "credentials", "trusted", "cache", "logs"}) or path.suffix.casefold() in {".pem", ".key", ".ini", ".der", ".pfx", ".p12"}:
                result[relative] = {"bytes": info.st_size, "mtimeNs": info.st_mtime_ns, "metadataOnly": True}
            else:
                digest = hashlib.sha256()
                with path.open("rb") as stream:
                    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                        digest.update(chunk)
                result[relative] = {"bytes": info.st_size, "sha256": digest.hexdigest()}
    for entry in os.scandir(root):
        visit(Path(entry.path))
    return result

mode = sys.argv[1]
if mode == "before":
    before = snapshot()
    (history / "retained-before.json").write_text(json.dumps(before, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"allRetainedEntriesRecorded": len(before), "credentialContentsRead": False}))
elif mode == "after":
    before = json.loads((history / "retained-before.json").read_text(encoding="utf-8"))
    after = snapshot()
    missing = [p for p in before if p not in after]
    changed = [p for p in before if p in after and before[p] != after[p]]
    report = {"allRetainedEntriesVerified": len(before), "missing": missing, "changed": changed,
              "allRetainedFilesPreserved": not missing and not changed, "credentialContentsRead": False}
    (history / "retained-verification.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, ensure_ascii=False))
    sys.exit(1 if missing or changed else 0)
else:
    raise SystemExit("Use before or after")
