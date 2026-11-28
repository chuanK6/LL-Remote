# LL远程 AI 接手指南

适用快照：2026-11-25，当前源码、本机及公开版本为 0.1.15，已发布 GitHub v0.1.15。用户确认上一轮缩略图定位修正版功能测试通过，本次沿用验收功能代码，统一更新 npm/Tauri/Rust 与锁文件中的版本号后重新构建，不升级依赖或内核，不重新运行单元、网页、桌面或内核测试。验收来自用户且未提供逐项环境矩阵；0.1.15 重建的 EXE 与验收 EXE 字节不同。本机 release 已改为 0.1.15，399 个原有 Data、配置及其他保留文件哈希不变。最新交付、下载复核与清理结果见 [0.1.15 记录](validation/0.1.15.json) 和 [发布回执](releases/0.1.15-publication.json)，升级与附件说明见 [发布说明](releases/0.1.15.md)。用户本轮明确不用核对旧版本，仅验证 v0.1.15 的附件与标签；附件继续保留原始文件名、空 label。

本次只修改 `SavedDeviceList.vue` 的两处缩略图定位样式：容器 `position: relative`，内部 `.cover-art` 重置为 `position: static; inset: auto`。共享详情页 `.cover-art` 原先的绝对定位以整个 `saved-device-item` 为范围，导致图片遮挡整行；不要只设置图片宽高而继续继承绝对定位。列表模板中的名称、方式、识别码、备注、时间、箭头和删除按钮沿用原布局。类型检查、生产构建及生成 CSS 检查通过，未启动网页、桌面或内核；运行效果待用户验收。

0.1.15 交付已完成：三个新版附件的公开下载字节数与 SHA-256 均匹配本机文件，`.work/package-current` 已按清理脚本删除。Data 保留哈希在目录改名/程序替换前后核对通过；发布期间后来观察到账号、内核资料及 WebView2 缓存变化，未恢复或覆盖这些后续文件。该观察不属于代理启动的运行测试，原候选的待验段落作为历史保留，用户已确认修正功能通过。

最新界面要求已明确：移除「全部设备」入口和页面，保留设备资料。`DevicesPage.vue` 删除，`FavoritesPage.vue` 只负责收藏筛选、排序、搜索和共用卡片；「我的设备」仍通过 `onlineAccountDevices` 显示在线主机。桌面默认首页、登录完成、详情设备移除后的跳转改为 `/assist`，旧 `/devices` 只作兼容重定向；store 的 `onboardingPath` 与 router 初次导航检查同步更新，首次设置/登录继续可达。`SavedDeviceList.vue` 左侧用 `DeviceWallpaper.vue` 读取 `item.device.wallpaper`，宽窗口 116×76，窄窗口 96×64/88×60，圆角裁剪；默认图和资源授权沿用详情页组件，删除按钮及记录引用不变。已有测试仅移除失效的全部设备表格场景、更新旧导航引用；用例未运行。详见 [本轮记录](validation/0.1.14-navigation-thumbnails.json)。

2026-11-25 壁纸修正：皎月连目录 ID 随主机离线重启变化，未取得 Sunshine UUID 的旧在线资料无法通过原身份规则衔接新 ID。本机只读数据检查发现旧条目仍有壁纸及图片、新已确认条目没有壁纸。`Runtime::refresh` 先构造完整在线目录，再调用 `Store::merge_account_directory`。独立保存 `Device.account_host_name`，避免 Sunshine 名称、昵称或 code 来源覆盖目录名称；旧 account 条目使用原 host_name 作迁移候选。仅在目录名称唯一、旧 ID 不属于另一在线行、已有身份没有指向其他目录名称且候选 UUID 无冲突时衔接，并交给原合并函数保留资料与重绑全部历史。已保存 UUID 始终保留为连接时的预期身份；目录名称用于资料连续性，不作为已确认主机身份。同名歧义/UUID 冲突保持独立。

