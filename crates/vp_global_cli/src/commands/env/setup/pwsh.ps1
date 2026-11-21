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
$__vp_exe = Join-Path $__vp_bin $(if ($env:OS -eq 'Windows_NT') { 'vp.exe' } else { 'vp' })
if ([IO.File]::Exists($__vp_exe)) {
    $__vp_prev_complete = $env:VP_COMPLETE
    $__vp_prev_exit = $global:LASTEXITCODE
    try {
        $env:VP_COMPLETE = 'powershell'
        (& $__vp_exe | Out-String) | Invoke-Expression
    } finally {
        $env:VP_COMPLETE = $__vp_prev_complete
        $global:LASTEXITCODE = $__vp_prev_exit
    }
    Remove-Variable __vp_prev_complete, __vp_prev_exit -ErrorAction SilentlyContinue
}
Remove-Variable __vp_exe -ErrorAction SilentlyContinue
