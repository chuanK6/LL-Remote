"""Prepare a cleanup manifest and report; never deletes or renames anything."""
from pathlib import Path
import json
import stat

root = Path(__file__).resolve().parents[1]
inventory = json.loads((root / "docs/workspace-inventory.json").read_text(encoding="utf-8"))
directories = inventory["directories"]
recommended = []
qt_debug = []

def size(path):
    relative = path.relative_to(root).as_posix()
    if relative in directories:
        return directories[relative]["bytes"]
    return path.stat(follow_symlinks=False).st_size if path.exists() else 0

def mark(path, reason):
    if path.exists():
        recommended.append({"path": path.relative_to(root).as_posix(), "bytes": size(path), "reason": reason})

target = root / "src-tauri/target"
if target.exists():
    for child in target.iterdir():
        if child.name == "release":
            for artifact in child.iterdir():
                if artifact.name.casefold() not in ("llremote.exe", "data"):
                    mark(artifact, "Rust 构建缓存、旧安装包或重复运行资源；最新打包输入 LLRemote.exe 保留")
        elif child.name.casefold() != "data":
            mark(child, "Rust 调试构建、增量缓存或可再生成的工具链元数据")
mark(root / ".work", "下载缓存、解压副本、临时构建和历史验证目录；执行前保留已有根目录验证记录")
mark(root / "dist", "前端编译输出，可由 npm run build 重新生成")
mark(root / "test-results", "自动化测试输出")
mark(root / "aqtinstall.log", "Qt 安装工具日志")

qt = root / ".tools/Qt/6.12.0/msvc2022_64"
if qt.exists():
    for path in qt.rglob("*"):
        if not path.is_file() or path.stat(follow_symlinks=False).st_file_attributes & stat.FILE_ATTRIBUTE_REPARSE_POINT:
            continue
        if path.suffix.lower() in (".dll", ".lib", ".pdb", ".exe") and path.stem.endswith("d"):
            counterpart = path.with_name(path.stem[:-1] + path.suffix)
            if counterpart.is_file():
                qt_debug.append({"path": path.relative_to(root).as_posix(), "bytes": size(path), "reason": "Qt 调试组件；同目录 Release 组件存在，当前 Moonlight 只构建 Release"})

