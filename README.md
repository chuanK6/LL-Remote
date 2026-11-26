# LL远程

按 `docs/LL远程-软件方案.md` V1.3 实现的 Windows 远程桌面初版，使用 Tauri 2、Vue 3、TypeScript、Pinia、Vue Router 和 Rust。项目拥有者已确认初版可正常使用。

项目资料：[初版总结](docs/项目总结.md) · [AI 接手指南](docs/AI接手指南.md) · [各版本处理的问题](docs/版本演进记录.md)。其他 AI 接手时先读根目录 [AGENTS.md](AGENTS.md)，再按指南定位代码与验证入口。

当前版本为 **0.1.15**。移除「全部设备」入口及页面，保留设备资料；连接记录和收藏只将左侧图标替换为壁纸缩略图，保留名称、连接信息、备注和时间，修正图片覆盖整行的问题。默认进入开始协助，首次使用引导保留。用户已确认修正版功能测试通过，0.1.15 沿用验收代码、更新版本配置并重新构建；本轮不重新运行单元、网页、桌面或内核测试。见 [发布说明](docs/releases/0.1.15.md) 和 [验证记录](docs/validation/0.1.15.json)。

版本管理：[Git 上传清单](docs/Git上传清单.md)。源码、配置、文档和脚本上传；本机 Data、SDK、依赖、内核资源、构建与交付产物由 `.gitignore` 排除，克隆后的资源恢复步骤见清单。

交付目录规则（2026-11-25）：最终程序与 ZIP 保留在 `release/`；其余临时交付目录和程序备份在编译、校验并同步 release 后删除，GitHub 上传材料上传完成后删除。临时打包统一用 `.work/package-current/`，不再向早期任务目录写新文件；源码、工具、用户数据及诊断证据继续保留。详见 [目录盘点与清理](docs/0.1.14目录与备份盘点.md)。

