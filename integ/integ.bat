setlocal EnableDelayedExpansion

cd %~dp0

if exist package.json (
    del /q package.json
)

call npm init --yes
call npm link ../core ../cli

set AILLY_ENGINE=noop

echo basic
call npx ailly --root 01_basic
if not exist "01_basic/basic.txt.ailly.md" goto :error
del "01_basic/basic.txt.ailly.md"

echo combined
call npx ailly --root 02_combined --combined
if exist "02_combined/combined.txt.ailly.md" goto :error

goto :end

:error
echo An error occurred.
exit /b 1

:end
endlocal