preserved = [
    {"path": "src", "reason": "Vue 界面与 TypeScript 源码"},
    {"path": "src-tauri/src", "reason": "Rust 业务与三个内核适配源码"},
    {"path": "src-tauri/Cargo.toml", "reason": "Rust 构建配置"},
    {"path": "src-tauri/Cargo.lock", "reason": "固定 Rust 依赖版本"},
    {"path": "src-tauri/target/release/LLRemote.exe", "reason": "最新已编译程序，打包脚本直接引用"},
    {"path": "release", "reason": "当前完整运行目录，以及用户的 Data 和登录配置"},
    {"path": "engines", "reason": "三个内核及其 DLL、Qt、Web 资源，下一次打包必需"},
    {"path": "runtimes", "reason": "MSVC 运行依赖"},
    {"path": "notices", "reason": "第三方许可证和对应源码归档，随包保留"},
    {"path": "third-party", "reason": "Moonlight 源码与 LL 扩展，继续开发时需要"},
    {"path": ".tools", "reason": "Qt Release SDK 和构建工具；只将确认过的调试组件列为可选清理"},
    {"path": "node_modules", "reason": "前端依赖，172.55 MB；体积较小，继续开发保留"},
    {"path": "component-tools", "reason": "x86 宿主和系统组件源码"},
    {"path": "LLRemote.EngineHost.exe", "reason": "打包脚本必需的 x86 宿主"},
    {"path": "scripts", "reason": "构建、打包、验证与本次清理工具"},
    {"path": "docs", "reason": "方案、验证证据和清理清单"},
    {"path": "tests", "reason": "回归测试源码"},
    {"path": "config", "reason": "可能是本机连接配置，未读取内容；不作为无关文件删除"},
    {"path": "package.json", "reason": "前端构建配置"},
    {"path": "package-lock.json", "reason": "固定前端依赖版本"},
]
removable = sum(entry["bytes"] for entry in recommended)
optional = sum(entry["bytes"] for entry in qt_debug)
plan = {
    "schemaVersion": 1,
    "status": "prepared_not_executed",
    "workspace": str(root),
    "inventoryRecordedAt": inventory["recordedAt"],
    "recommendedBytes": removable,
    "optionalQtDebugBytes": optional,
    "estimatedRemainingBytes": inventory["bytes"] - removable,
    "recommended": sorted(recommended, key=lambda item: item["bytes"], reverse=True),
    "optionalQtDebug": sorted(qt_debug, key=lambda item: item["bytes"], reverse=True),
    "preserved": preserved,
    "linkPolicy": "Remove junctions themselves without traversing their targets; 13 links currently point into the release directory.",
}
(root / "docs/workspace-cleanup-plan.json").write_text(json.dumps(plan, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

def gib(value):
    return f"{value / 1024**3:.2f} GiB"

def mib(value):
    return f"{value / 1024**2:.2f} MiB"

own_source = directories["src"]["bytes"] + directories["src-tauri/src"]["bytes"]
rows = [
    ("src-tauri/target", "Rust 调试库、增量缓存、发布构建及旧安装包", "准备清理，保留最新 LLRemote.exe"),
    (".work", "多份旧运行目录、WebView2 解压副本、下载和验证缓存", "准备清理，先保存根目录验证记录"),
    (".tools", "完整 Qt SDK、调试库和安装工具", "保留 Release 工具；调试组件可选清理"),
    ("release", "当前可运行软件及用户数据", "保留"),
    ("engines", "Sunshine、Moonlight、皎月连运行资源", "保留"),
    ("third-party", "Moonlight 上游源码和扩展", "保留"),
    ("node_modules", "前端依赖", "保留"),
    ("notices", "许可证及对应源码归档", "保留"),
]
lines = [
    "# 工作区文件结构与清理清单", "",
    "状态：已统计并标记，尚未删除或重命名。", "",
    f"工作区文件共 {gib(inventory['bytes'])}（{inventory['bytes'] / 10**9:.2f} GB），{inventory['files']:,} 个文件。统计跳过 {len(inventory['directoryLinksNotFollowed'])} 个目录联接，避免把其指向的运行资源重复计入。", "",
    f"自写界面与 Rust 源码合计 {mib(own_source)}。约 20 多 GB 的主体是构建和验证产物，不是业务源码。当前完整运行目录约 {gib(directories['release']['bytes'])}。", "",
    "| 路径 | 大小 | 用途 | 标记 |", "| --- | ---: | --- | --- |",
]
for path, purpose, label in rows:
    lines.append(f"| `{path}` | {gib(directories[path]['bytes'])} | {purpose} | {label} |")
lines += [
    "", "Rust 调试构建占 12.34 GiB，其中 build 目录 8.27 GiB、增量缓存 2.18 GiB。Cargo.toml 同时配置 staticlib、cdylib、rlib；调试 staticlib 把依赖打进静态库，单份 ll_remote.lib 约 926 MiB，多个历史构建目录重复保留。Windows 桌面应用后续可评估仅保留需要的 crate 类型，本轮未修改构建配置。", "",
    "发布构建还保留了 0.1.0 / 0.1.1 两份旧安装包，共约 731 MiB，以及两份 678 MiB 的 WebView2 副本；.work 中另外保留旧运行目录和一份 WebView2 解压副本。", "",
    f"第一批清理预计释放 {gib(removable)}（{removable / 10**9:.2f} GB），剩余约 {gib(inventory['bytes'] - removable)}。下次 Rust 构建将重新生成缓存，耗时会增加。最新打包输入 src-tauri/target/release/LLRemote.exe 和正式 release 目录保留。", "",
    f"Qt 调试组件另有 {len(qt_debug)} 个文件，共 {gib(optional)}，标为可选；清理后仍保留 Qt Release SDK，无法直接进行 Qt Debug 构建。", "",
    "| 第一批候选 | 大小 | 原因 |", "| --- | ---: | --- |",
]
for entry in plan["recommended"]:
    if entry["bytes"] >= 1024**2:
        lines.append(f"| `{entry['path']}` | {gib(entry['bytes']) if entry['bytes'] >= 1024**3 else mib(entry['bytes'])} | {entry['reason']} |")
lines += [
    "", "所有具体路径及 Qt 调试文件清单见 [workspace-cleanup-plan.json](workspace-cleanup-plan.json)，完整大小盘点见 [workspace-inventory.json](workspace-inventory.json)。", "",
    "正式 release/Data 中已存在用户数据，整棵 release 目录保留。根目录 config 也保留，未读取其内容。engines 和 notices 是打包依赖；WebView2 已切换到系统共享模式，不再依赖旧固定运行时。", "",
    ".work 中的目录联接指向正式 release 的内核和 WebView2；清理脚本仅删除联接本身，禁止遍历目标。", "",
    "预览：powershell -NoProfile -ExecutionPolicy Bypass -File scripts/clean-workspace.ps1。脚本默认只显示计划，只有显式加 -Execute 才删除；-IncludeQtDebug 用于额外清理已标记的 Qt 调试组件。本轮未执行删除。", "",
]
(root / "docs/工作区清理清单.md").write_text("\n".join(lines), encoding="utf-8")
print(json.dumps({"status": plan["status"], "recommendedBytes": removable, "optionalQtDebugBytes": optional,
                  "estimatedRemainingBytes": plan["estimatedRemainingBytes"], "ownSourceBytes": own_source,
                  "recommendedTargets": len(recommended), "optionalQtDebugFiles": len(qt_debug)}, indent=2))