源码仓库：[chuanK6/LL-Remote](https://github.com/chuanK6/LL-Remote)，主分支 `main`。

应用下载：[0.1.15 发布页](https://github.com/chuanK6/LL-Remote/releases/tag/v0.1.15) · [Windows x64 便携版 ZIP](https://github.com/chuanK6/LL-Remote/releases/download/v0.1.15/LLRemote-Portable-x64-0.1.15.zip) · [SHA-256 校验文件](https://github.com/chuanK6/LL-Remote/releases/download/v0.1.15/LLRemote-Portable-x64-0.1.15.zip.sha256.txt)。公开下载包不包含发布者的本机 Data 和配置。旧版 [0.1.14](https://github.com/chuanK6/LL-Remote/releases/tag/v0.1.14)、[0.1.13](https://github.com/chuanK6/LL-Remote/releases/tag/v0.1.13) 和 [0.1.12](https://github.com/chuanK6/LL-Remote/releases/tag/v0.1.12) 发布页保留。

0.1.14 于 2026-11-25 追加壁纸重连修复：目录 ID 变化后衔接原设备资料，同设备在线详情、多条连接记录详情及收藏共用壁纸，修改任一入口全部更新。用户测试后确认功能正常，同版本 GitHub 源码及附件已更新；沿用验收 EXE，不重新运行测试。见 [本轮说明](docs/releases/0.1.14-wallpaper-reconnect.md)、[验证记录](docs/validation/0.1.14-wallpaper-reconnect.json)、[发布回执](docs/releases/0.1.14-wallpaper-publication.json) 和 [完整发布说明](docs/releases/0.1.14.md)。附件保留原始文件名，不设置中文显示名称。0.1.13 的壁纸修复、5 MB 上限、默认托盘及 1 Mbps 步长继续保留。[更新日志](CHANGELOG.md)

0.1.14 原候选收到移动 Data 后重建旧版目录、全屏连接不显示悬浮球的反馈。修正版启动时更新随包 Sunshine 的旧绝对路径，调整悬浮球创建及显示，性能监测默认关闭且关闭时不创建窗口。新连接默认窗口模式，会话内切换全屏及内部重连继续可用；最新版本已有用户当前测试无问题的确认。

使用系统共享 WebView2。接受控制并行启动皎月连和 Sunshine，并在后台准备限定到组网接口的防火墙规则。远程设备按已确认的身份复用记录。主程序请求 Windows 管理员权限，网络宿主在加载内核前按绝对路径预加载 Wintun DLL。

网卡初始化和服务就绪分阶段等待，原生诊断保留 Windows 错误码、设备问题码及 NTSTATUS。已在本机确认并修复一条旧离线 Wintun 接口的持久编号与运行时编号冲突，连续三次真实网卡创建和收发会话启动成功。根因与验证范围见 [虚拟网卡创建故障排查](docs/虚拟网卡创建故障排查.md)。

皎月连设置显示本机组网地址。协助页在接受控制生效后显示识别码、连接密码与组网地址，并支持全部复制。识别码连接需要填写对方组网地址，同账号设备会尝试自动读取地址。两种连接方式都在 Sunshine 可达后执行首次 PIN 配对，已配对设备复用证书。详见 [连接流程与组网地址](docs/连接流程与组网地址.md)。

## 运行

**升级用户请先保存旧版整个 Data 文件夹。** 从托盘「完全退出」旧版，再完整解压新版，将旧 Data 复制到新版 EXE 同级目录，替换下载包的空 Data；同机同 Windows 用户无需重新配置账号、设备、收藏、画质和配对。不要只保留 preferences.json，也不要套成 Data/Data。更换电脑或 Windows 用户时，加密凭据需要重新认证。步骤见 [升级说明](docs/releases/0.1.15.md)。

收藏设备和连接记录使用同一套列表卡片，悬停出现删除按钮，点击立即删除设备及其全部连接记录、收藏，不再确认。设备资料已移除的历史不在列表显示。恢复备份同步清理，默认图和其他设备共享的资料保留；升级程序仍保留用户 Data，删除仅由用户在应用内点击触发。
0.1.14 的「我的设备」仅显示同账号在线主机；旧连接资料保留在「开始协助」下方的「连接记录」入口中。记录按时间排列为可高亮的列表卡片，进入详情后可返回列表，不显示主机在线状态。设备名右侧的五角星可直接收藏，收藏后变为黄色并提示；已有收藏、配置和配对保留。用户已确认当前测试无问题。

- 完整解压 `LLRemote-Portable-x64-0.1.15.zip` 后运行 `LLRemote.exe` 并接受管理员授权。程序旁的 `Data` 保存本机数据。
- 目录包含三个内核及其依赖，界面复用系统 WebView2。系统未安装时会提示打开微软官方安装页，安装一次后可启动。
- 首次使用创建或验证本机 Sunshine 管理账号，然后登录皎月连。已有运行内核与 Moonlight 配对优先复用。
- 关闭主窗口默认进入托盘，可以在设置里关闭「关闭窗口进入托盘」以恢复关闭即退出。已有用户明确保存的选择继续保留。托盘的「停止本机接受控制」会暂停本次运行的被控服务。「完全退出」会关闭应用。防休眠默认开启，自启动默认关闭。

Sunshine 服务未运行且普通用户权限不足时，LL 使用自身受限的授权组件弹出 Windows 权限确认，完成服务启动。已运行服务通过本机 HTTPS 接口同步账号状态。普通用户读不到系统进程路径时，通过 Windows 服务 PID、监听 PID 和直接子进程确认 Sunshine 身份，再绑定公共证书。

在 Sunshine 高级设置中完成账号设置后，LL 自动将第一步更新为已完成。点击导航后可从顶部“完成首次连接”返回未完成步骤；完成账号登录后按钮仍显示，只有实际连接成功并记录历史后才隐藏。设置页保留相同入口。

启用本机接受控制时，LL 提交连接密码和组网页面访问密码，并等待组网配置完成。页面密码可单独填写，要求 6–20 位；留空时复用已有页面密码或连接密码。启动失败后保留输入，已登录账号继续操作无需重复输入账号密码。

切换页面后从皎月连状态重新读取并以密码框回填，有效输入另存入当前 Windows 用户的 DPAPI 凭据。主程序以管理员权限运行，创建虚拟网卡时网络宿主继承权限。主动连接与开启接受控制均能识别网卡初始化错误，并在需要时恢复网络宿主登录后重试。

随包 Sunshine 的进程清单启用 UTF-8，配置、证书和应用文件使用各自 Data 的绝对路径，支持中文和空格路径。该运行清单改动使程序成为未签名的修改版；对应清单及可重复生成脚本随第三方源码材料提供。

LL 内的 PIN 确认与采集设置使用加密保存的 Sunshine 管理凭据。浏览器中的账号设置可以直接完成第一步；需要在 LL 中操作 PIN 或采集配置时，在 Sunshine 页验证一次管理账号即可。后台状态刷新不会禁用表单或清除正在输入的内容。

首次配对时控制端页面和连接横幅显示四位 PIN，被控端自动弹出确认窗口。填写控制端 PIN 和设备名并确认后自动进入桌面。0.1.12 设备名默认填入可修改的 `roth`，确认时需要填写；已有配对证书直接复用。

Moonlight 高级设置使用当前选中设备的参数，其支持的画质、窗口、输入与声音设置会同步回 LL。原生配置中的主机、配对与其他选项保留。随包补齐 Qt OpenSSL TLS 插件，配对证书可在进程退出后保存和复用。

首次配对 CLI 退出前等待主机配置落盘，避免配对成功后重启 Moonlight 又丢失证书。

三个内核、完整 DLL / Qt / Web 资源和 MSVC 运行库随包提供。WebView2 使用系统共享运行时。新环境的皎月连由独立 x86 宿主加载原 DLL，主程序保持 x64。

当前皎月连接口没有可确认的余额字段，账号页显示“暂无法读取”。识别码连接成功后会保存组网地址及确认过的主机身份。账号密码和历史连接密码使用 Windows DPAPI，普通设备 JSON 只保存凭据引用。

## 开发

0.1.14 在「设置 → 本机接受控制」增加「自动切换角色」，默认关闭。本机与目标组网地址相同且本机正在接受控制时，未开启会弹窗提供「开启功能」或「停止连接」。确认后保存选项、暂停本机接受控制并继续连接。

真正断开时，先确认串流和客户端组网退出，再等待至少 3 秒检查并恢复。分辨率、帧率、码率等内部重连保留原会话和暂停记录；手动停止本机接受控制或完全退出取消自动恢复。恢复失败提示原因并稍后重试。性能卡片跟随 Moonlight 客户区右下角，最小化或窗口不可用时隐藏。

要求 Node.js 24、Rust stable MSVC、Windows SDK、Visual Studio C++ Build Tools 与系统 Microsoft Edge WebView2 Runtime。脚本自动找到当前用户 Cargo 和 VS 工具链，不修改全局 PATH。Windows 主程序只生成需要的 Rust rlib，不额外生成移动端用的静态库和动态库。

```powershell
npm ci
npm run dev
npm run desktop
npm run build
npm test
npm run test:ui
npm run rust:test
```

`npm run dev` 是明确标记的网页界面预览；网页不会登录、连接电脑或保存真实密码。桌面版通过受限 Rust 命令读取真实内核状态。

## 内核与打包

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-engines.ps1
python scripts/prepare-sunshine-unicode.py
node scripts/build-engine-host.mjs
node scripts/build-moonlight.mjs
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-runtime.ps1
python scripts/prepare-sources.py
node scripts/tauri.mjs build --no-bundle --config src-tauri/tauri.release.conf.json
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package.ps1 -Portable
```

`prepare-engines.ps1` 从文档给定的 `E:\kongping` 复制运行资源，排除原配置、日志与私钥；它应在 Moonlight 扩展构建之前执行。`prepare-runtime.ps1` 只准备内核的 MSVC 依赖。交付打包要求空 Data，并在复制前过滤内核配置、日志和私钥；已有用户数据目录不会被覆盖。

便携目录的数据放在程序旁 `Data`，WebView2 缓存也随该数据目录保存。壁纸保存相对路径。DPAPI 凭据绑定 Windows 用户与电脑，更换用户或电脑需要重新认证。

设备壁纸属于设备资料，同设备的在线详情、全部连接记录详情和收藏详情共用；在任一详情页更换或恢复默认并保存后，其他详情页一起更新。目录名称唯一且已确认身份无冲突时，同账号设备离线重连的目录 ID 变化会衔接原资料。同名歧义保持独立，避免串用其他电脑的资料。

壁纸支持 PNG、JPEG、WebP，单张上传上限为 5 MB（5 × 1024 × 1024 字节，等于上限允许）。预览和正式保存均校验大小；超限图片不会替换已选壁纸。保存后的图片经后端路径校验并授权本地资源读取。

恢复默认或更换壁纸时，点击「保存」后删除旧自定义图片副本；取消编辑保留旧图。默认 SVG 与用户选择的原始文件保留。历史数据中若其他设备/账号共用同一图片，最后一个引用被移除时才删除。清理只处理应用 `Data/wallpapers` 下的图片，保存或删除失败会报告错误。

## 验证与范围

2026-11-23 清理后的前端构建和当前便携包资源校验通过；最近单元测试为 31 通过、3 失败，另有 7 次未处理异常，涉及缺少的测试替身接口与旧配对名称断言。当前基线及维护事项见 [AI 接手指南](docs/AI接手指南.md)，用户确认初版可用与历史自动验证范围分别记录。

0.1.10 的交付包、资源校验、实机防火墙结果与双机待验范围见 [0.1.10 验收记录](docs/validation/0.1.10.json)。运行日志保存在便携目录下的 `Data/logs`。此前版本见 [开发与验收记录](docs/开发与验收记录.md)。

系统运行时切换的实际启动、资源大小和 Data 保留结果见 `docs/validation/system-webview2.json`。WebView2 版本由系统更新维护，不限定 157.0.4341.60。

完整包验证前从托盘完全退出正在运行的 LL远程，随后在隔离目录运行 `node scripts/smoke-packaged.mjs "<目录>/LLRemote.exe"`。首用验证为 `node scripts/smoke-onboarding.mjs "<便携目录>"`，它另建测试目录、运行隔离的随包 Sunshine，并使用本机皎月连协议测试服务；附加 `--existing-sunshine` 则只读取现有系统服务。资源校验命令为 `python scripts/verify-bundle.py "<包目录>" --reference .`。

双端组网、实际串流、独占全屏悬浮、多屏 DPI、休眠 / 合盖 / 重启和干净 Windows 安装验证尚待后续验收。首次系统驱动或已有服务的权限要求仍遵循原内核；未将 API 启动成功当成跨电脑远控验收成功。

第三方许可、源码与再分发边界见 [第三方说明](notices/THIRD-PARTY-NOTICES.md)。应用通过 GitHub Releases 分发，当前未提供代码签名或自动升级。

## 致谢与友链

感谢 LINUX DO 社区为 LL远程提供技术支持。[访问 LINUX DO 社区](https://linux.do/)
