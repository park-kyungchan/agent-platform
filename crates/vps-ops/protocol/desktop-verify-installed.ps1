$env:HERMES_HOME='@@nativeHomeWindows@@'; $env:HERMES_INSTALL_ROOT='@@installRootWindows@@'; $env:PYTHONDONTWRITEBYTECODE='1'; $env:HERMES_RUNTIME_DIR=$null;
& '@@installRootWindows@@\.hermes\bin\hermes.exe' --run-module hermes_cli.desktop_update_verify | Out-Null;
if($LASTEXITCODE -ne 0){throw 'Native installed artifact verification failed'}; @{verified=$true}|ConvertTo-Json -Compress
