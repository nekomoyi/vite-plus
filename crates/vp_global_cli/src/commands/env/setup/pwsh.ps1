# Vite+ environment setup (https://viteplus.dev)
__ENV_EXPORTS__$__vp_bin = '__VP_BIN_WIN__'
$__vp_fallback = '__VP_FALLBACK_BIN_WIN__'
$env:PATH = @(
    $__vp_bin
    $env:PATH -split [IO.Path]::PathSeparator | Where-Object { $_ -and $_ -ne $__vp_bin -and $_ -ne $__vp_fallback }
    $__vp_fallback
) -join [IO.Path]::PathSeparator

# A script can propagate native failure through both $? and $LASTEXITCODE.
# A plain function returns $? = $true even when its native command fails.
Set-Alias vp (Join-Path $PSScriptRoot "vp.ps1")

# Dynamic shell completion for PowerShell
$env:VP_COMPLETE = "powershell"
& (Join-Path $__vp_bin "vp") | Out-String | Invoke-Expression
Remove-Item Env:\VP_COMPLETE -ErrorAction SilentlyContinue

$__vpr_comp = {
    param($wordToComplete, $commandAst, $cursorPosition)
    $prev = $env:VP_COMPLETE
    $env:VP_COMPLETE = "powershell"
    $commandLine = $commandAst.Extent.Text
    $args = $commandLine.Substring(0, [math]::Min($cursorPosition, $commandLine.Length))
    if ($args -match '^(vpr\.exe|vpr)\b(\s+-C\s+(?:"[^"]*"|''[^'']*''|\S+))\s') {
        $args = $args -replace '^(vpr\.exe|vpr)\b(\s+-C\s+(?:"[^"]*"|''[^'']*''|\S+))\s', 'vp$2 run '
    } elseif ($args -match '^(vpr\.exe|vpr)\b(\s+-C=?(?:"[^"]*"|''[^'']*''|\S+))\s') {
        $args = $args -replace '^(vpr\.exe|vpr)\b(\s+-C=?(?:"[^"]*"|''[^'']*''|\S+))\s', 'vp$2 run '
    } elseif ($args -match '^(vpr\.exe|vpr)\b\s+-C') {
        $args = $args -replace '^(vpr\.exe|vpr)\b', 'vp'
    } else {
        $args = $args -replace '^(vpr\.exe|vpr)\b', 'vp run'
    }
    if ($wordToComplete -eq "") { $args += " ''" }
    $results = Invoke-Expression @"
& (Join-Path $__vp_bin 'vp') -- $args
"@;
    if ($prev) { $env:VP_COMPLETE = $prev } else { Remove-Item Env:\VP_COMPLETE }
    $results | ForEach-Object {
        $split = $_.Split("`t")
        $cmd = $split[0];
        if ($split.Length -eq 2) { $help = $split[1] } else { $help = $split[0] }
        [System.Management.Automation.CompletionResult]::new($cmd, $cmd, 'ParameterValue', $help)
    }
}
Register-ArgumentCompleter -Native -CommandName vpr -ScriptBlock $__vpr_comp
