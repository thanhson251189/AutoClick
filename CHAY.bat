@echo off
chcp 65001 >nul
cd /d "%~dp0"

if exist "target\release\automatic-mouse-keyboard.exe" (
  "target\release\automatic-mouse-keyboard.exe"
  goto :eof
)

where cargo >nul 2>&1
if %errorlevel%==0 (
  cargo run --release
  goto :eof
)

echo Chua co file .exe va chua cai Rust.
echo 1. Cai rustup: https://rustup.rs/
echo 2. Cai Build Tools C++ (workload Desktop C++)
echo 3. Double-click BUILD-RUST.bat
pause
