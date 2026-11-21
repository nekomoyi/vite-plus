$ErrorActionPreference = 'Stop'

. (Join-Path $env:VP_HOME 'env.ps1')

function Assert-Completes([string]$Line, [string]$Expected) {
    $matches = @((TabExpansion2 $Line $Line.Length).CompletionMatches.CompletionText)
    if ($Expected -notin $matches) {
        throw "Line '$Line' missing expected completion '$Expected', got: $($matches -join ', ')"
    }
}

$global:LASTEXITCODE = 19
Assert-Completes 'vp un' 'uninstall'
if ($global:LASTEXITCODE -ne 19) {
    throw "TabExpansion2 clobbered LASTEXITCODE: expected 19, got $global:LASTEXITCODE"
}
Assert-Completes 'vp -C packages/subpkg run ' 'sub:task'
Assert-Completes 'vpr build:' 'build:watch'
Assert-Completes 'vp remove ' 'react'
Assert-Completes 'vp run --filter ' "'@demo/subpkg'"
Assert-Completes 'vp check --' '--fix-dangerously'
Assert-Completes 'vpx -C custom ./scripts/' './scripts/seed.ts'
Assert-Completes 'vp env current | Select-Object -Fir' '-First'

Write-Output 'PowerShell completion assertions passed'
