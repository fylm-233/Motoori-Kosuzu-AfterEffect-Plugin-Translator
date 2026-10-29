param([string]$PanelDirectory='',[switch]$Quiet)
$ErrorActionPreference='Stop'
$version='0.8.3-test'
function Is-Admin {([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)}
if(-not(Is-Admin)){
 try{$engine=Join-Path $PSHOME 'powershell.exe';if($PSVersionTable.PSEdition -eq 'Core'){$engine=Join-Path $PSHOME 'pwsh.exe'}
 $arguments=@('-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$PSCommandPath+'"'));if($PanelDirectory){$arguments+=@('-PanelDirectory',('"'+$PanelDirectory+'"'))};if($Quiet){$arguments+='-Quiet'}
 $child=Start-Process -FilePath $engine -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList $arguments
 if($child.ExitCode -ne 0){throw "安装进程错误码 $($child.ExitCode)，请查看 %LOCALAPPDATA%\Kosuzu\install.log"};exit 0
 }catch{Write-Error ('管理员授权取消或安装失败：'+$_.Exception.Message);exit 1}
}
$productHome=Join-Path $env:LOCALAPPDATA 'Kosuzu'
New-Item -ItemType Directory -Path $productHome -Force|Out-Null
$log=Join-Path $productHome 'install.log'
function Note([string]$text){Add-Content -LiteralPath $log -Value ((Get-Date -Format s)+' '+$text) -Encoding UTF8;Write-Output $text}
function Assert-Child([string]$path,[string]$parent){$full=[IO.Path]::GetFullPath($path);$prefix=[IO.Path]::GetFullPath($parent).TrimEnd('\')+'\';if(-not $full.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){throw "路径超出本居小铃安装目录：$full"}}
try{
 if(Get-Process AfterFX -ErrorAction SilentlyContinue){throw '请先保存工程并关闭 AE，再运行安装。安装不会强制关闭 AE。'}
 if(-not $PanelDirectory){
  $candidates=@();$adobe=Join-Path $env:ProgramFiles 'Adobe';if(Test-Path -LiteralPath $adobe){$candidates=@(Get-ChildItem -LiteralPath $adobe -Directory -Filter 'Adobe After Effects *'|Sort-Object Name -Descending|ForEach-Object{Join-Path $_.FullName 'Support Files\Scripts\ScriptUI Panels'}|Where-Object{Test-Path -LiteralPath $_ -PathType Container})}
  if(-not $candidates.Count){throw '未找到 AE。请用 -PanelDirectory 指定 ScriptUI Panels 目录。'}
  $PanelDirectory=$candidates[0]
 }
 if(-not(Test-Path -LiteralPath $PanelDirectory -PathType Container)){throw '指定的 ScriptUI Panels 目录不存在。'}
 $installRoot=Join-Path $productHome 'app';$stage=Join-Path $productHome ('staging-'+[Guid]::NewGuid().ToString('N'));$backup=Join-Path $productHome ('backups\'+(Get-Date -Format yyyyMMddHHmmssfff))
 Assert-Child $stage $productHome;Assert-Child $installRoot $productHome;Assert-Child $backup $productHome
 New-Item -ItemType Directory -Path $stage,$backup -Force|Out-Null
 foreach($part in @('backend','dictionaries')){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $part) -Destination (Join-Path $stage $part) -Recurse}
 foreach($name in @('Kosuzu.jsx','environment.json','README.md','Uninstall.ps1')){Copy-Item -LiteralPath (Join-Path $PSScriptRoot $name) -Destination (Join-Path $stage $name)}
 $target=Join-Path $PanelDirectory 'Kosuzu.jsx';$legacyRoot=$null;$hadPanel=Test-Path -LiteralPath $target
 if($hadPanel){$old=Get-Content -LiteralPath $target -Raw -Encoding UTF8;if($old -notmatch 'LBOpenDictionaryManager|KOSUZU_PACKAGE_ROOT'){throw '目标面板同名但不能确认为本居小铃，停止覆盖。'}
 Copy-Item -LiteralPath $target -Destination (Join-Path $backup 'Kosuzu.jsx');if($old -match 'var KOSUZU_PACKAGE_ROOT=("(?:[^"\\]|\\.)*");'){try{$legacyRoot=$Matches[1]|ConvertFrom-Json}catch{}}
 Note ('检测到旧版本居小铃面板，已备份：'+$backup)
 }
 $oldPanel=Join-Path $PanelDirectory 'LinguaBridge.jsx';$oldPanelMoved=$false
 if(Test-Path -LiteralPath $oldPanel){$previous=Get-Content -LiteralPath $oldPanel -Raw -Encoding UTF8;if($previous -notmatch 'LBOpenDictionaryManager|LINGUABRIDGE_PACKAGE_ROOT'){throw '旧同名面板无法确认为译桥，停止迁移。'};Copy-Item -LiteralPath $oldPanel -Destination (Join-Path $backup 'LinguaBridge.jsx');if(-not $legacyRoot -and $previous -match 'var LINGUABRIDGE_PACKAGE_ROOT=("(?:[^"\\]|\\.)*");'){$legacyRoot=$Matches[1]|ConvertFrom-Json}}
 foreach($kind in @('names','parameters')){
  $filename='Kosuzu.'+$kind+'.json';$preserved=$null
  foreach($existing in @((Join-Path $installRoot 'dictionaries'),$(if($legacyRoot){Join-Path $legacyRoot 'dictionaries'}))){if($existing -and (Test-Path -LiteralPath (Join-Path $existing $filename))){$preserved=Join-Path $existing $filename;break}}
  if(-not $preserved){foreach($oldRoot in @($legacyRoot,(Join-Path $env:LOCALAPPDATA 'LinguaBridge\app'))){if($oldRoot){$candidate=Join-Path $oldRoot ('dictionaries\LinguaBridge.'+$kind+'.json');if(Test-Path -LiteralPath $candidate){$preserved=$candidate;break}}}}
  if($preserved){& (Join-Path $stage 'backend\KosuzuDictionaryCheck.exe') $preserved;if($LASTEXITCODE -ne 0){throw '已有自定义词典损坏，停止更新并保留原文件。'};Copy-Item -LiteralPath $preserved -Destination (Join-Path $stage ('dictionaries\'+$filename));Note ('保留用户词典：'+$preserved)}
 }
 foreach($binary in @('KosuzuNative.dll','KosuzuController.exe','KosuzuClipboard.exe')){if(-not(Test-Path -LiteralPath (Join-Path $stage ('backend\'+$binary)))){throw '安装包缺少原生组件：'+$binary}}
 $source=Get-Content -LiteralPath (Join-Path $stage 'Kosuzu.jsx') -Raw -Encoding UTF8
 $source=[regex]::Replace($source,'(?m)^#include[^\r\n]*\r?\n','')
 $code='(function(){'+[Environment]::NewLine+'var KOSUZU_PACKAGE_ROOT='+($installRoot|ConvertTo-Json -Compress)+';'+[Environment]::NewLine
 $code+=(Get-Content -LiteralPath (Join-Path $stage 'backend\json.jsxinc') -Raw -Encoding UTF8)+[Environment]::NewLine
 $code+=(Get-Content -LiteralPath (Join-Path $stage 'backend\dictionary-manager.jsxinc') -Raw -Encoding UTF8)+[Environment]::NewLine+$source+[Environment]::NewLine+'}).call(this);'
 $hadApp=Test-Path -LiteralPath $installRoot
 try{
  if($hadApp){Assert-Child $installRoot $productHome;Move-Item -LiteralPath $installRoot -Destination (Join-Path $backup 'app')}
  Move-Item -LiteralPath $stage -Destination $installRoot
  [IO.File]::WriteAllText($target,$code,[Text.UTF8Encoding]::new($false))
  if(Test-Path -LiteralPath $oldPanel){Remove-Item -LiteralPath $oldPanel;$oldPanelMoved=$true}
  @{product='Kosuzu';version=$version;root=$installRoot;panel=$target;backup=$backup}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $productHome 'installed.json') -Encoding UTF8
 }catch{
  if($oldPanelMoved){Copy-Item -LiteralPath (Join-Path $backup 'LinguaBridge.jsx') -Destination $oldPanel}
  if(Test-Path -LiteralPath $installRoot){Assert-Child $installRoot $productHome;Move-Item -LiteralPath $installRoot -Destination (Join-Path $backup 'failed-app')}
  if($hadApp -and (Test-Path -LiteralPath (Join-Path $backup 'app'))){Move-Item -LiteralPath (Join-Path $backup 'app') -Destination $installRoot}
  if($hadPanel){Copy-Item -LiteralPath (Join-Path $backup 'Kosuzu.jsx') -Destination $target}else{if(Test-Path -LiteralPath $target){Remove-Item -LiteralPath $target}}
  throw
 }
 Note ('安装成功：'+$target+'；离线组件：'+$installRoot)
 if(-not $Quiet){Add-Type -AssemblyName System.Windows.Forms
 [Windows.Forms.MessageBox]::Show('本居小铃 0.8 离线原生测试版安装完成。启动 AE，在“窗口”菜单打开 Kosuzu.jsx。无需手动启动后端。','本居小铃 Kosuzu')|Out-Null}
 exit 0
}catch{Note ('安装失败：'+$_.Exception.Message);if(-not $Quiet){Add-Type -AssemblyName System.Windows.Forms;[Windows.Forms.MessageBox]::Show($_.Exception.Message,'本居小铃安装失败')|Out-Null};exit 1}
