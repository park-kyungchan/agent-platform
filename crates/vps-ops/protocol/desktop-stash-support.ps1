$env:HERMES_HOME='@@nativeHomeWindows@@'; $env:HERMES_INSTALL_ROOT='@@installRootWindows@@'; $env:PYTHONDONTWRITEBYTECODE='1'; $env:HERMES_RUNTIME_DIR=$null;
$savedPreference=$ErrorActionPreference; $ErrorActionPreference='Continue';
try {$helpText=((& '@@installRootWindows@@\.hermes\bin\hermes.exe' update --help 2>&1) -join "
"); $helpExit=$LASTEXITCODE} finally {$ErrorActionPreference=$savedPreference};
@{nativeExitCode=$helpExit;keepStash=($helpText -match '(?m)--keep-stash(?:\s|$)')}|ConvertTo-Json -Compress
