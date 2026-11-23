# LL远程 AI 接手指南

适用快照：2026-11-23，软件 0.1.12。项目拥有者确认初版已经能够正常使用；接手工作应从现有实现继续。

源码仓库应包含哪些文件、哪些原生资源需要在克隆后补齐，见 [Git 上传清单](Git上传清单.md) 和 [当前上传文件列表](Git上传文件列表.txt)。

## 1. 十分钟了解项目

先阅读 [项目总结](项目总结.md)，了解产品和三个内核分工，再看 [版本演进记录](版本演进记录.md) 的版本总表与 0.1.9—0.1.12。然后打开 `package.json`、`src/bridge.ts`、`src/stores/app.ts`、`src-tauri/src/commands.rs` 和 `src-tauri/src/runtime.rs`，把“界面操作如何变成原生行为”串起来。

遇到连接问题，继续阅读 [连接流程与组网地址](连接流程与组网地址.md)；遇到 Wintun 错误，先读 [故障排查](虚拟网卡创建故障排查.md) 和当前错误日志。完整 V1.3 方案适合按修改主题查阅，不必首次接手就读取所有历史清理清单、SDK 文件和构建缓存。

理解当前行为时优先结合用户最新确认、当前代码与配置、对应版本验证记录。旧方案与历史文档帮助解释原因；同一个文档中的旧版本段落不能直接当作 0.1.12 的运行规则。

## 2. 代码地图

| 修改主题 | 首先查看 | 需要一起检查的部分 |
| --- | --- | --- |
| 主窗口、导航、首次 PIN 弹窗 | `src/App.vue`、`src/router.ts` | `src/pages/`、`src/components/`、路由和配对测试 |
| 接受控制与分享信息 | `src/pages/AssistPage.vue`、`src/local-share.ts` | `src/stores/app.ts`、`commands.rs`、`runtime.rs`、输入保持测试 |
| 前端状态和后台刷新 | `src/stores/app.ts` | `src/types.ts`、`src/bridge.ts`，账号切换和迟到结果 |
| 前后端接口 | `src/bridge.ts`、`src-tauri/src/commands.rs` | `src/types.ts`、Rust `model.rs`、`lib.rs` 的命令注册、窗口权限 |
| 连接、取消、重连、后台接受控制与退出 | `src-tauri/src/runtime.rs` | `engines/` 适配层、取消令牌、操作锁和组件生命周期 |
| 皎月连协议、地址、Wintun 状态 | `src-tauri/src/engines/nat.rs` | `component-tools/EngineHost.cpp`、`GetPeerAddress.ps1`、引擎清单 |
| Sunshine 账号、配置、配对、服务 | `src-tauri/src/engines/sunshine.rs` | `sunshine_service.rs`、`commands.rs`、中文路径与证书处理 |
| 防火墙 | `component-tools/ConfigureSunshineVpn.ps1` | Rust 调用及警告/错误分离、Apply/Reapply/Restore 原生证据 |
| Moonlight 配置和串流 | `engines/moonlight.rs`、`engines/moonlight_preferences.rs` | `third-party/moonlight-extension/`、`scripts/build-moonlight.mjs` |
| 悬浮球、DPI、窗口跟随 | `src-tauri/src/controls.rs` | `src/pages/SessionPanel.vue`、`SessionPerformance.vue`、Moonlight SDL 事件 |
| Windows 权限、自启动、防休眠、路径 | `src-tauri/src/platform.rs`、`main.rs` | `windows-app.manifest`、`lib.rs`、`runtime.rs` |
| 配置、设备去重、账号隔离、凭据 | `src-tauri/src/storage.rs`、`model.rs` | `domain.ts`、迁移/备份/DPAPI 测试 |
| 被控端声音处理 | `src-tauri/src/host_audio.rs` | 运行状态和退出恢复 |
| 打包与资源 | `scripts/package.ps1`、Tauri 配置 | `engines/manifest.json`、`runtimes/`、`notices/`、资源校验脚本 |

表中未写完整前缀的 Rust 文件均位于 `src-tauri/src/`。CSS 主要位于 `src/styles.css`、`src/app.css` 和组件内；参考设计在 `docs/LL远程-界面草图-V1.3.html` 及配套 CSS/JS 中。

网页预览使用 `src/preview.ts`，桌面通过 Tauri 调用真实后端。不要用预览设备或模拟指标去解释实际登录、组网和串流状态。

