"""Verify the source, tools, dependencies and released app survive cache cleanup."""
from pathlib import Path
import os
import stat
import sys
import json
import hashlib

sys.stdout.reconfigure(encoding="utf-8")
root = Path(__file__).resolve().parents[1]
baseline = root / "docs/workspace-cleanup-preserved-before.json"
protected = [
    "src", "src-tauri/src", "src-tauri/icons", "src-tauri/capabilities", "src-tauri/gen",
    "src-tauri/runtimes", "engines", "runtimes", "third-party", "notices", ".tools",
    "node_modules", "release", "component-tools", "scripts", "tests",
    "src-tauri/Cargo.toml", "src-tauri/Cargo.lock", "src-tauri/build.rs",
    "src-tauri/tauri.conf.json", "src-tauri/tauri.release.conf.json",
    "src-tauri/target/release/LLRemote.exe", "LLRemote.EngineHost.exe", "config",
    "package.json", "package-lock.json", "README.md", ".gitignore", ".npmrc",
    "index.html", "tsconfig.json", "vite.config.ts", "playwright.config.ts",
]

def snapshot():
    files = {}
    def visit(path):
        info = path.stat(follow_symlinks=False)
        relative = path.relative_to(root).as_posix()
        if info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            files[relative] = {"link": os.readlink(path)}
        elif path.is_dir():
            for entry in os.scandir(path):
                visit(Path(entry.path))
        elif path.is_file():
            if relative == "config" or any(part.casefold() == "data" for part in path.relative_to(root).parts):
                # User data is checked by metadata without reading credential contents.
                files[relative] = {"bytes": info.st_size, "mtimeNs": info.st_mtime_ns, "metadataOnly": True}
            else:
                digest = hashlib.sha256()
                with path.open("rb") as stream:
                    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                        digest.update(chunk)
                files[relative] = {"bytes": info.st_size, "sha256": digest.hexdigest()}
    for relative in protected:
        path = root / relative
        if path.exists():
            visit(path)
    return files

if sys.argv[1:] == ["before"]:
    files = snapshot()
    baseline.write_text(json.dumps({"workspace": str(root), "files": files}, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"protectedEntriesRecorded": len(files), "credentialContentsRead": False}))
elif sys.argv[1:] == ["after"]:
    before = json.loads(baseline.read_text(encoding="utf-8"))["files"]
    after = snapshot()
    missing = [path for path in before if path not in after]
    changed = [path for path in before if path in after and before[path] != after[path]]
    report = {"protectedEntriesVerified": len(before), "missing": missing, "changed": changed,
              "sourceToolsDependenciesAndReleasedAppPreserved": not missing and not changed,
              "credentialContentsRead": False}
    (root / "docs/workspace-cleanup-preservation.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({key: value for key, value in report.items() if key not in ("missing", "changed")}, ensure_ascii=False))
    if missing or changed:
        print(json.dumps({"missing": missing, "changed": changed}, ensure_ascii=False))
        sys.exit(1)
else:
    raise SystemExit("Usage: python scripts/check-cleanup-preserved.py before|after")
