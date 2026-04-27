@echo off
echo ===========================================
echo       Dictando Builder for Windows
echo ===========================================
echo.
echo Step 1: Building React Web Application...
cd ..
call npm install
call npm run build
cd windows_app

echo.
echo Step 2: Installing required Python packages...
python -m pip install keyboard pyaudio pyperclip google-genai pywebview pyinstaller

echo.
echo Step 3: Building the Dictando application without a console window...
python -m PyInstaller --noconsole --onefile --add-data "../dist;dist" dictando.py

echo.
echo ===========================================
echo Build complete! 
echo Your native application is inside the "windows_app\dist" folder.
echo You can move "dictando.exe" anywhere you want.
echo ===========================================
pause
