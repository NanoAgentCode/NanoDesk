$ErrorActionPreference = "Stop"

# 兼容入口：package:win 仍走本文件，内部转发到模块化构建流程
# （前端内容指纹检查 -> Tauri 编译与 NSIS 打包，增量模式）。
& (Join-Path $PSScriptRoot "build\build-all.ps1") @args