壁纸只存在设备资料中。在线详情、历史详情和收藏详情均通过 `DevicePage.vue` / `DeviceEditor.vue` 按当前 deviceId 读写同一资料；历史不保存独立壁纸或创建设备。识别码连接确认同一 UUID 后继续共用该资料。新增五项回归用例覆盖未确认 ID 变化、历史/收藏共享修改及恢复默认、旧重复资料衔接、同名/UUID 冲突和目录名称独立性；只做编译，不运行用例、桌面或内核。用户随后自行测试并确认功能正常，未提供逐项矩阵，授权更新同版本 GitHub。详见 [壁纸重连修正记录](validation/0.1.14-wallpaper-reconnect.json)。保留个人 Data、配置、配对；不得将用户一般反馈扩展为自动化或全部环境通过。

0.1.14 角色切换位于 `src-tauri/src/runtime/role_switch.rs`，作为 runtime 子模块保存逻辑会话暂停记录及仅存在内存中的清零凭据。`automaticRoleSwitch` 默认 false。地址在连接前检查；同账号地址只能连接后确认时，结束本次客户端组网并在同一会话中重建一次。`resolve_role_switch` 仅允许主窗口针对当前待确认会话作决定，批准后持久化选项，拒绝不暂停本机服务。前端弹窗在 `RoleSwitchDialog.vue`，固定描述由 `src/role-switch.ts` 与设置页共用。

恢复仅从真实结束及清理完成处调度。至少等待 3 秒后核对相同会话 ID、终止阶段、`changing=false`、无 Moonlight 子进程和皎月连客户端连接、账号相同、手动操作代数未变化。画质等内部重连保持原 ID 和暂停记录，`update_session`/`kill_stream` 不调度恢复。停止失败保留进程句柄；恢复失败保留记录，8 秒后重试。手动停止、退出撤销恢复，始终接受控制的后台恢复也受暂停记录约束。性能卡片按 `controls.rs` 的原始客户区定位，悬浮球继续使用已有可见区域边界。见 [0.1.14 记录](validation/0.1.14.json)。

0.1.13：壁纸经 `wallpaper_path` 校验后按文件授予资源访问权限，解决 Windows 下保存后读取 403；`storage.rs::read_wallpaper` 对预览和保存统一限制 5 MB。设备编辑器验证成功才更新图片草稿，默认关闭到托盘（已有明确保存的选择不覆盖），共享画质组件数值步长 1 Mbps。新便携包单独输出，0.1.12 保留。用户指定后续运行测试自行完成，本次结果见 [0.1.13 记录](validation/0.1.13.json)。

用户确认原 0.1.13 功能测试通过后，要求恢复默认/更换图片并保存时删除旧自定义壁纸。`Store::edit_device` 先持久化新记录，再删除已无设备引用的旧副本；保存失败保留旧记录并清理未提交的新导入，删除失败尝试恢复原设置。仅处理 `Data/wallpapers` 下直接存放的 PNG/JPEG/WebP，检查实际路径，默认 SVG 和来源文件不删。用户要求保持 0.1.13 版本号；最终发布包使用 `package.ps1 -OutputRoot .work/github-release-0.1.13` 独立生成，保留本机已使用的同版本目录及 ZIP。

用户随后明确要求替换本机 0.1.13：更新程序和 ZIP，保留 Data、根目录 config 及其他本机配置；替换前程序文件与 ZIP 在 `.work` 单独备份，用户数据逐文件核对不变。0.1.12 继续原样保留。下载包从独立干净目录制作，不能把已运行的本机目录直接压缩上传。

源码仓库应包含哪些文件、哪些原生资源需要在克隆后补齐，见 [Git 上传清单](Git上传清单.md) 和 [当前上传文件列表](Git上传文件列表.txt)。

当前版本为 0.1.16，包含应用内签到、可自定义快捷键和 IP 直连。用户确认局域网直连可用，公网尚待实测，显示器熄屏及与 UU 同时控制的黑屏列为已知问题。此次公开交付沿用现有 Windows EXE，更新对应源码与发布资料。[0.1.16 说明](releases/0.1.16.md) · [构建记录](validation/0.1.16-shortcuts.json)

2026-11-28 用户报告局域网直连在管理员运行时仍报防火墙操作失败。直连脚本按 Windows 规则所需格式处理 Sunshine 程序路径，检查当前网卡的入站与本机规则策略，读回独立的 TCP 和 UDP 规则。失败后只清理自身规则，提示保留失败步骤及 Windows 状态码。台式机熄屏后首次串流黑屏的根因待实机定位，先核对 Windows 用户会话、Sunshine 活动显示输出和「保持显示器常亮」设置。[本轮记录](validation/0.1.16-lan-fix.json)

