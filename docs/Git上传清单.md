# LL远程 Git 上传清单

整理日期：2026-11-23。目标是上传继续开发所需的源码、配置、文档和脚本；运行包、依赖、SDK、缓存和个人数据留在本地。当前规则已写入根目录 [.gitignore](../.gitignore)。忽略文件不等于删除文件，已有本机开发资源继续保留。

按当前规则实际盘点，首次提交候选为 **184 个文件，约 2.63 MiB**，没有超过 5 MiB 的单个文件。逐文件路径见 [Git 上传文件列表](Git上传文件列表.txt)。该列表是本次快照，新增文件后以 Git 实际候选为准。

## 1. 应上传到 Git 的内容

| 路径 | 内容与理由 |
| --- | --- |
| `src/` | Vue/TypeScript 业务、页面、样式、图标和壁纸资源 |
| `src-tauri/src/` | Rust 业务、内核适配、数据、权限、会话和测试源码 |
| `src-tauri/Cargo.toml`、`Cargo.lock`、`build.rs` | Rust 构建与锁定依赖；应用项目需要保留 Cargo.lock |
| `src-tauri/tauri*.json`、`windows-app.manifest` | 桌面配置、权限与发布配置 |
| `src-tauri/capabilities/`、`icons/` | 窗口权限和应用图标；PNG、ICO、SVG 属于项目资源，可以上传 |
| `tests/`、`playwright.config.ts` | 前端回归、页面测试和测试配置 |
| `scripts/` | 构建、打包、准备资源、验证和清理脚本 |
| `component-tools/` | x86 宿主 C++ 源码、系统辅助脚本与安装钩子 |
| `third-party/moonlight-extension/` | LL 自己维护的 Moonlight 扩展源码、补丁与应用脚本 |
| `docs/` 中正常项目资料 | 项目总结、AI 指南、版本记录、产品方案、设计 HTML/CSS/JS/DOCX、截图 |
| `docs/validation/` | 已整理的版本验收摘要；发布前确认不含账号密码、令牌或私钥 |
| `docs/cleanup-history/` 中说明、脚本和摘要 | MD、PY、PS1、MJS，以及 validation、retained-verification、preservation 摘要 |
| `docs/cleanup-evidence/` 中原始辅助脚本 | MD、PY、PS1、MJS；重复内核资源目录继续排除 |
| `notices/` 中许可证与说明 | MD/TXT、Qt 许可证、Sunshine UTF-8 清单与生成脚本、源码归档 manifest |
| `engines/manifest.json`、`engines/README.md` | 固定内核版本/哈希和资源说明，实际内核文件不上传 |
| `runtimes/README.md` | 运行依赖准备说明 |
| 根目录工程文件 | `package.json`、`package-lock.json`、`index.html`、`tsconfig.json`、`vite.config.ts` |
| 根目录说明和规则 | `README.md`、`CHANGELOG.md`、`AGENTS.md`、`.gitignore` |
| `.npmrc` | 当前仅配置项目缓存、fund 和 audit，可上传；以后不要在该文件写入认证令牌 |

DOCX、截图及源码内的图片有实际设计用途，保留在仓库中。当前这些资料不需要 Git LFS。

## 2. 不上传到普通 Git 仓库的内容

| 路径或类型 | 为什么不上传 | 克隆后如何恢复 |
| --- | --- | --- |
| `node_modules/` | 安装的前端依赖 | `npm ci` |
| `dist/` | 生成的前端构建 | `npm run build` |
| `src-tauri/target/` | Rust 缓存、EXE、PDB 与复制的资源 | Rust/Tauri 构建重新生成 |
| `src-tauri/gen/` | Tauri 自动生成的能力与配置 schema | 按项目 Tauri 开发/构建流程生成 |
| `.tools/` | Qt SDK、Python/安装工具、jom | 安装或准备对应固定版本的本机工具 |
| `.work/` | 临时构建、测试副本、诊断数据和下载缓存 | 脚本重新生成；原始证据留在本机备份 |
| `engines/` 中实际程序和资源 | 完整第三方内核、DLL、Qt/Web 资源 | 从内核样本准备，再构建 LL 扩展并更新清单 |
| `runtimes/` 中 DLL、`src-tauri/runtimes/` | 本机准备的 MSVC/历史 WebView2 运行资源 | `prepare-runtime.ps1`；当前应用使用系统 WebView2 |
| `third-party/sources/` | 大份上游源码及预编译依赖 | 取得固定 Moonlight 上游与对应依赖，按扩展说明准备 |
| `release/` | 最终运行目录、ZIP、可能存在用户 Data | 从源码完整构建和打包；交付包单独分发 |
| 根目录 `LLRemote.EngineHost.exe` | C++ 编译输出 | `node scripts/build-engine-host.mjs` |
| `notices/sources/*.zip` | 约 77 MiB 的生成源码归档，普通开发仓库无需存重复压缩包 | `python scripts/prepare-sources.py`，先补齐上游源码 |
| `docs/workspace-*.json` | 本机路径、体积盘点、完整文件哈希与清理计划 | 本机重新盘点和审核生成，不能重放旧计划 |
| 清理历史中的全量 JSON、归档清单和日志 | 大型本机快照或原始采样，摘要和脚本已单独保留 | 本机保留；需要取证时单独提供相关材料 |
| 清理证据中的重复内核/许可证/组件资源目录 | 测试时复制的旧资源，已有正式来源 | 使用当前资源和构建步骤 |
| `Data/`、`credentials/`、根目录 `config` | 用户配置、登录和配对凭据 | 原机保留；新电脑/新 Windows 用户重新认证 |
| `.env`、`.env.*`、私钥/证书容器 | 本机环境及敏感材料 | 使用不含秘密的 `.env.example` 或 `.env.sample` 模板 |
| EXE/DLL/PDB/OBJ/LIB/RLIB/RMETA、安装包、压缩包、日志、ETL/DMP | 编译、交付或诊断产物 | 构建/采样生成，需要分发时使用独立渠道 |
| `test-results/`、`playwright-report/`、`coverage/`、Python 缓存 | 自动验证产物 | 运行对应验证重新生成 |

