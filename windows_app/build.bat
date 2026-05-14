@echo off
echo ===========================================
echo        Dictando Builder for Windows
echo ===========================================
echo.

echo Step 1: Building React web application...
cd ..
call npm install
call npm run build
cd windows_app

echo.
echo Step 2: Installing Python dependencies...
python -m pip install --upgrade pip
python -m pip install ^
    keyboard ^
    pyaudio ^
    pyperclip ^
    google-genai ^
    pywebview ^
    faster-whisper ^
    numpy ^
    pystray ^
    Pillow ^
    pyinstaller

echo.
echo Step 3: Building Dictando.exe...
python -m PyInstaller dictando_v4.spec --clean --noconfirm

echo.
echo ===========================================
echo  Build complete!
echo  Output: windows_app\dist\Dictando.exe
echo ===========================================
pause
