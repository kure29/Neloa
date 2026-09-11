@echo off
chcp 65001 >nul
setlocal EnableExtensions
cd /d "%~dp0"

echo.
echo ========================================
echo       Neloa Windows 安装包构建器
echo ========================================
echo.

where node >nul 2>&1
if errorlevel 1 goto missing_node
where npm >nul 2>&1
if errorlevel 1 goto missing_node
where rustc >nul 2>&1
if errorlevel 1 goto missing_rust
where cargo >nul 2>&1
if errorlevel 1 goto missing_rust

echo [1/3] 安装前端依赖...
call npm ci
if errorlevel 1 goto build_failed

echo.
echo [2/3] 编译 Neloa 和 NSIS 安装程序...
call npm run tauri build -- --bundles nsis
if errorlevel 1 goto build_failed

echo.
echo [3/3] 构建完成。
echo 安装程序位于：
echo   src-tauri\target\release\bundle\nsis\
echo.
for /r "src-tauri\target\release\bundle\nsis" %%F in (*.exe) do echo   %%~fF
echo.
echo 可以只保留上面的 .exe，node_modules 和 src-tauri\target 均可删除。
pause
exit /b 0

:missing_node
echo [缺少环境] 请安装 Node.js 20 或更高版本，然后重新运行本文件。
echo https://nodejs.org/
pause
exit /b 1

:missing_rust
echo [缺少环境] 请安装 Rust MSVC 工具链，然后重新运行本文件。
echo https://rustup.rs/
pause
exit /b 1

:build_failed
echo.
echo 构建失败。请查看上方第一条红色错误，并对照 WINDOWS_BUILD.md。
pause
exit /b 1
