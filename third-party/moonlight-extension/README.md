# Moonlight 管理扩展

固定上游：v6.2.0，提交 `de2467e433821664cdd2224aad8c89a625be1ad9`。使用原 Qt 6 / C++ 工具链，原串流协议与编解码不作替换。

运行 `node scripts/build-moonlight.mjs` 会应用版本检查补丁、构建并将完整程序与 Qt 资源部署到 `engines/Moonlight`。构建使用 `no_batch` 与正确的清单路径转义，支持当前包含空格的工作区。

新增 `--ll-settings` 直达原 SettingsView。`LLREMOTE_MOONLIGHT_DATA_DIR` 可指定独立内核数据目录；不设置时继续使用原注册表 / 原便携配置。LL 检测到已有 Moonlight 主机时复用其现有配置和配对。

命名管道名称由 `LLREMOTE_PIPE_NAME` 指定，访问 ACL 使用 `QLocalServer::UserAccessOption`，每个请求校验随机会话令牌，Rust 再核对服务端 PID。主界面管理命令与悬浮窗口命令另由 Rust 检查窗口标签。

画质变化由 Rust 传递完整快照并短暂重连，退出本次串流不结束主机应用。鼠标及窗口变化通过 SDL 自定义事件交给原会话线程；音量作用于本次 Moonlight 进程的 Windows 音频会话。

性能数据来自原 VIDEO_STATS，约一秒发布。只有收到实际渲染帧，才向 LL 返回会话已就绪；未串流时不会产生模拟指标。独占全屏叠加、多屏拖动、输入隔离等仍需后续双端验收。