`.gitignore` 按目录和类型执行这些规则。压缩归档排除规则不影响 `.docx`；日志规则不影响源码中的日志处理代码。将来若需要提交特定二进制测试样本，先确认用途，再加明确例外，避免整体取消排除规则。

## 3. 文档和脚本的上传原则

正常项目文档、设计稿、截图、所有正式构建/验证脚本都上传。清理归档只上传说明、辅助脚本和简洁结果，完整盘点与原始采样保留本机。因此仓库中不会包含清理清单指向的全部本机 JSON，也不会包含 `.work/wintun-diagnostic` 原始采样；相关链接用于原工作区审阅，接手者应优先看已上传的版本摘要。

当前仓库规则不上传 `docs/cleanup-evidence` 中复制的内核 Web 页面和重复辅助资源。需要持续维护的诊断/测试工具应放到 `scripts/` 或 `tests/`，不要长期依赖 `.work` 中的一次性文件。

发布软件时，GPL 组件对应源码和许可证仍需按要求提供。把生成的源码 ZIP 从普通 Git 排除，不会取消实际交付时准备对应源码材料的要求。皎月连的专有资源继续单独管理，不把本机资源上传视为公开再分发授权。

应用 ZIP 使用 [GitHub Releases](https://github.com/chuanK6/LL-Remote/releases/tag/v0.1.12) 附件分发，不加入普通 Git 提交。发布前检查最终 ZIP 的实际内容，确保 Data 为空并排除根目录 config、私钥和缓存；不能只相信清单的 cleanData 标记。本次发现本机原 ZIP 已含运行数据，保留原包并制作了单独的干净下载副本。

## 4. 从 Git 克隆后恢复开发环境

Git 仓库用于保存工程和交接资料，完整桌面打包还需要本机原生资源。恢复顺序如下：

1. 阅读根目录 `AGENTS.md` 和 [AI 接手指南](AI接手指南.md)，安装 Node.js 24、Rust MSVC、VS C++ Build Tools、Windows SDK 和系统 WebView2；Python PE 脚本需要 pefile。
2. 执行 `npm ci`，然后 `npm run check`、`npm run build`，可以先进行网页开发。
3. 按脚本固定目录准备 Qt 6.12.0 MSVC x64、jom 等本机工具，并创建 `.work` 临时根目录，供内核构建日志使用。
4. 准备 Moonlight v6.2.0 固定上游及其依赖到 `third-party/sources/moonlight`，以及 Sunshine、皎月连运行资源。`prepare-engines.ps1` 的 `-NativeRoot` 默认是 `E:\kongping`，新环境可以提供自己的资源目录。
5. 按接手指南依次准备内核、应用 Sunshine UTF-8 清单、构建 x86 宿主和 Moonlight 扩展、准备 MSVC 运行库与对应源码归档。
6. 构建 Tauri 主程序、生成便携目录、运行资源校验，再在隔离环境验证运行。当前前端单测已有记录的问题见接手指南，不引用旧版全绿数量。

`prepare-engines.ps1` 不负责下载完整 Moonlight 上游源码；`prepare-sources.py` 主要生成对应源码归档，也不负责补齐 Moonlight 上游。不要在只克隆源码后直接运行打包脚本，并把缺少内核资源误认为项目源码不完整。

## 5. 首次提交前如何检查

当前工作区已初始化 Git，主分支为 `main`，`origin` 指向用户指定的仓库 `https://github.com/chuanK6/LL-Remote.git`。首次上传及后续提交前，可以在项目根目录检查：

```powershell
git status --short --untracked-files=all
git status --short --ignored
git add --dry-run .
```

确认候选清单后再执行 `git add .`，并用 `git diff --cached --stat` 查看实际暂存内容。正常提交中应能看到源码、锁文件、文档、脚本和许可证；不应看到用户 Data、运行内核、SDK 或大型压缩包。提交后使用 `git push origin main` 推送到已配置仓库。

对已有仓库，`.gitignore` 只影响未跟踪文件，不会自动取消已提交文件的跟踪。若以后发现不应上传的产物已被跟踪，应先检查实际跟踪清单，再针对具体路径处理；不要直接批量删除本地文件。本次首次上传使用现有排除规则，未将本机运行资源和用户数据纳入跟踪。

后续增加目录、改变资源恢复步骤或正式发布方式时，同步更新本清单和 `.gitignore`。
