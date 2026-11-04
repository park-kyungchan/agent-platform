$env:HERMES_HOME='@@nativeHomeWindows@@'; $env:HERMES_INSTALL_ROOT='@@installRootWindows@@'; $env:PYTHONDONTWRITEBYTECODE='1'; $env:HERMES_RUNTIME_DIR=$null;
$launcher='@@installRootWindows@@\.hermes\bin\hermes.exe';
$runtime=(((& $launcher --print-runtime-command --module hermes_cli.source_releases) -join "
")|ConvertFrom-Json);
if($LASTEXITCODE -ne 0 -or $runtime.Count -lt 4 -or $runtime[2] -cne '-c'){throw 'Selected native runtime command is unsupported'};
$runtime[3]+='@@append@@';
$runtimeArgs=@($runtime|Select-Object -Skip 1); & $runtime[0] @runtimeArgs; if($LASTEXITCODE -ne 0){throw 'Native stable target resolution failed'}
