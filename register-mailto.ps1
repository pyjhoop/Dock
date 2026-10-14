# Dock 을 Windows "기본 앱 > mailto" 후보로 등록한다 (HKCU 만 사용, 관리자 권한 불필요).
# 사용: powershell -ExecutionPolicy Bypass -File register-mailto.ps1 -Exe "C:\경로\dock.exe"
# 등록 후 설정 > 앱 > 기본 앱 > Dock 에서 MAILTO 를 Dock 으로 지정하세요.
param([Parameter(Mandatory = $true)][string]$Exe)

$Exe = (Resolve-Path $Exe).Path
$cap = 'HKCU:\Software\Dock\Capabilities'
New-Item -Path $cap -Force | Out-Null
Set-ItemProperty $cap -Name ApplicationName -Value 'Dock'
Set-ItemProperty $cap -Name ApplicationDescription -Value 'Dock personal web app dock'
New-Item -Path "$cap\URLAssociations" -Force | Out-Null
Set-ItemProperty "$cap\URLAssociations" -Name mailto -Value 'Dock.mailto'

New-Item -Path 'HKCU:\Software\RegisteredApplications' -Force | Out-Null
Set-ItemProperty 'HKCU:\Software\RegisteredApplications' -Name Dock -Value 'Software\Dock\Capabilities'

$cls = 'HKCU:\Software\Classes\Dock.mailto'
New-Item -Path "$cls\shell\open\command" -Force | Out-Null
Set-ItemProperty $cls -Name '(default)' -Value 'URL:Dock mailto'
Set-ItemProperty $cls -Name 'URL Protocol' -Value ''
Set-ItemProperty "$cls\shell\open\command" -Name '(default)' -Value "`"$Exe`" `"%1`""

Write-Host "등록 완료. 설정 > 앱 > 기본 앱 에서 Dock 의 MAILTO 를 지정하세요."
Start-Process 'ms-settings:defaultapps'
