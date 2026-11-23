# 系统 WebView2 与内核依赖

LL远程使用系统安装的 Microsoft Edge WebView2 Evergreen Runtime。程序目录不附带浏览器运行时；启动时检查系统运行时，缺失时显示微软官方安装入口。

`prepare-runtime.ps1` 只准备 `VC/x64` 中的 MSVC 库，供三个内核和主程序打包使用。构建和打包不再依赖 `src-tauri/runtimes/WebView2`。

便携版的 `Data/WebView2` 是用户缓存，随 Data 保留；它与已经移除的程序目录 `runtimes/WebView2` 浏览器运行时不同。
