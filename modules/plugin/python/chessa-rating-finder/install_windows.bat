@echo off
cd /d "%~dp0"

where py >nul 2>nul
if not errorlevel 1 (
    set "PY=py -3"
) else (
    where python >nul 2>nul || (echo Python 3.10+ is required. & exit /b 1)
    set "PY=python"
)

%PY% -c "import sys; exit(sys.version_info < (3,10))"
if errorlevel 1 (
    echo Python 3.10 or newer is required.
    exit /b 1
)

if not exist ".venv\Scripts\python.exe" (
    %PY% -m venv .venv || exit /b 1
)

".venv\Scripts\python.exe" -m pip install -r requirements.txt || exit /b 1

cls
".venv\Scripts\python.exe" chessa_rating_tui.py
exit /b %errorlevel%