同日将 `delete_history_entry` 与 `delete_device` 分开。历史页按记录 ID 单条删除，兼容旧资料生成的历史卡片，删除最后一条后也会隐藏该卡片。设备资料及 `last_connected_at` 保留，新连接会产生新记录。整台设备删除入口位于收藏页，旧文档中的历史页整机删除仅作为 0.1.14 实现记录。

快捷键设置保存在 `Preferences.shortcuts`。旧版偏好缺失此字段会采用默认组合。主界面通过 `trigger_shortcut` 执行动作，LL 管理的 Moonlight 会话由 SDL 键盘处理捕获组合。断开和本机常亮只上报固定动作，LL 按串流进程及会话 ID 核对来源。会话内更新通过原认证 IPC 应用，普通 Moonlight 窗口继续使用原生键位。[快捷键交付记录](validation/0.1.16-shortcuts.json)

## 1. 十分钟了解项目

2026-11-24 用户明确反馈「目前测试没有问题」并授权 GitHub 发布。保留各实现阶段的待验记录作为历史，当前用户结论见 `validation/0.1.14.json`；不要把用户一般反馈扩大为逐项测试矩阵全部通过。本次不改程序、不重测，用验收的同一 EXE 制作空 Data 发布包，附 [修复与 Data 升级说明](releases/0.1.14.md)。升级提醒面向下载用户：保存整个旧 Data 后复制到新版，同机同 Windows 用户无需重新配置；当前个人 Data 留在本机，不打入 Git 或 Release。

最新追加的收藏/历史列表由 `SavedDeviceList.vue` 共用卡片渲染，无主机状态，悬停/键盘聚焦出现删除按钮，按钮独立于详情链接，点击不进入详情或弹确认。`HistoryPage.vue` 仅展示能按 deviceId 找到当前资料的历史，失去资料的历史不计入列表/数量；收藏详情 `/favorites/device/:id` 可返回收藏并保留搜索。

`Store::remove_device` 现在硬删除目标设备、收藏及该 ID 的全部历史。私有 `write_atomic(..., false)` 先更新已清理的 devices.bak，再替换 devices.json，两处均不包含删除记录，其他正常保存仍沿用原备份规则。只清理确属本账号目标身份且无其他设备/账号引用的连接授权，以及 `obsolete_wallpaper` 已校验的无引用自定义图；不清理默认图、来源文件、其他设备或独立内核原生配对。用户 Data 本轮不实际执行删除，应用内点击才触发。

`delete_device` 保留主窗口及会话门闩权限，加管理锁，删除前仅在缓存连接确认匹配目标时断开其客户端组网。设置同步遇到已删除设备会撤销旧绑定，不再让后续 snapshot 失败。清理失败返回错误并刷新显示真实保存结果，不报虚假成功。仍不运行单测、桌面或内核测试，用户负责运行验收。
0.1.14 追加连接记录页面：`MainSidebar.vue` 与普通 `DevicesPage.vue` 共用 store 的 `onlineAccountDevices`，仅取后端 `accountOnline === true` 且已认证的当前账号设备。不能用历史 `source` 或 `networkConnected` 判断目录成员：同账号主机可能保存过识别码，合并后 `source` 为 code。`Runtime::refresh` 每次清除旧 `account_online`，仅对新 `pclist` 在线行的合并结果标记 true；刷新失败/未登录清除标记，`snapshot` 还核对登录状态。`Device.account_online` 跳过反序列化，磁盘旧在线状态不会在启动时生效。

`/history` 由 `HistoryPage.vue` 按时间解析值倒序显示现有历史，列表卡片显示名称、连接方式、识别码和时间，不显示主机状态。无详细历史条目的旧识别码/已有连接资料在内存中补充展示，不创建重复历史条目；没有连接时间时标记「时间未记录」并排在末尾，不使用导入时间代替连接时间。详情 `/history/record/:id` 按历史 ID 查记录，再按其当前 deviceId 查资料，保持合并后引用有效；旧资料详情为 `/history/device/:id`。历史引用的设备已删除时不在列表显示；直接打开旧详情链接仍可返回，不猜测或重新创建设备。旧 `/history/:deviceId` 重定向为全局列表的设备筛选。

