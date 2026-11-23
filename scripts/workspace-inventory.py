"""Read-only workspace size inventory. Never follows directory junctions."""
from pathlib import Path
from datetime import datetime, timezone, timedelta
import os
import stat
import json
import heapq
import sys

sys.stdout.reconfigure(encoding="utf-8")

root = Path(__file__).resolve().parents[1]
directories = {}
links = []
errors = []
largest = []
seen_files = set()

def walk(path):
    logical = unique = count = 0
    try:
        entries = list(os.scandir(path))
    except OSError as error:
        errors.append({"path": str(path.relative_to(root)), "error": error.strerror})
        return logical, unique, count
    for entry in entries:
        full = Path(entry.path)
        relative = full.relative_to(root).as_posix()
        try:
            info = entry.stat(follow_symlinks=False)
            if getattr(info, "st_file_attributes", 0) & stat.FILE_ATTRIBUTE_REPARSE_POINT:
                try:
                    target = os.readlink(full)
                except OSError:
                    target = "unresolved"
                links.append({"path": relative, "target": target})
                continue
            if entry.is_dir(follow_symlinks=False):
                child_bytes, child_unique, child_count = walk(full)
                logical += child_bytes
                unique += child_unique
                count += child_count
            elif entry.is_file(follow_symlinks=False):
                size = info.st_size
                logical += size
                count += 1
                identity = (info.st_dev, info.st_ino) if info.st_ino else relative
                if identity not in seen_files:
                    seen_files.add(identity)
                    unique += size
                item = (size, relative)
                if len(largest) < 40:
                    heapq.heappush(largest, item)
                elif size > largest[0][0]:
                    heapq.heapreplace(largest, item)
        except OSError as error:
            errors.append({"path": relative, "error": error.strerror})
    relative = path.relative_to(root).as_posix()
    directories[relative] = {"bytes": logical, "uniqueFileBytes": unique, "files": count}
    return logical, unique, count

total, unique, count = walk(root)
report = {
    "workspace": str(root),
    "recordedAt": datetime.now(timezone(timedelta(hours=8))).isoformat(timespec="seconds"),
    "metric": "Logical file lengths; directory junctions excluded; uniqueFileBytes counts hard links once. NTFS allocation may differ.",
    "bytes": total,
    "uniqueFileBytes": unique,
    "files": count,
    "directories": directories,
    "directoryLinksNotFollowed": links,
    "largestFiles": [{"path": name, "bytes": size} for size, name in sorted(largest, reverse=True)],
    "errors": errors,
}
output = root / "docs" / "workspace-inventory.json"
output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
top = [{"path": name, **data} for name, data in directories.items() if name != "." and "/" not in name]
detail = [{"path": name, **data} for name, data in directories.items() if name.count("/") == 1]
print(json.dumps({"bytes": total, "uniqueFileBytes": unique, "files": count,
                  "top": sorted(top, key=lambda row: row["bytes"], reverse=True),
                  "largestSubdirectories": sorted(detail, key=lambda row: row["bytes"], reverse=True)[:30],
                  "largestFiles": report["largestFiles"][:15],
                  "linksSkipped": len(links), "errors": errors}, ensure_ascii=False, indent=2))
