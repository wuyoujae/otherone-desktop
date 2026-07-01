@echo off
setlocal

set "PROJECT_ROOT=%~dp0.."
set "CARGO_TARGET_DIR=%PROJECT_ROOT%\.tauri-target"

if not exist "%CARGO_TARGET_DIR%" mkdir "%CARGO_TARGET_DIR%"

set "VSDEVCMD="

if defined VSDEVCMD_PATH (
  if exist "%VSDEVCMD_PATH%" set "VSDEVCMD=%VSDEVCMD_PATH%"
)

if not defined VSDEVCMD (
  if exist "D:\Microsoft\VisualStudioIDE\Common7\Tools\VsDevCmd.bat" set "VSDEVCMD=D:\Microsoft\VisualStudioIDE\Common7\Tools\VsDevCmd.bat"
)

if not defined VSDEVCMD (
  if exist "%ProgramFiles%\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat" set "VSDEVCMD=%ProgramFiles%\Microsoft Visual Studio\2022\Community\Common7\Tools\VsDevCmd.bat"
)

if not defined VSDEVCMD (
  if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" set "VSDEVCMD=%ProgramFiles(x86)%\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat"
)

if not defined VSDEVCMD (
  if exist "%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" (
    for /f "usebackq delims=" %%I in (`"%ProgramFiles(x86)%\Microsoft Visual Studio\Installer\vswhere.exe" -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath`) do set "VSROOT=%%I"
    if defined VSROOT (
      if exist "%VSROOT%\Common7\Tools\VsDevCmd.bat" set "VSDEVCMD=%VSROOT%\Common7\Tools\VsDevCmd.bat"
    )
  )
)

if defined VSDEVCMD (
  call "%VSDEVCMD%" -arch=x64 -host_arch=x64 >nul
) else (
  echo Warning: Visual Studio C++ build environment was not found. Cargo may fail to find link.exe.
)

call "%PROJECT_ROOT%\node_modules\.bin\tauri.cmd" %*
exit /b %ERRORLEVEL%