## 3. 数据和运行模式

| 模式 | 数据位置 | 如何识别 |
| --- | --- | --- |
| 便携版 | EXE 旁的 `Data/` | `portable.flag` 或 `--portable` |
| 非便携版 | `%LOCALAPPDATA%/LLRemote` | 没有便携标记时按当前用户目录保存 |
| Debug 隔离测试 | `LLREMOTE_TEST_DATA` 指定的位置 | 只在 Debug 构建生效，Release 不能依赖它隔离数据 |
| WebView2 | 当前应用数据目录内的 `WebView2/` | 主程序设置浏览器缓存位置，使用系统共享运行时 |

便携 Data 内还包含各内核配置、设备/历史数据、日志、壁纸和加密凭据。具体文件路径以 `runtime.rs`、`storage.rs` 和运行日志为准。账号密码与历史连接密码由 Windows DPAPI 保存，普通 JSON 保存凭据引用；更换电脑或 Windows 用户后通常需要重新认证。

在读日志或制作报告时，不输出密码、PIN、令牌、私钥和完整账号配置。清理、升级或打包时保留用户 Data、根目录 `config`、已有配对和备份。故障日志应保留数值错误码和时间线，避免把中文显示乱码误当成数值证据。

## 4. 开发环境与常用命令

需要 Windows x64、Node.js 24、Rust stable MSVC、Windows SDK、Visual Studio C++ Build Tools 和系统 WebView2。Moonlight 还依赖 `.tools/Qt/6.12.0/msvc2022_64` 与 jom；Python 脚本使用本机 Python，PE 处理/校验需要 `pefile`。

在项目根目录运行：

```powershell
npm run check
npm run build
npm test
```

现有工作区保留了 `node_modules`。在新环境缺少依赖时执行 `npm ci`；不要仅为接手项目升级 Node、Rust、Qt、第三方内核或依赖版本。

仅查看界面时：

```powershell
npm run dev
```

需要真实桌面功能时：

```powershell
npm run desktop
```

网页预览不执行真实远控。桌面进程涉及本机配置、登录及服务；调试前明确数据目录，尤其是“始终接受控制”可能在启动时使用已有凭据开启服务。Rust/Tauri 包装脚本会寻找当前用户 Cargo 与 VS 工具链，不需要手工修改全局 PATH。

按修改范围选择验证：

```powershell
npm run test:ui
npm run rust:check
npm run rust:test
```

Playwright 需要可用的浏览器依赖。Rust 原生测试中存在显式忽略的场景；标准 `rust:test` 不能证明这些场景已执行。编译缓存已清理，首次 Rust/Moonlight 构建耗时会增加。

## 5. 当前测试基线

2026-11-23 清理后的前端生产构建和 0.1.12 便携包资源校验通过。最近一次 `npm test` 是 **31 通过、3 失败，另有 7 次未处理异常**，不是全绿。

`tests/unit/device-order.test.ts` 与 `tests/unit/pairing.test.ts` 的 bridge 替身没有提供 `listenOperationError`，而 `app.init()` 已调用它；配对断言仍预期空名称，当前 `App.vue` 默认使用 `roth` 并要求设备名。相关文件在清理前后未变。先核对当前产品行为、同步测试，再区分新增修改造成的失败；不要为通过旧断言恢复过时的产品行为。

0.1.11 的 34/16/54 项测试通过记录是历史证据，不能写成 0.1.12 这次全部通过。清理后的原始结果见 [validation.json](cleanup-history/2026-11-23/validation.json)。

## 6. 修改后端和接口时的注意点

新增或改动命令时，同时检查前端类型、`bridge.ts`、Rust 参数/模型、`commands.rs`、`lib.rs` 命令注册及 `src-tauri/capabilities/`。主窗口、会话控制窗口和性能窗口权限不同；保持原生端的窗口标签校验，敏感操作不能仅依赖前端隐藏按钮。

状态由后端确认。组网已连接、Sunshine 可达、PIN 已确认、Moonlight 已配对和真实桌面已渲染是不同阶段。保留取消、超时、认证失效与重试分支，不把进程启动或 WebSocket 成功当作桌面已可用。

后台刷新、账号切换与接受控制涉及并发。操作取得锁后重新读取偏好；旧请求不能覆盖新账号或用户刚刚关闭的状态。停止未完成的启动必须真正请求内核停止并等待确认，不能只清除本地变量。停止失败要保留可再次处理的状态。

