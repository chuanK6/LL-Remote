"""Write the final UTF-8 report from the recorded cleanup and validation results."""
from pathlib import Path
import json
import sys

sys.stdout.reconfigure(encoding="utf-8")
root = Path(__file__).resolve().parents[3]
docs = root / "docs"
history = Path(__file__).resolve().parent
report = json.loads((history / "validation.json").read_text(encoding="utf-8"))
plan = json.loads((docs / "workspace-cleanup-plan.json").read_text(encoding="utf-8"))
gib = lambda value: f"{value / 1024**3:.2f} GiB"
major = ["src-tauri/target/debug/build", "src-tauri/target/debug/incremental",
         "src-tauri/target/release/build", ".work/moonlight-build"]
lines = [
    "# 工作区文件结构与清理清单", "",
    "状态：2026-11-23 已按当前 0.1.12 工作区重新盘点并执行清理；Qt Debug/Release SDK 全部保留。", "",
    f"本次清理前约 {gib(report['beforeBytes'])}，删除 {report['deletedTargets']} 个缓存或重复资源目标，合计约 {gib(report['deletedCandidateBytes'])}。归档证据并完成前端构建及验证后，工作区约 {gib(report['afterBytes'])}，净减少约 {gib(report['netBytesReduced'])}。大小按文件逻辑长度计算，不包含目录联接的目标。", "",
    "## 已删除", "",
    "| 路径或类型 | 大小 | 删除依据 |", "| --- | ---: | --- |",
]
for path in major:
    item = next(t for t in plan["recommended"] if t["path"] == path)
    lines.append(f"| `{path}` | {gib(item['bytes'])} | 可由现有源码、依赖与工具重新生成 |")
other = sum(t["bytes"] for t in plan["recommended"] if t["path"] not in major)
lines.append(f"| 隔离验证中的旧程序、重复内核/运行库/许可证副本、历史 EXE 备份、其他缓存与 16 个旧目录联接 | {gib(other)} | 当前资源另有完整保留；联接只删除本身，不遍历目标 |")
lines += [
    "", "完整路径见 [本次清理计划](workspace-cleanup-plan.json) 和 [执行记录](workspace-cleanup-execution.json)。", "",
    "## 开发与数据保留", "",
    "- 保留 Vue/TypeScript/Rust 源码、测试、配置、锁文件、构建及打包脚本、方案和设计原稿。",
    "- 保留完整 `.tools`、`node_modules`、`third-party`、`engines`、`runtimes` 和 `notices`。Qt 调试库属于开发工具，本次未删除。",
    "- 保留 `src-tauri/target/debug` 和 `src-tauri/target/release` 中当前 EXE、PDB、运行资源及 DLL，只清理其中 build/incremental 缓存。",
    "- 保留整个正式 `release`，包括 0.1.12 运行目录、ZIP 和用户 Data。根目录 config，以及测试目录内所有 Data、config、credentials、trusted、logs、cache 均原位保留。",
    "- 保留 `.work/wintun-diagnostic`、其他原始诊断脚本及证据和 `sunshine-upstream-original.exe`，故障排查文档的原始证据链接仍可访问。",
    "- `src-tauri/runtimes` 的旧固定 WebView2 本次保留；当前配置及发布包使用系统共享 WebView2，不依赖此目录。",
    "- `.work` 保留，构建日志和测试仍可在该目录生成；`dist` 已通过正式前端构建重新生成。", "",
    "删除前已复制并校验 805 份脚本与验证资料，归档至 [本次证据目录](cleanup-evidence/2026-11-23/)，原位置与 SHA-256 见 [归档清单](cleanup-history/2026-11-23/archived-evidence.json)。", "",
    "## 验证结果", "",
    "- 32,471 个保留项完整性比对通过，缺失 0、变化 0。普通文件校验 SHA-256；用户数据与凭据仅比对长度和修改时间，未读取凭据内容。见 [完整比对结果](cleanup-history/2026-11-23/retained-verification.json) 及 [开发资源比对](workspace-cleanup-preservation.json)。",
    "- `npm run build` 通过，包括 TypeScript 类型检查及 Vite 生产构建。",
    "- `python scripts/verify-bundle.py release/LLRemote-Portable-x64-0.1.12 --reference .` 通过：138 个原生二进制、1,597 个资源文件，缺失依赖 0。",
    "- `npm test`：31 通过、3 失败，另有 7 次未处理异常。两个测试文件的 bridge 替身缺少当前初始化使用的 listenOperationError；配对测试仍预期空设备名，而当前界面提交默认名称。相关源码及测试文件的 SHA-256 与清理前一致，本次未修改。测试集未全部通过，不将其记录为通过。",
    "- 本次未重新编译 Rust/Moonlight；下一次编译会重建缓存，首次耗时增加。开发所需源码、工具与依赖完整保留。", "",
    "详细结果见 [本次验证记录](cleanup-history/2026-11-23/validation.json)。当前盘点见 [workspace-inventory.json](workspace-inventory.json)，本次清理前盘点见 [workspace-inventory-before-cleanup.json](workspace-inventory-before-cleanup.json)。", "",
    "## 历史与后续清理", "",
    "上一轮清单、计划和验证记录已保存在 [历史目录](cleanup-history/2026-11-23/)。上一轮约 21.12 GiB 的删除不计入本次结果。", "",
    "后续清理必须重新盘点并检查计划，尤其要保护新产生的 Data 和开发资料，不能直接重放本次清单。原清理脚本仍默认只预览；本次执行器及完整日志保存在历史目录中。", "",
]
(docs / "工作区清理清单.md").write_text("\n".join(lines), encoding="utf-8")
print(json.dumps({"checklistUpdated": True, "deletedGiB": report["deletedCandidateBytes"] / 1024**3,
                  "netReducedGiB": report["netBytesReduced"] / 1024**3,
                  "remainingGiB": report["afterBytes"] / 1024**3}))
