@echo off
REM Smoke test rapido em laboratorio (somente leitura)
setlocal
chcp 65001 >nul
set JAR=%~dp0guialar-digital.jar
if not exist "%JAR%" set JAR=%~dp0..\build\jar\guialar-digital.jar
java -Dfile.encoding=UTF-8 -jar "%JAR%" --cli --diagnostico
echo.
echo Se a saida mostrou o diagnostico sem crash, o smoke test basico passou.
pause
endlocal