## 7. 网络、权限和内核中的关键约束

| 约束 | 修改时需要保留的行为 |
| --- | --- |
| 本机/对端 IP | `conpc` 第 4 项和部分设备回包尾部 IP 是客户端本机地址；同账号优先查询 `pcinfo`，识别码连接保留对方组网地址输入 |
| 主机身份 | 已保存地址重连时核对 Sunshine UUID；地址变化或 UUID 冲突不能误合并不同电脑 |
| PIN 与证书 | 首配必须完成真实握手和落盘；Qt OpenSSL TLS 插件与 Moonlight 退出前保存不可省略 |
| 账号状态 | Sunshine 账号已设置与 LL 持有可用于 PIN 管理的凭据分开处理，不因缺少管理凭据重新阻止皎月连登录 |
| 页面密码 | 皎月连页面访问密码与连接密码分别处理，格式错误不误报为对端认证失效，后台刷新不清空输入 |
| 防火墙 | 按实际 Sunshine 路径、接口、IP、端口匹配生效规则，写入后回读；退出/停止与后台写规则串行化 |
| 防火墙失败 | 当前版本单独提示警告，保持已启动服务；不能恢复 0.1.7/0.1.8 的关键启动门禁行为 |
| Windows 权限 | 主程序使用 `requireAdministrator`，网络宿主继承权限；旧文档中的“主程序普通权限”属于历史实现 |
| DLL 与 ABI | 主程序 x64，皎月连宿主 x86；原 `cdecl Run(int port)` 和原 Go DLL 参数解析须保持，路径使用规定环境变量 |
| Wintun 日志接入 | 宿主按固定 SDK 身份、哈希和内存布局校验后接入日志；升级内核必须重新验证，不直接沿用内存偏移 |
| 原生诊断 | Rust 核对 PID、操作开始时间和阶段；旧错误或清理阶段日志不能冒充本次网卡创建失败 |
| Moonlight 管理 | 管道限定当前用户，请求校验令牌和 PID；窗口/鼠标操作交给原 SDL 会话线程 |
| 会话指标 | 从真实 VIDEO_STATS 读取，有渲染帧才标为就绪；断开本次串流默认保留远端主机应用 |

Windows 自动 Block 规则可能覆盖 Allow 规则。不要用关闭整个防火墙、删除所有阻止规则或全网卡重置替代定位。0.1.6 的 Wintun 修复只针对已确认的旧离线设备冲突；遇到新故障时先收集证据，不能直接套用删除设备步骤。

组件退出需要区分 LL 创建、当前复用和其他程序拥有的进程及会话，遵守当前生命周期策略。不要凭进程名称批量结束 Sunshine、Moonlight 或皎月连，也不要卸载驱动来清理测试。

## 8. 正确构建和交付

已保留内核和宿主、只改 LL 代码时，可执行：

```powershell
node scripts/tauri.mjs build --no-bundle --config src-tauri/tauri.release.conf.json
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package.ps1 -Portable
python scripts/verify-bundle.py release/LLRemote-Portable-x64-0.1.12 --reference .
```

示例校验路径对应当前版本；升级后换成新版本目录。`package.ps1` 从 `package.json` 读取版本，直接使用 `src-tauri/target/release/LLRemote.exe` 等资源，要求扩展清单和 UTF-8 Sunshine 标记正确。先构建再打包，避免把旧 EXE 当作新改动交付。

打包脚本生成完整便携目录，不会自动刷新同名 ZIP。需要压缩交付时，另行生成 ZIP，解压到隔离目录并验证；交付包 Data 必须为空，运行资源不能含用户配置、日志或私钥。同名输出已有用户 Data 时脚本会拒绝覆盖，应保留原目录并使用明确的新输出，不能删除用户数据绕过检查。

公开 ZIP 放在 GitHub Releases。已运行过的包可能重新包含 Data、根目录 config 和浏览器缓存，即使 component-manifest 仍写着 cleanData:true 也需要检查实际归档。本机原 0.1.12 ZIP 已保留，公开下载使用去除个人数据、修正中文文件名并完成校验的副本。

新版本同步 `package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json` 及相应锁文件中的项目版本、README、CHANGELOG 和该版本验收记录。检查生成 EXE、目录、ZIP、版本与校验记录属于同一次最终构建。

确实需要重新准备内核或重建扩展时，按以下顺序：

