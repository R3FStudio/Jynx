param([Parameter(Mandatory=$true)][string]$BinDir,[switch]$Remove)
$ErrorActionPreference='Stop'
$target=[IO.Path]::GetFullPath($BinDir).TrimEnd('\')
$previous=[Environment]::GetEnvironmentVariable('Path','User')
$entries=@($previous -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ine $target })
if(-not $Remove){$entries+= $target}
[Environment]::SetEnvironmentVariable('Path',($entries -join ';'),'User')
