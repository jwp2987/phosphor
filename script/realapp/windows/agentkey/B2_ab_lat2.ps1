# realapp/windows: PowerShell scripts executed on the Windows 10 test VM (interactive desktop session via indesk.ps1) and wrapper .sh/.py on the driver machine.
# Platform: Windows 10 x64, Windows PowerShell 5.1, standard user. @WINUSER@ is a placeholder: render with ../shared/render.sh before upload.
# Safety: take C:\Users\@WINUSER@\WINPUSH.lock before driving the GUI (one agent at a time); back up the app data dir with md5 before and restore after (lib/bak*.ps1, fin*.ps1, restore_sys.ps1); never touch the GitLab runner.
# Prereqs: config.example.sh sourced; ssh key auth to the VM; IsPho foreground guard in lib/lib.ps1 refuses to type when Phosphor is not foreground.
# #1225 / #1279 owner A/B (2026-11-03): key -> first changed pixel with PHOSPHOR_FRAME_LATENCY=2 against the DX12 default (1,
# resources.rs desired_frame_latency), interleaved def/lat2/def/lat2 so the session's 31/46 ms vsync phase drifts equally into both.
# Both a PowerShell tab (terminal mode) and an agent-conversation tab are measured in every block (owner rule: both modes).
# Read the result with: python3 dist.py out/lat_ab_lat2.csv   (labels: <variant>|r<round>|<tab>-key)
Start-Transcript "$R\out\B_ab_lat2_transcript.txt" -Force | Out-Null
. C:\Users\@WINUSER@\r18wk\lat.ps1
LatInit 700 200
$o="$R\out\lat_ab_lat2.csv"; Remove-Item $o -ea 0
foreach($round in 1,2){
  foreach($L in 'def','lat2'){
    $env:PHOSPHOR_GPU_TIMING="1"
    if($L -eq 'lat2'){ $env:PHOSPHOR_FRAME_LATENCY="2" } else { Remove-Item Env:PHOSPHOR_FRAME_LATENCY -ea 0 }
    FreshApp "info,warp::input_order=debug,warpui::frame_cause=debug,warpui::gpu_done=debug" 14
    Rect
    [M]::Click(300,660); Start-Sleep 1
    LatRun "$L|r$round|ps-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
    [M]::Click(313,42); Start-Sleep 1; [M]::Click(330,159); Start-Sleep 5; Focus
    [M]::Click(300,660); Start-Sleep 1
    LatRun "$L|r$round|agent-key" 0 520 "zqxjkvwpyz" 40 $o "^a{BACKSPACE}" 20
    Shot "B_ab_${L}_r${round}_agent"
    SaveLog "B_ab_${L}_r${round}_app"
  }
}
Remove-Item Env:PHOSPHOR_FRAME_LATENCY -ea 0
"done " + (Get-Content $o).Count
Stop-Transcript | Out-Null
