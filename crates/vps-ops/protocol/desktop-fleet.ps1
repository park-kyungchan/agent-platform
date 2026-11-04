$env:HERMES_HOME='@@nativeHomeWindows@@'; $env:HERMES_INSTALL_ROOT='@@installRootWindows@@'; $env:PYTHONDONTWRITEBYTECODE='1';
$launcher='@@installRootWindows@@\.hermes\bin\hermes.exe';
$runtime=(((& $launcher --print-runtime-command --module hermes_cli.update_inventory) -join "
")|ConvertFrom-Json);
if($LASTEXITCODE -ne 0 -or $runtime.Count -lt 4 -or $runtime[2] -cne '-c'){throw 'Selected native runtime command is unsupported'};
$runtime[3]+='; import json; from hermes_cli.update_inventory import collect_runtime_inventory; p=collect_runtime_inventory().to_dict(); print(json.dumps({**{k:p.get(k) for k in (''install_method'',''updatable_in_place'',''expected_sha'',''profiles'')}, ''runtimes'':[{k:r.get(k) for k in (''kind'',''profile'',''pid'',''supervisor'',''code_sha'',''restart_via'')} for r in p.get(''runtimes'',[])]}))';
$runtimeArgs=@($runtime|Select-Object -Skip 1); & $runtime[0] @runtimeArgs; if($LASTEXITCODE -ne 0){throw 'Native inventory failed'}
