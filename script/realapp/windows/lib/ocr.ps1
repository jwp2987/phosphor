# realapp/windows/lib/ocr.ps1: OCR of one PNG with the Windows.Media.Ocr engine that ships with Windows 10 (1507 and later); no tesseract, nothing installed.
# Platform: Windows 10/11 x64, Windows PowerShell 5.1 (the WinRT projection and System.Runtime.WindowsRuntime are in the box). Run by appci/winocr.py as
#   powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File ocr.ps1 -Path <png> [-Lang en-US]      read the file
#   powershell.exe ... -File ocr.ps1 -Probe [-Lang en-US]                                                          only check an engine can be made
# Output (stdout, UTF-8, one record per line):  L<TAB>line text      one per recognised text line
#                                              W<TAB>x<TAB>y<TAB>w<TAB>h<TAB>word   one per word, pixels of the image (OcrWord.BoundingRect)
#                                              probe: OK<TAB>language tag
# Exit codes: 0 read; 2 file missing; 3 image larger than OcrEngine.MaxImageDimension; 4 no engine (the message names the language pack); 5 any other failure.
# Safety: reads one file, writes nothing, changes no setting. The engine needs an OCR language pack for the user's language or -Lang
# (Windows ships en-US OCR with an English install; `Get-WindowsCapability -Online -Name Language.OCR*` lists them: this script never installs one).
param([string]$Path = "", [string]$Lang = "", [switch]$Probe)
$ErrorActionPreference = "Stop"
try {
    [Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
    Add-Type -AssemblyName System.Runtime.WindowsRuntime
    $null = [Windows.Media.Ocr.OcrEngine, Windows.Foundation, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.BitmapDecoder, Windows.Foundation, ContentType = WindowsRuntime]
    $null = [Windows.Graphics.Imaging.SoftwareBitmap, Windows.Foundation, ContentType = WindowsRuntime]
    $null = [Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime]
    $null = [Windows.Storage.FileAccessMode, Windows.Storage, ContentType = WindowsRuntime]
    $null = [Windows.Globalization.Language, Windows.Globalization, ContentType = WindowsRuntime]
} catch {
    [Console]::Error.WriteLine("the Windows.Media.Ocr types could not be loaded: " + $_.Exception.Message)
    exit 4
}
$asTask = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq "AsTask" -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq "IAsyncOperation``1" })[0]
function Await($op, [Type]$type) {
    $t = $asTask.MakeGenericMethod($type).Invoke($null, @($op))
    $null = $t.Wait(-1)
    return $t.Result
}
try {
    if ($Lang) { $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage((New-Object Windows.Globalization.Language($Lang))) }
    else { $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromUserProfileLanguages() }
    if (-not $engine) { $engine = [Windows.Media.Ocr.OcrEngine]::TryCreateFromLanguage((New-Object Windows.Globalization.Language("en-US"))) }
    if (-not $engine) {
        [Console]::Error.WriteLine("no OCR engine: no OCR language pack for the user's languages or en-US (Settings > Language, or the capability Language.OCR~~~en-US~0.0.1.0)")
        exit 4
    }
    if ($Probe) { [Console]::Out.WriteLine("OK`t" + $engine.RecognizerLanguage.LanguageTag); exit 0 }
    if (-not $Path -or -not (Test-Path -LiteralPath $Path)) { [Console]::Error.WriteLine("file not found: " + $Path); exit 2 }
    $file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync((Resolve-Path -LiteralPath $Path).Path)) ([Windows.Storage.StorageFile])
    $stream = Await ($file.OpenAsync([Windows.Storage.FileAccessMode]::Read)) ([Windows.Storage.Streams.IRandomAccessStream])
    $decoder = Await ([Windows.Graphics.Imaging.BitmapDecoder]::CreateAsync($stream)) ([Windows.Graphics.Imaging.BitmapDecoder])
    if ($decoder.PixelWidth -gt [Windows.Media.Ocr.OcrEngine]::MaxImageDimension -or $decoder.PixelHeight -gt [Windows.Media.Ocr.OcrEngine]::MaxImageDimension) {
        [Console]::Error.WriteLine("image " + $decoder.PixelWidth + "x" + $decoder.PixelHeight + " is larger than OcrEngine.MaxImageDimension " + [Windows.Media.Ocr.OcrEngine]::MaxImageDimension)
        exit 3
    }
    $bmp = Await ($decoder.GetSoftwareBitmapAsync()) ([Windows.Graphics.Imaging.SoftwareBitmap])
    $res = Await ($engine.RecognizeAsync($bmp)) ([Windows.Media.Ocr.OcrResult])
    foreach ($line in $res.Lines) {
        [Console]::Out.WriteLine("L`t" + (($line.Text -replace "[`t`r`n]", " ")))
        foreach ($w in $line.Words) {
            $b = $w.BoundingRect
            [Console]::Out.WriteLine(("W`t{0}`t{1}`t{2}`t{3}`t{4}" -f [int]$b.X, [int]$b.Y, [int]$b.Width, [int]$b.Height, ($w.Text -replace "[`t`r`n]", " ")))
        }
    }
    exit 0
} catch {
    [Console]::Error.WriteLine("OCR failed: " + $_.Exception.Message)
    exit 5
}