```powershell
# 从本机内核样本重新复制资源，会覆盖修改过的内核，应确认需要后执行。
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-engines.ps1 -NativeRoot E:\kongping
python scripts/prepare-sunshine-unicode.py
node scripts/build-engine-host.mjs
node scripts/build-moonlight.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-runtime.ps1
python scripts/prepare-sources.py
```

`E:\kongping` 是开发资源来源，可通过参数替换；正式应用不依赖这个本机路径。`prepare-engines.ps1` 应在扩展构建和 Sunshine UTF-8 修改之前执行。修改第三方扩展后更新对应源码包和清单，保持交付程序与源码材料一致。

当前使用系统 Evergreen WebView2。旧 `src-tauri/runtimes/WebView2` 仍在工作区，属于保留的历史资源，不需要再次打进当前便携包。Sunshine UTF-8 清单改动使其成为未签名修改版，不能宣称仍由上游原签名覆盖。

## 9. 实机验证与工作区保护

资源校验可直接读取现有包；启动类验证会创建进程、数据及服务，先查看脚本的测试范围、隔离方式和退出清理逻辑。Release 隔离不要依赖 `LLREMOTE_TEST_DATA`。在运行新包前，从托盘完全退出旧 LL 实例，单实例或旧宿主占用端口可能干扰结果。

常见脚本：

| 脚本 | 作用和范围 |
| --- | --- |
| `verify-bundle.py` | 依赖、清单和资源比对；不证明实际远控成功 |
| `smoke-packaged.mjs` | 隔离数据的完整包启动验证 |
| `smoke-onboarding.mjs` | 首用流程；部分皎月连行为使用本机协议测试服务 |
| `smoke-native.mjs` | 原生桌面探测，先阅读其目录和进程处理 |
| `probe-host.mjs`、`probe-moonlight.mjs` | 独立内核接口/扩展探测 |
| `workspace-inventory.py` | 工作区大小盘点，不跟随目录联接 |
| `clean-workspace.ps1` | 默认预览清理计划；旧计划必须重新审核后才能执行 |

保留 `release`、Data、配置、源码、依赖、`.tools`、`third-party`、`engines`、`runtimes`、`notices` 和原始诊断证据。删除目录前解析绝对路径并确认在目标工作区内，目录联接只删除联接本身，不递归访问目标。

当前工作区已初始化 Git，主分支为 `main`，远程仓库为 `https://github.com/chuanK6/LL-Remote.git`。修改前检查 `git status`、现有文件和用户变更，保持可审阅的差异；被忽略的用户 Data、内核及本机证据不由 Git 保护，需要另行保留。

## 10. 交付一次修改时应留下什么

说明具体触发问题、最终行为、改动入口、实际执行的验证和待验场景。按“源码检查、单元测试、网页交互、本机原生、真实环回、双电脑实机、干净系统”记录范围，并注明使用真实组件还是协议测试服务。

与版本有关的结果写到 `docs/validation/<版本>.json`，用户提供的实测结论写明其来源及环境，不补写未提供的成功场景。更新 CHANGELOG 和 [版本演进记录](版本演进记录.md)，涉及架构、数据或打包入口时同时更新本指南。

## 11. 可以直接发给其他 AI 的接手提示

```text
请接手当前工作区的 LL远程项目。当前软件版本 0.1.12，项目拥有者已确认初版能够正常使用。
先阅读 AGENTS.md、docs/项目总结.md、docs/AI接手指南.md 和 docs/版本演进记录.md，
再查看 package.json、src/bridge.ts、src/stores/app.ts、src-tauri/src/commands.rs、
src-tauri/src/runtime.rs 和本次任务相关模块。

项目是 Windows x64 的 Tauri 2 + Vue 3 + TypeScript + Rust 应用，
集成皎月连 v1.06、Sunshine 和带 LL 扩展的 Moonlight，皎月连宿主为 x86。
请沿用现有架构，保护用户 Data、DPAPI 凭据、配对证书和正在使用的服务。
网页预览与真实桌面功能分别验证；当前使用系统 Evergreen WebView2。

最新前端单测有已记录的失败，先核对基线，不能引用历史通过数量当作当前测试结果。
修改完成后说明触发问题、最终行为、验证范围及待验事项；按需要更新版本和交接文档。

本次需要你完成的任务：<在这里填写具体功能、问题及复现步骤>。
```
