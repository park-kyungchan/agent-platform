$userHome=[Environment]::GetFolderPath('UserProfile'); $applicationHome=if($env:HERMES_HOME){$env:HERMES_HOME}else{Join-Path $env:LOCALAPPDATA 'hermes'};
$git='@@nativeGitExecutableWindows@@'; $root='@@installRootWindows@@';
$env:GIT_OPTIONAL_LOCKS='0'; $env:GIT_NO_LAZY_FETCH='1'; $env:GIT_TERMINAL_PROMPT='0';
$head=(& $git --no-optional-locks -C $root rev-parse HEAD) -join ''; if($LASTEXITCODE -ne 0){throw 'Native Git HEAD probe failed'};
& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root diff --no-ext-diff --no-textconv --no-renames --quiet HEAD --;
$dirtyCode=$LASTEXITCODE; if($dirtyCode -notin @(0,1)){throw 'Native Git dirty probe could not establish a result'};
$untrackedPaths=@(& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root ls-files --others --exclude-standard); if($LASTEXITCODE -ne 0){throw 'Native Git untracked paths failed'};
$checkoutDirty=($dirtyCode -eq 1 -or $untrackedPaths.Count -gt 0);
$diffIdentity=$null;
if($checkoutDirty){
  $diffProcess=New-Object System.Diagnostics.Process; $diffProcess.StartInfo.FileName=$git;
  $diffProcess.StartInfo.Arguments='--no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C "'+$root+'" diff --binary --full-index --no-ext-diff --no-textconv --no-renames HEAD --';
  $diffProcess.StartInfo.UseShellExecute=$false; $diffProcess.StartInfo.CreateNoWindow=$true; $diffProcess.StartInfo.RedirectStandardOutput=$true; $diffProcess.StartInfo.RedirectStandardError=$true;
  if(-not $diffProcess.Start()){throw 'Native Git diff probe failed'};
  $diffErrors=$diffProcess.StandardError.ReadToEndAsync(); $diffBytes=New-Object System.IO.MemoryStream;
  $diffProcess.StandardOutput.BaseStream.CopyTo($diffBytes); $diffProcess.WaitForExit();
  if($diffProcess.ExitCode -ne 0 -or $diffBytes.Length -gt 2097152){throw 'Native Git diff identity could not be bounded'};
  $digest=[System.Security.Cryptography.SHA256]::Create(); $diffHash=([System.BitConverter]::ToString($digest.ComputeHash($diffBytes.ToArray()))).Replace('-','').ToLowerInvariant(); $digest.Dispose(); $diffBytes.Dispose(); $diffProcess.Dispose();
  $diffPaths=@(& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root diff --name-only --no-ext-diff --no-textconv --no-renames HEAD --); if($LASTEXITCODE -ne 0){throw 'Native Git diff paths failed'};
  $diffIdentity=@{sha256=$diffHash;paths=@($diffPaths|Sort-Object -Unique);untrackedPaths=@($untrackedPaths|Sort-Object -Unique)};
};
$artifactRoot='@@installRootWindows@@\apps\desktop\release\';
$owned=@(Get-CimInstance Win32_Process -Filter "Name = 'Hermes.exe'" | Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith($artifactRoot,[StringComparison]::OrdinalIgnoreCase) });
$ownedIds=@($owned|ForEach-Object{[int]$_.ProcessId});
$rows=@($owned | Where-Object { $ownedIds -notcontains [int]$_.ParentProcessId } | Sort-Object ProcessId | ForEach-Object { @{pid=[int]$_.ProcessId; executablePath=$_.ExecutablePath; createdAtUtc=$_.CreationDate.ToUniversalTime().ToString('o')} });
@{nativeUserHomeWindows=$userHome;nativeHomeWindows=$applicationHome;processes=$rows;checkoutSha=$head;checkoutDirty=$checkoutDirty;checkoutDiff=$diffIdentity}|ConvertTo-Json -Depth 4 -Compress