`DevicePage.vue` 复用原桌面、设置、快速启动布局，记录详情隐藏主机状态，返回时保留列表搜索/筛选。星标收藏直接使用原受限 `edit_device` 保存，仅切换 favorite，保留名字、备注和壁纸；收藏成功变黄并提示，取消可撤销。编辑弹窗收藏项移除。`App.vue` 对历史详情解析实际设备 ID，保证画质/输入设置仍作用于该设备。运行测试由用户完成，不能将编译或资源校验写作列表、收藏或原回归已经实测通过。

0.1.14 原候选用户验收反馈失败：旧 Data 内 Sunshine 六个绝对路径指向 0.1.12，验证账号会创建旧目录；全屏连接看不到悬浮球，本机日志记录性能窗口和控制窗口创建失败，但旧日志没有底层错误。修正版 `SunshineAdapter::new` 在非外部服务模式下先执行 `rebase_managed_config`，仅更新上述 LL 管理路径到当前 Data，再解析账号/证书位置；不删除或移动账号、配对，不改外部 Sunshine 服务配置。现有旧目录可能含用户资料，不能擅自清理。

修正版 `controls.rs` 的会话窗口继承主窗口配置，通过 `with_webview` 在界面线程复用主窗口已有 WebView2 环境创建，保留实际 Tauri 错误。串流成功先恢复 Moonlight 前台并创建/定位悬浮球，随后按开关创建性能窗口；性能监测默认 false，关闭时不创建，开启时由命令延迟创建。悬浮窗口等最终版本已有用户当前测试无问题反馈，未提供全屏叠加的逐项结果，也未证明旧失败的底层原因。用户追加要求新连接默认窗口：`connect_attempt` 仅对新连接启动参数设置 `windowed`，不修改 `update_session` 的内部重连参数，因此会话内主动选全屏后调画质保持全屏。见同版本修正记录，未运行单测或应用/内核测试。

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
| IP 直连 | `src/pages/DirectPage.vue`、`src-tauri/src/runtime/direct.rs` | `runtime.rs` 的公共串流阶段、`ConfigureSunshineDirect.ps1`、历史和收藏重连 |
| 官网签到 | `src-tauri/src/signin.rs` | `signin_status.rs`、`signin-login.js`、`signin-submit.js`、`src/stores/app.ts` |
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

开发版的顶部栏、账号页和服务到期提示共用 `open_signin`。仅在用户点击后使用与当前已登录皎月连账号相符的凭据，通过官网表单的 HTTPS 端点登录。独立的无痕签到窗口没有应用命令权限，官网签到页加载后触发一次官方按钮，由官网处理滑块和显示结果。自动登录失败或凭据不可用时可以在同一窗口手动登录；LL 账号退出或重新登录后关闭原签到窗口。前端生产构建、Windows 调试构建及本地脚本模拟检查通过，真实账号签到待验收。

用户随后确认一键签到功能正常。2026-11-27 起由用户打开的官方签到窗口读取「服务到期时间」和「下次可签到时间」，按北京时间解析并在个人 Data 按账号保存 `signin.json`。`get_signin_status` 读取本机记录，启动和账号切换不发起官网登录。面板用到期时间减系统时间显示剩余服务时长；没有日期时按到期处理，黄色按钮可进入官网读取。已读取的服务到期时间提前两小时触发黄色提醒。成功续期后按官网页面更新本机日期。

同日修正「我的设备」与在线设备详情偶尔显示旧昵称或 Sunshine 名称的问题。目录刷新后显示 `account_host_name`，并在其下方单独显示备注。备注随已保存的设备身份保留，主机名或目录 ID 变化时继续按原有身份匹配规则衔接。历史、收藏及设备编辑器保留本机 `display_name`。主机名与目录 ID 同时变化且 Sunshine 身份未确认时，保留原设备备注，待身份确认后衔接。

