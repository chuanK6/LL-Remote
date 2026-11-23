# 内核资源

开发与打包脚本将三个内核的完整运行目录放在此处。不会将现有配置、配对私钥、账号密码或日志复制到发布包。

运行 `scripts/prepare-engines.ps1` 读取本机 `E:\kongping` 样本；也可指定自己的资源根目录。默认资源路径由清单固定，正式应用不依赖 `E:\kongping`。

Moonlight 扩展源码与构建说明位于 `third-party/moonlight-extension/`。`extensionProtocol` 仅在扩展版本构建与验证成功后设为 1。
