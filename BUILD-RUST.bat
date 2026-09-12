@echo off
chcp 65001 >nul
cd /d "%~dp0"

where cargo >nul 2>&1
if not %errorlevel%==0 (
  echo Chua cai Rust. Tai rustup-init.exe tai https://rustup.rs/
  echo Windows con can "Build Tools for Visual Studio" (workload Desktop C++).
  pause
  exit /b 1
)

cargo build --release
if %errorlevel%==0 (
  echo.
  echo OK. File chay:
  echo   %cd%\target\release\automatic-mouse-keyboard.exe
  echo Headless:
  echo   automatic-mouse-keyboard.exe --run script.amk
) else (
  echo.
  echo BUILD LOI.
  echo - Cai Build Tools C++: https://aka.ms/vs/17/release/vs_BuildTools.exe
  echo   chon workload "Desktop development with C++"
)
pause
