$ErrorActionPreference='Stop'
if(-not([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)){
 $engine=Join-Path $PSHOME 'powershell.exe';if($PSVersionTable.PSEdition -eq 'Core'){$engine=Join-Path $PSHOME 'pwsh.exe'}
 $child=Start-Process -FilePath $engine -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList @('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$PSCommandPath+'"'));exit $child.ExitCode
}
if(Get-Process AfterFX -ErrorAction SilentlyContinue){throw '请保存工程并关闭 AE，再卸载。'}
$homeDirectory=Join-Path $env:LOCALAPPDATA 'Kosuzu';$manifestPath=Join-Path $homeDirectory 'installed.json'
$manifest=Get-Content -LiteralPath $manifestPath -Raw -Encoding UTF8|ConvertFrom-Json
if($manifest.product -ne 'Kosuzu'){throw '安装记录不属于本居小铃，停止操作。'}
$panel=[IO.Path]::GetFullPath($manifest.panel)
if([IO.Path]::GetFileName($panel) -ne 'Kosuzu.jsx'){throw '面板路径异常，停止操作。'}
if(Test-Path -LiteralPath $panel){$source=Get-Content -LiteralPath $panel -Raw -Encoding UTF8;if($source -notmatch 'LBOpenDictionaryManager|KOSUZU_PACKAGE_ROOT'){throw '同名面板不是本居小铃，保留原文件。'}
 $backup=Join-Path $homeDirectory ('backups\uninstall-'+(Get-Date -Format yyyyMMddHHmmssfff));New-Item -ItemType Directory -Path $backup -Force|Out-Null;Move-Item -LiteralPath $panel -Destination (Join-Path $backup 'Kosuzu.jsx')
}
Write-Output '本居小铃面板已移除，应用文件、自定义词典与备份均保留在 %LOCALAPPDATA%\Kosuzu。'
