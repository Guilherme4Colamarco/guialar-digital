@echo off
REM Smoke test rapido em laboratorio (somente leitura)
setlocal
set JAR=%~dp0guialar-digital.jar
if not exist "%JAR%" set JAR=%~dp0..\build\jar\guialar-digital.jar
java -jar "%JAR%" --cli --diagnostico
echo.
echo Se a saida mostrou o diagnostico sem crash, o smoke test basico passou.
pause
endlocal
