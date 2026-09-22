@echo off
REM GuiaLar Digital - launcher Windows (sem admin para diagnóstico)
setlocal
set JAR=%~dp0guialar-digital.jar
if not exist "%JAR%" set JAR=%~dp0..\build\jar\guialar-digital.jar

where java >nul 2>&1
if errorlevel 1 (
  echo Java nao encontrado. Instale JDK 17+ ou use o Java portatil da universidade.
  echo Exemplo: java -jar guialar-digital.jar --diagnostico
  pause
  exit /b 1
)

if "%~1"=="" (
  java -jar "%JAR%" --gui
) else (
  java -jar "%JAR%" %*
)
endlocal
