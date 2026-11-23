# LL远程

按 `docs/LL远程-软件方案.md` V1.3 实现的 Windows 远程桌面初版，使用 Tauri 2、Vue 3、TypeScript、Pinia、Vue Router 和 Rust。项目拥有者已确认初版可正常使用。

项目资料：[初版总结](docs/项目总结.md) · [AI 接手指南](docs/AI接手指南.md) · [各版本处理的问题](docs/版本演进记录.md)。其他 AI 接手时先读根目录 [AGENTS.md](AGENTS.md)，再按指南定位代码与验证入口。

版本管理：[Git 上传清单](docs/Git上传清单.md)。源码、配置、文档和脚本上传；本机 Data、SDK、依赖、内核资源、构建与交付产物由 `.gitignore` 排除，克隆后的资源恢复步骤见清单。

源码仓库：[chuanK6/LL-Remote](https://github.com/chuanK6/LL-Remote)，主分支 `main`。

应用下载：[0.1.12 发布页](https://github.com/chuanK6/LL-Remote/releases/tag/v0.1.12) · [Windows x64 便携版 ZIP](https://github.com/chuanK6/LL-Remote/releases/download/v0.1.12/LLRemote-Portable-x64-0.1.12.zip) · [SHA-256 校验文件](https://github.com/chuanK6/LL-Remote/releases/download/v0.1.12/LLRemote-Portable-x64-0.1.12.zip.sha256.txt)。公开下载包不包含发布者的本机 Data 和配置。

当前为 0.1.12，使用系统共享 WebView2。接受控制并行启动皎月连和 Sunshine，并在后台准备限定到组网接口的防火墙规则。远程设备按已确认的身份复用记录。主程序请求 Windows 管理员权限，网络宿主在加载内核前按绝对路径预加载 Wintun DLL。[更新日志](CHANGELOG.md)

网卡初始化和服务就绪分阶段等待，原生诊断保留 Windows 错误码、设备问题码及 NTSTATUS。已在本机确认并修复一条旧离线 Wintun 接口的持久编号与运行时编号冲突，连续三次真实网卡创建和收发会话启动成功。根因与验证范围见 [虚拟网卡创建故障排查](docs/虚拟网卡创建故障排查.md)。

皎月连设置显示本机组网地址。协助页在接受控制生效后显示识别码、连接密码与组网地址，并支持全部复制。识别码连接需要填写对方组网地址，同账号设备会尝试自动读取地址。两种连接方式都在 Sunshine 可达后执行首次 PIN 配对，已配对设备复用证书。详见 [连接流程与组网地址](docs/连接流程与组网地址.md)。

## 运行

- 解压 `LLRemote-Portable-x64-0.1.12.zip` 后，运行目录中的 `LLRemote.exe` 并接受 Windows 管理员授权。应用在程序旁的 `Data` 保存本机数据。
- 目录包含三个内核及其依赖，界面复用系统 WebView2。系统未安装时会提示打开微软官方安装页，安装一次后可启动。
- 首次使用创建或验证本机 Sunshine 管理账号，然后登录皎月连。已有运行内核与 Moonlight 配对优先复用。
- 关闭主窗口默认退出，也可以在设置里开启「关闭窗口进入托盘」。托盘的「停止本机接受控制」会暂停本次运行的被控服务。「完全退出」会关闭应用。防休眠默认开启，自启动默认关闭。

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

## 验证与范围

2026-11-23 清理后的前端构建和当前便携包资源校验通过；最近单元测试为 31 通过、3 失败，另有 7 次未处理异常，涉及缺少的测试替身接口与旧配对名称断言。当前基线及维护事项见 [AI 接手指南](docs/AI接手指南.md)，用户确认初版可用与历史自动验证范围分别记录。

0.1.10 的交付包、资源校验、实机防火墙结果与双机待验范围见 [0.1.10 验收记录](docs/validation/0.1.10.json)。运行日志保存在便携目录下的 `Data/logs`。此前版本见 [开发与验收记录](docs/开发与验收记录.md)。

系统运行时切换的实际启动、资源大小和 Data 保留结果见 `docs/validation/system-webview2.json`。WebView2 版本由系统更新维护，不限定 157.0.4341.60。

完整包验证前从托盘完全退出正在运行的 LL远程，随后在隔离目录运行 `node scripts/smoke-packaged.mjs "<目录>/LLRemote.exe"`。首用验证为 `node scripts/smoke-onboarding.mjs "<便携目录>"`，它另建测试目录、运行隔离的随包 Sunshine，并使用本机皎月连协议测试服务；附加 `--existing-sunshine` 则只读取现有系统服务。资源校验命令为 `python scripts/verify-bundle.py "<包目录>" --reference .`。

双端组网、实际串流、独占全屏悬浮、多屏 DPI、休眠 / 合盖 / 重启和干净 Windows 安装验证尚待后续验收。首次系统驱动或已有服务的权限要求仍遵循原内核；未将 API 启动成功当成跨电脑远控验收成功。

第三方许可、源码与再分发边界见 [第三方说明](notices/THIRD-PARTY-NOTICES.md)。当前未进行公开发布、代码签名或自动升级部署。
