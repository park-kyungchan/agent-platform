$git='@@nativeGitExecutableWindows@@'; $root='@@installRootWindows@@'; $stash='@@stashSha@@';
$marker='@@nativeHomeWindows@@\.hermes-update-in-progress'; if(Test-Path -LiteralPath $marker){throw 'Native update still active; stash readback refused'};
$env:GIT_OPTIONAL_LOCKS='0'; $env:GIT_NO_LAZY_FETCH='1'; $env:GIT_TERMINAL_PROMPT='0';
$base=(& $git --no-optional-locks -c protocol.allow=never -C $root rev-parse ($stash+'^1')) -join ''; if($LASTEXITCODE -ne 0){throw 'Native stash base readback failed'};
$head=(& $git --no-optional-locks -c protocol.allow=never -C $root rev-parse HEAD) -join ''; if($LASTEXITCODE -ne 0){throw 'Native stash source readback failed'};
$diffProcess=New-Object System.Diagnostics.Process; $diffProcess.StartInfo.FileName=$git;
$diffProcess.StartInfo.Arguments='--no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C "'+$root+'" diff --binary --full-index --no-ext-diff --no-textconv --no-renames '+$stash+'^1 '+$stash+' --';
$diffProcess.StartInfo.UseShellExecute=$false; $diffProcess.StartInfo.CreateNoWindow=$true; $diffProcess.StartInfo.RedirectStandardOutput=$true; $diffProcess.StartInfo.RedirectStandardError=$true;
if(-not $diffProcess.Start()){throw 'Native stash diff probe failed'};
$diffErrors=$diffProcess.StandardError.ReadToEndAsync(); $diffBytes=New-Object System.IO.MemoryStream;
$diffProcess.StandardOutput.BaseStream.CopyTo($diffBytes); $diffProcess.WaitForExit();
if($diffProcess.ExitCode -ne 0 -or $diffBytes.Length -gt 2097152){throw 'Native stash diff identity could not be bounded'};
$digest=[System.Security.Cryptography.SHA256]::Create(); $diffHash=([System.BitConverter]::ToString($digest.ComputeHash($diffBytes.ToArray()))).Replace('-','').ToLowerInvariant(); $digest.Dispose(); $diffBytes.Dispose(); $diffProcess.Dispose();
$paths=@(& $git --no-optional-locks -c core.fsmonitor=false -c protocol.allow=never -C $root diff --name-only --no-ext-diff --no-textconv --no-renames ($stash+'^1') $stash --); if($LASTEXITCODE -ne 0){throw 'Native stash paths failed'};
if(Test-Path -LiteralPath $marker){throw 'Native update started during stash readback'};
@{stashSha=$stash;baseSha=$base;headSha=$head;diffSha256=$diffHash;paths=@($paths|Sort-Object -Unique)}|ConvertTo-Json -Depth 3 -Compress
