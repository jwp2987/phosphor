# windows/ci/cases_caseintl.ps1: group 8 of the new real-app cases (#1466), international input on Windows. ASCII-ONLY SOURCE (Windows
# PowerShell 5.1 reads a BOM-less file as ANSI): every non-ASCII character is built from a code point.
# Dot-sourced by app_test.ps1 right after $Cases is defined; appends to $Cases (full tier only: the smoke filter drops them).
#   intl_wide          CJK, accents, emoji, a ZWJ family typed (KEYEVENTF_UNICODE packets) and pasted: the file PowerShell wrote holds the exact UTF-8
#   intl_altgr_ps      German layout (KLID 00000407) loaded for the Phosphor window only (WM_INPUTLANGCHANGEREQUEST; nothing system-wide is
#                      changed): AltGr+Q @, E EUR, 7 {, 8 [, 9 ], 0 }, sz \, < |, + ~, 2 sq, 3 cu, M micro, and a plain Y key that must give z.
#                      The characters are typed INTO a command that writes them to a file: the bytes PowerShell received are read back.
#   intl_altgr_bash    the same through Git Bash started from the pane (skips when Git for Windows is not installed)
# Keyboard-layout safety: Intl-Snapshot (first use) saves the user's language list + HKCU\Keyboard Layout\Preload to $Bak\kbd_pre.json; the
# layout is switched back and unloaded in a finally block, and restore.ps1 step "keyboard-layout" re-verifies the snapshot and repairs it.

