$ErrorActionPreference = "Stop"

# 兼容入口：package:win 仍走本文件，内部转发到模块化构建流程
# （阶段1 前端 -> 阶段2 cargo -> 阶段3 NSIS 打包，全量模式）。
& (Join-Path $PSScriptRoot "build\build-all.ps1") @args