0.1.16 包含 2026-11-26 加入的公网和局域网 IP 直连。直连会话通过 `connect_direct` 启动，仅对指定 IP 探测 Sunshine，随后复用配对与串流。取消、失败、会话结束和断开时跳过皎月连组网清理。直连资料按当前设备资料范围保存，使用独立的设备身份匹配，避免覆盖已有组网地址。被控端直连与皎月连接受控制需先关闭当前模式再切换。直连防火墙脚本只管理本功能创建的 Sunshine 规则，关闭和退出按标记清理。局域网、公网与原皎月连流程的运行验收由用户执行。[用法与边界](连接流程与组网地址.md#ip-直连)

## 3. 数据和运行模式

| 模式 | 数据位置 | 如何识别 |
| --- | --- | --- |
| 便携版 | EXE 旁的 `Data/` | `portable.flag` 或 `--portable` |
| 非便携版 | `%LOCALAPPDATA%/LLRemote` | 没有便携标记时按当前用户目录保存 |
| Debug 隔离测试 | `LLREMOTE_TEST_DATA` 指定的位置 | 只在 Debug 构建生效，Release 不能依赖它隔离数据 |
| WebView2 | 当前应用数据目录内的 `WebView2/` | 主程序设置浏览器缓存位置，使用系统共享运行时 |

便携 Data 内还包含各内核配置、设备/历史数据、日志、壁纸和加密凭据。具体文件路径以 `runtime.rs`、`storage.rs` 和运行日志为准。账号密码与历史连接密码由 Windows DPAPI 保存，普通 JSON 保存凭据引用；更换电脑或 Windows 用户后通常需要重新认证。

在读日志或制作报告时，不输出密码、PIN、令牌、私钥和完整账号配置。清理、升级或打包时保留用户 Data、根目录 `config` 和已有配对；程序/ZIP 的临时备份按 11-25 新规则在交付完成后删除。故障日志应保留数值错误码和时间线，避免把中文显示乱码误当成数值证据。

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

日常运行入口为 `release/LLRemote-Portable-x64-0.1.14/LLRemote.exe`，11-25 壁纸修正的 EXE 和 ZIP 已同步到 release；用户无需从 `.work` 启动。release 的实际使用目录包含个人 Data，所以干净打包脚本拒绝直接覆盖该输出目录；这项检查只约束打包，不限制运行程序。用户 11-25 新规定：release 以外的临时交付目录、ZIP 和程序备份在编译/包校验完成并同步 release 后删除，GitHub 上传材料在上传后删除，不等待功能验收、不长期保留旧副本。旧目录不再写入，现状与本轮清理见 [目录与备份盘点](0.1.14目录与备份盘点.md)。

已保留内核和宿主、只改 LL 代码时，可执行：

```powershell
node scripts/tauri.mjs build --no-bundle --config src-tauri/tauri.release.conf.json
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package.ps1 -Portable -OutputRoot .work/package-current
python scripts/verify-bundle.py .work/package-current/LLRemote-Portable-x64-0.1.14 --reference . --require-clean-data
```

示例校验路径对应当前版本；升级后换成新版本目录。`package.ps1` 从 `package.json` 读取版本，直接使用 `src-tauri/target/release/LLRemote.exe` 等资源，要求扩展清单和 UTF-8 Sunshine 标记正确。先构建再打包，避免把旧 EXE 当作新改动交付。

同版本本机目录已经运行过时，使用 `-OutputRoot .work/package-current` 生成独立干净目录；不要复用早期任务目录。先校验并把最终 EXE/ZIP/校验文件同步回 release，保存验证及发布回执到当前 docs，然后运行 `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/clean-delivery.ps1 -TemporaryPaths .work/package-current -Execute` 删除临时输出。需要 GitHub 上传时，在上传确认成功后执行该清理。最终包与源码提交对应；功能测试来自用户确认与定向文件回归，不能扩展为跨电脑或干净系统全部通过。

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
请接手当前工作区的 LL远程项目。当前源码为 0.1.15，用户已确认缩略图定位修正版功能测试通过并授权发布、整理本机 release 版本号。最新构建、包校验、个人 Data 保留、发布与清理结果见 docs/validation/0.1.15.json。0.1.15 沿用验收功能代码，仅更新版本配置并重新构建；不补写未提供的逐项测试结果。此前图片覆盖整行的问题已通过列表内定位修正解决。0.1.13、0.1.14 发布和验收记录作为历史保留。
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
