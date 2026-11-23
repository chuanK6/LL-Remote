# 第三方组件说明

| 组件 | 固定版本 | 许可与源码 |
| --- | --- | --- |
| Moonlight | v6.2.0 + LL 管理扩展 | GPLv3；`Moonlight-GPLv3.txt`，随包 `sources/Moonlight-v6.2.0-LLRemote.zip` |
| Sunshine | v2026.914.233613 | GPLv3；`Sunshine-GPLv3.txt`，随包 `sources/Sunshine-v2026.914.233613.zip` |
| 皎月连 | natpierce-win-v1.06 | 专有软件；官网 https://www.natpierce.cn/ ，本机开发产物不得被视为供应商的公开再分发授权 |
| Qt | 6.12.0 | Moonlight 原 Qt 内核，GPL/LGPL；保留 SDK 随附许可，源码 https://download.qt.io/archive/qt/6.12/6.12.0/ |
| Microsoft WebView2 | Fixed Runtime 157.0.4341.60 x64 | 微软 WebView2 分发条款；下载来源 https://developer.microsoft.com/en-us/microsoft-edge/webview2 |
| Microsoft Visual C++ Runtime | 本机 MSVC 14.52 x64 | Visual Studio 随附 app-local 可再分发运行库；随包提供 DLL，不注册独立启动入口 |

Moonlight 的少量修改仅涉及管理入口、会话 IPC、原输入处理器、音量及指标输出。对应修改文件和构建脚本包含在源码包中；原串流协议、编解码及设置界面沿用上游。

上游依赖构建脚本与版本来源：https://github.com/moonlight-stream/moonlight-qt-deps/tree/v19 。FFmpeg、SDL、Opus、OpenSSL、libplacebo 等仍受各自许可约束。本轮交付为本机开发验证产物；公开发布前，需核对专有内核的再分发授权及全部依赖的对应源码提供要求。
