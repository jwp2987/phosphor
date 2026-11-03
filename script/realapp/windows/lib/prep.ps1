# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
$r="C:\Users\@WINUSER@\r18ba"
New-Item -ItemType Directory -Force "$r","$r\out","$r\shots","$r\bak" | Out-Null
foreach($f in 'lib.ps1','mouse.ps1','lat.ps1','tc.ps1','setup.ps1','md5.ps1'){ (Get-Content "C:\Users\@WINUSER@\r18v\$f" -Raw) -replace 'r18v','r18ba' | Set-Content -Encoding UTF8 "$r\$f" }
Copy-Item C:\Users\@WINUSER@\r18v\md5_orig.txt $r\md5_r18v_final.txt
Get-ChildItem "C:\Users\@WINUSER@\AppData\Local\phosphor" -Recurse -File | measure Length -Sum | % { "files $($_.Count) bytes $($_.Sum)" }
Get-ChildItem "C:\Users\@WINUSER@\AppData\Local\phosphor" -Recurse -File | % { $_.FullName.Substring(28) } | select -First 40
Test-Path C:\Users\@WINUSER@\AppData\Roaming\phosphor
[Environment]::GetEnvironmentVariable('Path','User')
reg export "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\zap-oss_is1" "$r\uninstall_zap-oss_is1.reg" /y
Get-Process | ? Name -match 'phosphor|pwsh|WindowsTerminal|OpenConsole|bash' | select Name,Id | ft | out-string
Get-Command pwsh,wt,bash,cmd -ea 0 | select Name,Source | ft | out-string
Get-ChildItem 'C:\Program Files\PowerShell','C:\Program Files\Git\bin\bash.exe' -ea 0 | select -expand FullName
(Get-WmiObject Win32_VideoController).Name
Get-WmiObject Win32_ComputerSystem | % { "$($_.NumberOfLogicalProcessors) cpus $([math]::Round($_.TotalPhysicalMemory/1GB,1))GB" }
"$([System.Windows.Forms.Screen]::PrimaryScreen)"