Add-Type @"
using System; using System.Runtime.InteropServices;
public class IL {
 [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx, dy; public uint mouseData, dwFlags, time; public IntPtr extra; }
 [StructLayout(LayoutKind.Sequential)] struct KEYBDINPUT { public ushort vk, scan; public uint flags, time; public IntPtr extra; }
 [StructLayout(LayoutKind.Explicit)] struct INPUTU { [FieldOffset(0)] public MOUSEINPUT mi; [FieldOffset(0)] public KEYBDINPUT ki; }
 [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public INPUTU u; }
 [DllImport("user32.dll", SetLastError = true)] static extern uint SendInput(uint n, INPUT[] i, int size);
 [DllImport("user32.dll")] static extern uint MapVirtualKey(uint code, uint type);
 [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr LoadKeyboardLayout(string klid, uint flags);
 [DllImport("user32.dll")] public static extern bool UnloadKeyboardLayout(IntPtr hkl);
 [DllImport("user32.dll")] public static extern IntPtr GetKeyboardLayout(uint tid);
 [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
 [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
 public static int Fails = 0;
 static void Ev(ushort vk, ushort scan, uint flags) {
  var i = new INPUT[1]; i[0].type = 1; i[0].u.ki.vk = vk; i[0].u.ki.scan = scan; i[0].u.ki.flags = flags;
  if (SendInput(1, i, Marshal.SizeOf(typeof(INPUT))) != 1) Fails++; }
 static void Vk(ushort vk, bool down, bool ext) { Ev(vk, (ushort)MapVirtualKey(vk, 0), (down ? 0u : 2u) | (ext ? 1u : 0u)); }
 // a plain key by virtual key code, layout decides the character (the receiving thread's layout)
 public static void Plain(ushort vk) { Vk(vk, true, false); Vk(vk, false, false); }
 // AltGr + key: Right Alt (extended). On a layout with AltGr the system generates the Left Control that goes with it.
 public static void AltGr(ushort vk) { Vk(0xA5, true, true); Vk(vk, true, false); Vk(vk, false, false); Vk(0xA5, false, true); }
 public static uint WindowThread(IntPtr h) { uint pid; return GetWindowThreadProcessId(h, out pid); }
}
"@

function Intl-S([int[]]$cp) { -join ($cp | ForEach-Object { [char]::ConvertFromUtf32($_) }) }
function Intl-Hex($b) { if ($null -eq $b) { "none" } else { (($b | ForEach-Object { "{0:x2}" -f $_ }) -join "") } }
function Intl-Utf8([string]$s) { [Text.UTF8Encoding]::new($false).GetBytes($s) }
function Intl-ReadBytes($path, $n, $timeout = 12) {
  $t = Get-Date
  while (((Get-Date) - $t).TotalSeconds -lt $timeout) { if ((Test-Path $path) -and (Get-Item $path).Length -ge $n) { Start-Sleep -Milliseconds 300; break }; Start-Sleep -Milliseconds 150 }
  if (Test-Path $path) { , [IO.File]::ReadAllBytes($path) } else { $null }
}
# type a string element by element (a surrogate pair stays one unit) with the unicode-packet method, which does not depend on the target's layout
function Intl-Type([string]$t, [switch]$Enter) {
  $en = [Globalization.StringInfo]::GetTextElementEnumerator($t)
  while ($en.MoveNext()) { Keys (SKEsc ([string]$en.GetTextElement())); Start-Sleep -Milliseconds $rng.Next(10, 26) }
  if ($Enter) { Keys "{ENTER}" }
}
function Intl-WithUnicodeKeys([scriptblock]$body) {
  $old = $script:KeyMethod; $script:KeyMethod = "sendinput-unicode"
  try { & $body } finally { $script:KeyMethod = $old }
}

$script:IntlSnap = $false
function Intl-Snapshot {
  if ($script:IntlSnap) { return }
  $pre = @{ preload = @{}; langs = @() }
  try { $k = Get-ItemProperty "HKCU:\Keyboard Layout\Preload" -ErrorAction Stop; foreach ($p in $k.PSObject.Properties) { if ($p.Name -notlike "PS*") { $pre.preload[$p.Name] = [string]$p.Value } } } catch {}
  try { $pre.langs = @(Get-WinUserLanguageList | ForEach-Object { @{ tag = $_.LanguageTag; tips = @($_.InputMethodTips) } }) } catch {}
  $pre | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $Bak "kbd_pre.json") -Encoding ascii
  $script:IntlSnap = $true
}
# switch the Phosphor window to the German layout. Returns @{ ok; reason; hkl; orig }.
function Intl-GermanOn {
  Intl-Snapshot
  $h = $script:AppProc.MainWindowHandle; $tid = [IL]::WindowThread($h)
  $orig = [IL]::GetKeyboardLayout($tid)
  $de = [IL]::LoadKeyboardLayout("00000407", 0)
  if ($de -eq [IntPtr]::Zero) { return @{ ok = $false; reason = "LoadKeyboardLayout(00000407, German) failed: the layout is not available on this VM"; hkl = $de; orig = $orig } }
  [void][IL]::PostMessage($h, 0x50, [IntPtr]::Zero, $de)     # WM_INPUTLANGCHANGEREQUEST
  $t = Get-Date; $cur = $orig
  while (((Get-Date) - $t).TotalSeconds -lt 5) { $cur = [IL]::GetKeyboardLayout($tid); if (($cur.ToInt64() -band 0xFFFF) -eq 0x0407) { break }; Start-Sleep -Milliseconds 200 }
  if (($cur.ToInt64() -band 0xFFFF) -ne 0x0407) { return @{ ok = $false; reason = "the Phosphor window's layout did not change to German (is 0x{0:x}, was 0x{1:x}): the VM did not apply WM_INPUTLANGCHANGEREQUEST" -f $cur.ToInt64(), $orig.ToInt64(); hkl = $de; orig = $orig } }
  Start-Sleep -Milliseconds 400
  @{ ok = $true; reason = ""; hkl = $de; orig = $orig }
}
function Intl-GermanOff($st) {
  try {
    if ($st.orig -and $script:AppProc -and -not $script:AppProc.HasExited) { [void][IL]::PostMessage($script:AppProc.MainWindowHandle, 0x50, [IntPtr]::Zero, $st.orig); Start-Sleep -Milliseconds 600 }
    if ($st.hkl -and $st.hkl -ne $st.orig) { [void][IL]::UnloadKeyboardLayout($st.hkl) }
  } catch {}
}

function Intl-Compare($label, $got, [string]$text) {
  $want = Intl-Utf8 $text
  if ($null -ne $got -and (Intl-Hex $got) -eq (Intl-Hex $want)) { $null } else { "${label}: file bytes $(Intl-Hex $got), expected $(Intl-Hex $want)" }
}

$IntlWide = @(
  @("cjk", (Intl-S 0x65E5, 0x672C, 0x8A9E, 0x306E, 0x30C6, 0x30B9, 0x30C8)),
  @("accents", (Intl-S 0xE9, 0xFC, 0xF1, 0xE7, 0xDF, 0xF8)),
  @("emoji", (Intl-S 0x1F600, 0x1F680)),
  @("zwj", (Intl-S 0x1F468, 0x200D, 0x1F469, 0x200D, 0x1F467)))

function C-IntlWide($mode) {
  $fails = @(); $enc = "(New-Object Text.UTF8Encoding `$false)"
  foreach ($w in $IntlWide) {
    $tag = $w[0]; $text = $w[1]
    foreach ($how in "typed", "pasted") {
      $f = "$Work\iw_${how}_$tag.txt"; Remove-Item $f -ErrorAction SilentlyContinue
      $cmd = "[IO.File]::WriteAllText('$f', '$text', $enc)"
      try {
        if ($how -eq "typed") { Intl-WithUnicodeKeys { Intl-Type $cmd -Enter } }
        else { Set-Clipboard -Value $cmd; $back = Get-Clipboard -Raw; if ($back.TrimEnd() -ne $cmd) { $fails += "$how ${tag}: harness clipboard round trip altered the text"; continue }; Keys "^+v"; Start-Sleep -Milliseconds 200; Keys "{ENTER}" }
      } catch { $fails += "$how ${tag}: $($_.Exception.Message)"; continue }
      $e = Intl-Compare "$how $tag" (Intl-ReadBytes $f (Intl-Utf8 $text).Length) $text
      if ($e) { $fails += $e }
      Start-Sleep -Milliseconds 400
    }
  }
  if ($fails.Count) { @("fail", $fails, @{}) } else { @("pass", @(), @{ samples = $IntlWide.Count * 2 }) }
}

# German AltGr string: key + expected character, built from code points
$IntlAltGr = @(@(0x51, 0x40), @(0x45, 0x20AC), @(0x37, 0x7B), @(0x38, 0x5B), @(0x39, 0x5D), @(0x30, 0x7D), @(0xDB, 0x5C), @(0xE2, 0x7C), @(0xBB, 0x7E), @(0x32, 0xB2), @(0x33, 0xB3), @(0x4D, 0xB5))
function Intl-SendAltGrString {
  foreach ($p in $IntlAltGr) { if (-not (IsPho)) { throw "not foreground: refusing to type" }; [IL]::AltGr([uint16]$p[0]); Start-Sleep -Milliseconds $rng.Next(10, 26) }
  if (-not (IsPho)) { throw "not foreground: refusing to type" }; [IL]::Plain(0x59); Start-Sleep -Milliseconds 20      # Y key on a German layout types z
}
function Intl-AltGrExpected { (Intl-S ($IntlAltGr | ForEach-Object { $_[1] })) + "z" }

function C-IntlAltGrPs($mode) {
  $st = Intl-GermanOn
  if (-not $st.ok) { return @("skip", @("German layout unavailable: " + $st.reason), @{}) }
  try {
    $f = "$Work\ag_ps.txt"; Remove-Item $f -ErrorAction SilentlyContinue; $want = Intl-AltGrExpected
    Intl-WithUnicodeKeys {
      Intl-Type "[IO.File]::WriteAllText('$f', '"
      Intl-SendAltGrString
      Intl-Type "', (New-Object Text.UTF8Encoding `$false))" -Enter
    }
    $e = Intl-Compare "AltGr string in PowerShell" (Intl-ReadBytes $f (Intl-Utf8 $want).Length) $want
    $m = @{ layout = "German 00000407, per-window"; sendinput_failures = [IL]::Fails }
    if ($e) { @("fail", @($e, "expected characters: @ EUR { [ ] } \ | ~ sq cu micro, then z from the Y key; the layout was German for the window"), $m) } else { @("pass", @(), $m) }
  } finally { Intl-GermanOff $st }
}

function C-IntlAltGrBash($mode) {
  $bash = @("$env:ProgramFiles\Git\bin\bash.exe", "${env:ProgramFiles(x86)}\Git\bin\bash.exe") | Where-Object { $_ -and (Test-Path $_) } | Select-Object -First 1
  if (-not $bash) { return @("skip", @("Git for Windows (Git Bash) is not installed on this VM: no bash.exe under Program Files\Git\bin"), @{}) }
  $st = Intl-GermanOn
  if (-not $st.ok) { return @("skip", @("German layout unavailable: " + $st.reason), @{}) }
  try {
    $f = "$Work\ag_bash.txt"; Remove-Item $f -ErrorAction SilentlyContinue; $want = Intl-AltGrExpected
    Intl-WithUnicodeKeys {
      Intl-Type "`$env:CHERE_INVOKING = '1'; `$env:LC_ALL = 'C.UTF-8'; & '$bash' --login -i" -Enter
      Start-Sleep -Seconds 4
      Intl-Type "printf '%s' '"
      Intl-SendAltGrString
      Intl-Type "' > ag_bash.txt" -Enter
      Start-Sleep -Milliseconds 800
    }
    $got = Intl-ReadBytes $f (Intl-Utf8 $want).Length
    Intl-WithUnicodeKeys { Intl-Type "exit" -Enter; Start-Sleep -Seconds 1; Intl-Type "Remove-Item Env:LC_ALL, Env:CHERE_INVOKING -ErrorAction SilentlyContinue" -Enter }
    $e = Intl-Compare "AltGr string in Git Bash (LC_ALL=C.UTF-8)" $got $want
    $m = @{ layout = "German 00000407, per-window"; bash = $bash }
    if ($e) { @("fail", @($e), $m) } else { @("pass", @(), $m) }
  } finally { Intl-GermanOff $st }
}

$Cases += @(
  @{ n = "intl_wide"; f = ${function:C-IntlWide} },
  @{ n = "intl_altgr_ps"; f = ${function:C-IntlAltGrPs} },
  @{ n = "intl_altgr_bash"; f = ${function:C-IntlAltGrBash} })
