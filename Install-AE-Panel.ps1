param([string]$PanelDirectory='',[switch]$Quiet)
$ErrorActionPreference='Stop'
$version='0.8.5-test'
function Is-Admin {([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)}
if(-not(Is-Admin)){
 try{$engine=Join-Path $PSHOME 'powershell.exe';if($PSVersionTable.PSEdition -eq 'Core'){$engine=Join-Path $PSHOME 'pwsh.exe'}
 $arguments=@('-STA','-NoProfile','-ExecutionPolicy','Bypass','-File',('"'+$PSCommandPath+'"'));if($PanelDirectory){$arguments+=@('-PanelDirectory',('"'+$PanelDirectory+'"'))};if($Quiet){$arguments+='-Quiet'}
 $windowStyle=if($Quiet){'Hidden'}else{'Normal'}
 $child=Start-Process -FilePath $engine -Verb RunAs -WindowStyle $windowStyle -Wait -PassThru -ArgumentList $arguments
 if($child.ExitCode -ne 0){throw "安装进程错误码 $($child.ExitCode)，请查看 %LOCALAPPDATA%\Kosuzu\install.log"};exit 0
 }catch{Write-Error ('管理员授权取消或安装失败：'+$_.Exception.Message);exit 1}
}
$productHome=Join-Path $env:LOCALAPPDATA 'Kosuzu'
New-Item -ItemType Directory -Path $productHome -Force|Out-Null
$log=Join-Path $productHome 'install.log'
function Note([string]$text){Add-Content -LiteralPath $log -Value ((Get-Date -Format s)+' '+$text) -Encoding UTF8;Write-Output $text}
function Assert-Child([string]$path,[string]$parent){$full=[IO.Path]::GetFullPath($path);$prefix=[IO.Path]::GetFullPath($parent).TrimEnd('\')+'\';if(-not $full.StartsWith($prefix,[StringComparison]::OrdinalIgnoreCase)){throw "路径超出本居小铃安装目录：$full"}}
function Resolve-KosuzuAEFolder([string]$Folder){
 $selected=[IO.Path]::GetFullPath($Folder).TrimEnd('\')
 $supports=@((Join-Path $selected 'Support Files'),$selected)
 if([IO.Path]::GetFileName($selected) -eq 'ScriptUI Panels'){$supports+=Split-Path (Split-Path $selected -Parent) -Parent}
 foreach($support in $supports){if(Test-Path -LiteralPath (Join-Path $support 'AfterFX.exe') -PathType Leaf){return (Join-Path $support 'Scripts\ScriptUI Panels')}}
 throw '请选择 AE 安装文件夹、Support Files 或 ScriptUI Panels；未找到 AfterFX.exe。'
}
function Get-KosuzuAEInstallations([string[]]$AdobeDirectories=@('C:\Program Files\Adobe')){
 foreach($year in @(2026,2025,2024,2023)){foreach($adobe in $AdobeDirectories){$folder=Join-Path $adobe ('Adobe After Effects '+$year);try{$resolved=Resolve-KosuzuAEFolder $folder;[PSCustomObject]@{Year=$year;PanelDirectory=$resolved;Label=('After Effects '+$year+' — '+$folder)}}catch{}}}
}
function Select-KosuzuAEFolder($Candidates){
 Add-Type -AssemblyName System.Windows.Forms
 $form=New-Object Windows.Forms.Form;$form.Text='本居小铃 Kosuzu — 选择 AE';$form.ClientSize=New-Object Drawing.Size(680,200);$form.StartPosition='CenterScreen';$form.FormBorderStyle='FixedDialog';$form.MaximizeBox=$false;$form.MinimizeBox=$false
 $label=New-Object Windows.Forms.Label;$label.Text='自动检测 C 盘 AE 2023–2026；其他位置请点击“浏览目录”。';$label.SetBounds(18,15,645,30)
 $list=New-Object Windows.Forms.ComboBox;$list.DropDownStyle='DropDownList';$list.SetBounds(18,50,645,28)
 foreach($candidate in $Candidates){[void]$list.Items.Add($candidate.Label)};if($list.Items.Count){$list.SelectedIndex=0}
 $pathBox=New-Object Windows.Forms.TextBox;$pathBox.ReadOnly=$true;$pathBox.SetBounds(18,90,645,25);if($Candidates.Count){$pathBox.Text=$Candidates[0].PanelDirectory}
 $list.Add_SelectedIndexChanged({if($list.SelectedIndex -ge 0){$pathBox.Text=$Candidates[$list.SelectedIndex].PanelDirectory}})
 $browse=New-Object Windows.Forms.Button;$browse.Text='浏览目录…';$browse.SetBounds(18,140,130,32)
 $browse.Add_Click({$picker=New-Object Windows.Forms.FolderBrowserDialog;$picker.Description='选择 AE 安装目录或 Support Files 文件夹';$picker.ShowNewFolderButton=$false;try{if($picker.ShowDialog() -eq 'OK'){try{$pathBox.Text=Resolve-KosuzuAEFolder $picker.SelectedPath}catch{[Windows.Forms.MessageBox]::Show($_.Exception.Message,'目录无效')|Out-Null}}}finally{$picker.Dispose()}})
 $install=New-Object Windows.Forms.Button;$install.Text='安装到此 AE';$install.SetBounds(385,140,130,32);$install.Add_Click({if(-not $pathBox.Text){[Windows.Forms.MessageBox]::Show('请先选择 AE 目录。','Kosuzu')|Out-Null;return};$form.DialogResult='OK';$form.Close()})
 $cancel=New-Object Windows.Forms.Button;$cancel.Text='取消';$cancel.SetBounds(530,140,130,32);$cancel.DialogResult='Cancel';$form.CancelButton=$cancel;$form.AcceptButton=$install
 $form.Controls.AddRange(@($label,$list,$pathBox,$browse,$install,$cancel));$form.TopMost=$true;$form.Add_Shown({$form.Activate();$form.TopMost=$false});try{if($form.ShowDialog() -ne 'OK'){throw '安装已取消，未更改 AE。'};return $pathBox.Text}finally{$form.Dispose()}
}
try{
 Note '安装程序已启动，准备检测并选择 AE 目录。'
 if(Get-Process AfterFX -ErrorAction SilentlyContinue){throw '请先保存工程并关闭 AE，再运行安装。安装不会强制关闭 AE。'}
 if(-not $PanelDirectory){
  $candidates=@(Get-KosuzuAEInstallations)
  if($Quiet){if(-not $candidates.Count){throw '未找到 C 盘 AE 2023–2026；请使用 -PanelDirectory 指定目录。'};$PanelDirectory=$candidates[0].PanelDirectory}
  else{$PanelDirectory=Select-KosuzuAEFolder $candidates}
  if(-not(Test-Path -LiteralPath $PanelDirectory)){New-Item -ItemType Directory -Path $PanelDirectory -Force|Out-Null}
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
