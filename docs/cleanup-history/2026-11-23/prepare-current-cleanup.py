"""Prepare the reviewed 2026-11-23 cleanup; archive evidence, never delete files."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import stat
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(__file__).resolve().parents[3]
history = Path(__file__).resolve().parent
inventory = json.loads((root / "docs/workspace-inventory.json").read_text(encoding="utf-8"))
targets = []
protected_names = {"data", "config", "credentials", "trusted", "logs", "cache"}

def entries(path):
    for entry in os.scandir(path):
        yield Path(entry.path), entry.stat(follow_symlinks=False)

def linked(info):
    return bool(info.st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT)

def mark(path, reason):
    info = path.stat(follow_symlinks=False)
    relative = path.relative_to(root).as_posix()
    size = 0 if linked(info) else inventory["directories"].get(relative, {}).get("bytes", info.st_size)
    targets.append({"path": relative, "bytes": size, "reason": reason})

def contains_data(path):
    for child, info in entries(path):
        if child.name.casefold() in protected_names:
            return True
        if not linked(info) and stat.S_ISDIR(info.st_mode) and contains_data(child):
            return True
    return False

def mark_resource(path):
    info = path.stat(follow_symlinks=False)
    if path.name.casefold() in protected_names:
        return
    if linked(info) or not stat.S_ISDIR(info.st_mode) or not contains_data(path):
        mark(path, "隔离验证目录中的重复运行资源；Data、配置、凭据和日志原位保留")
    else:
        for child, _ in entries(path):
            mark_resource(child)

for relative in ("src-tauri/target/debug/build", "src-tauri/target/debug/incremental",
                 "src-tauri/target/release/build", "src-tauri/target/release/incremental",
                 ".work/moonlight-build", ".work/engine-host-build", ".work/npm-cache"):
    path = root / relative
    if path.exists():
        mark(path, "可重新生成的编译或下载缓存；当前 Debug/Release 程序及符号保留")

work = root / ".work"
resource_names = {"engines", "notices", "component-tools", "runtimes", "assets", "scripts"}

def find_fixture(path):
    children = list(entries(path))
    files = {p.name.casefold() for p, info in children if not stat.S_ISDIR(info.st_mode)}
    is_fixture = "llremote.exe" in files or ("sunshine.exe" in files and path.parent == work)
    if is_fixture:
        for child, info in children:
            if child.name.casefold() in resource_names:
                mark_resource(child)
            elif not stat.S_ISDIR(info.st_mode) and child.suffix.casefold() in {".exe", ".dll", ".pdb"}:
                mark(child, "隔离验证目录中的旧程序或重复 DLL；正式版本保留")
        return
    if path == work or path.relative_to(work).as_posix() in {"connection-repair", "sunshine-reachability"}:
        for child, info in children:
            if not linked(info) and stat.S_ISDIR(info.st_mode) and child.name not in {"moonlight-build", "engine-host-build", "npm-cache"}:
                find_fixture(child)

find_fixture(work)
for child, info in entries(work):
    if not stat.S_ISDIR(info.st_mode) and child.name.startswith("LLRemote-before-") and child.suffix.casefold() == ".exe":
        mark(child, "历史 LLRemote 程序备份；当前 Debug/Release 和正式交付包保留")

targets.sort(key=lambda item: item["bytes"], reverse=True)
plan = {"schemaVersion": 1, "status": "prepared_not_executed", "workspace": str(root),
        "inventoryRecordedAt": inventory["recordedAt"], "recommendedBytes": sum(t["bytes"] for t in targets),
        "recommended": targets, "optionalQtDebug": [], "optionalQtDebugBytes": 0,
        "linkPolicy": "Only remove links themselves. Never follow a reparse point.",
        "preserved": ["All source, SDKs, dependencies, release, current debug/release executables and symbols",
                      "All Data, config, credentials, trusted, logs and cache folders inside test fixtures",
                      ".work/wintun-diagnostic and other original diagnostic scripts/evidence",
                      ".work/sunshine-upstream-original.exe", "src-tauri/runtimes"]}
(root / "docs/workspace-cleanup-plan.json").write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
shutil.copy2(root / "docs/workspace-inventory.json", root / "docs/workspace-inventory-before-cleanup.json")

archive = root / "docs/cleanup-evidence/2026-11-23"
source_extensions = {".ps1", ".py", ".cpp", ".h", ".mjs", ".js", ".ts", ".vue", ".rs", ".cs",
                     ".json", ".txt", ".md", ".log", ".xml", ".manifest", ".reg", ".html", ".css"}
archived = []

def archive_file(path):
    relative = path.relative_to(root)
    destination = archive / relative
    if destination.exists():
        raise RuntimeError(f"Archive already exists: {destination}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(path, destination)
    source_hash = hashlib.sha256(path.read_bytes()).hexdigest()
    if hashlib.sha256(destination.read_bytes()).hexdigest() != source_hash:
        raise RuntimeError(f"Archive verification failed: {path}")
    archived.append({"source": relative.as_posix(), "archive": destination.relative_to(root).as_posix(), "sha256": source_hash})

def archive_tree(path):
    info = path.stat(follow_symlinks=False)
    if linked(info):
        return
    if stat.S_ISDIR(info.st_mode):
        for child, _ in entries(path):
            archive_tree(child)
    elif path.suffix.casefold() in source_extensions:
        archive_file(path)

for item in targets:
    if item["path"].startswith(".work/") and item["path"] not in {".work/moonlight-build", ".work/engine-host-build", ".work/npm-cache"}:
        archive_tree(root / item["path"])
for child, info in entries(work):
    if not stat.S_ISDIR(info.st_mode) and child.suffix.casefold() in source_extensions:
        archive_file(child)
(history / "archived-evidence.json").write_text(json.dumps(archived, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
print(json.dumps({"targets": len(targets), "candidateGiB": plan["recommendedBytes"] / 1024**3,
                  "archivedFilesVerified": len(archived), "preserveQtDebug": True, "preserveAllDataInPlace": True}))
