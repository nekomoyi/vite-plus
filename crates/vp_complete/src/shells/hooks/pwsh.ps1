if (-not (Test-Path Function:\__vp_orig_TabExpansion2)) {
    Copy-Item Function:\TabExpansion2 Function:\__vp_orig_TabExpansion2
}

function global:__vp_complete_ast {
    param([System.Management.Automation.Language.CommandAst]$cmdAst, [int]$cursorOffset)

    $words = @()
    $idx = 0
    $repIdx = $cursorOffset
    $repLen = 0
    $matched = $false
    $rawCur = ''

    for ($i = 0; $i -lt $cmdAst.CommandElements.Count; $i++) {
        $elem = $cmdAst.CommandElements[$i]
        if ($cursorOffset -le $elem.Extent.StartOffset) { break }
        if ($cursorOffset -le $elem.Extent.EndOffset) {
            if ($cursorOffset -lt $elem.Extent.EndOffset -or $elem -isnot [System.Management.Automation.Language.StringConstantExpressionAst]) {
                $val = $elem.Extent.Text.Substring(0, $cursorOffset - $elem.Extent.StartOffset)
                $rawCur = '1'
            } else {
                $val = $elem.Value
            }
            $words += $val
            $idx = $words.Count - 1
            $repIdx = $elem.Extent.StartOffset
            $repLen = $elem.Extent.EndOffset - $elem.Extent.StartOffset
            $matched = $true
            break
        }
        $words += if ($elem -is [System.Management.Automation.Language.StringConstantExpressionAst]) { $elem.Value } else { $elem.Extent.Text }
    }

    if (-not $matched) {
        $words += ''
        $idx = $words.Count - 1
    }

    $vpExe = ''
    if (-not ($vpExe -and [IO.File]::Exists($vpExe)) -and $env:PATH) {
        $vpExe = $null
        foreach ($dir in ($env:PATH -split [IO.Path]::PathSeparator)) {
            $trimmed = $dir.Trim().Trim('"')
            if (-not $trimmed -or -not [IO.Path]::IsPathRooted($trimmed)) { continue }
            if ($trimmed.Replace('\', '/').TrimEnd('/') -imatch '(^|/)node_modules/\.bin($|/)') { continue }
            foreach ($name in @('vp.exe', 'vp')) {
                $cand = Join-Path $trimmed $name
                if ([IO.File]::Exists($cand)) { $vpExe = $cand; break }
            }
            if ($vpExe) { break }
        }
    }

    $results = [System.Collections.ObjectModel.Collection[System.Management.Automation.CompletionResult]]::new()
    if (-not $vpExe) {
        return @{ Results = $results; Directive = 0; RepIdx = $repIdx; RepLen = $repLen }
    }

    $oldComplete = $env:VP_COMPLETE
    $oldIndex = $env:_VP_COMPLETE_INDEX
    $oldRaw = $env:_VP_PWSH_RAW_CUR
    $oldExit = $global:LASTEXITCODE
    $lines = @()
    try {
        $env:VP_COMPLETE = 'powershell'
        $env:_VP_COMPLETE_INDEX = "$idx"
        $env:_VP_PWSH_RAW_CUR = $rawCur
        $lines = @(& $vpExe -- @words 2>$null)
    } finally {
        $env:VP_COMPLETE = $oldComplete
        $env:_VP_COMPLETE_INDEX = $oldIndex
        $env:_VP_PWSH_RAW_CUR = $oldRaw
        $global:LASTEXITCODE = $oldExit
    }

    $directive = 0
    if ($lines.Count -gt 0 -and $lines[-1] -like ':*') {
        $directive = [int]($lines[-1].Substring(1))
        $lines = if ($lines.Count -gt 1) { $lines[0..($lines.Count - 2)] } else { @() }
    }

    foreach ($line in $lines) {
        if ([string]::IsNullOrWhiteSpace($line)) { continue }
        $p = $line.Split("`t")
        if ([string]::IsNullOrEmpty($p[0])) { continue }
        $list = if ($p.Length -gt 1 -and -not [string]::IsNullOrWhiteSpace($p[1])) { $p[1] } elseif (-not [string]::IsNullOrWhiteSpace($p[0])) { $p[0] } else { ' ' }
        $type = if ($p.Length -gt 2 -and -not [string]::IsNullOrWhiteSpace($p[2])) { $p[2] } else { 'ParameterValue' }
        $tip = if ($p.Length -gt 3 -and -not [string]::IsNullOrWhiteSpace($p[3])) { $p[3] } else { $list }
        $results.Add([System.Management.Automation.CompletionResult]::new($p[0], $list, $type, $tip))
    }

    return @{ Results = $results; Directive = $directive; RepIdx = $repIdx; RepLen = $repLen }
}

function global:TabExpansion2 {
    [CmdletBinding(DefaultParameterSetName = 'ScriptInputSet')]
    [OutputType([System.Management.Automation.CommandCompletion])]
    param(
        [Parameter(ParameterSetName = 'ScriptInputSet', Mandatory = $true, Position = 0)] [AllowEmptyString()] [string]$inputScript,
        [Parameter(ParameterSetName = 'ScriptInputSet', Position = 1)] [int]$cursorColumn = $inputScript.Length,
        [Parameter(ParameterSetName = 'AstInputSet', Mandatory = $true, Position = 0)] [System.Management.Automation.Language.Ast]$ast,
        [Parameter(ParameterSetName = 'AstInputSet', Mandatory = $true, Position = 1)] [System.Management.Automation.Language.Token[]]$tokens,
        [Parameter(ParameterSetName = 'AstInputSet', Mandatory = $true, Position = 2)] [System.Management.Automation.Language.IScriptPosition]$positionOfCursor,
        [Parameter(ParameterSetName = 'ScriptInputSet', Position = 2)] [Parameter(ParameterSetName = 'AstInputSet', Position = 3)] [hashtable]$options = $null
    )

    if ($PSCmdlet.ParameterSetName -eq 'ScriptInputSet') {
        $parsed = [System.Management.Automation.CommandCompletion]::MapStringInputToParsedInput($inputScript, $cursorColumn)
        return TabExpansion2 -ast $parsed.Item1 -tokens $parsed.Item2 -positionOfCursor $parsed.Item3 -options $options
    }

    $cursorOffset = $positionOfCursor.Offset
    $cmdAst = $ast.FindAll({
        param($m)
        if ($m -isnot [System.Management.Automation.Language.CommandAst] -or $cursorOffset -lt $m.Extent.StartOffset) { return $false }
        if ($cursorOffset -le $m.Extent.EndOffset) { return $true }
        foreach ($tok in $tokens) {
            if ($tok.Extent.StartOffset -ge $m.Extent.EndOffset -and $tok.Extent.StartOffset -lt $cursorOffset) { return $false }
        }
        return $true
    }, $true) | Select-Object -Last 1

    if (-not $cmdAst -or $cmdAst.CommandElements.Count -eq 0 -or $cursorOffset -le $cmdAst.CommandElements[0].Extent.EndOffset) {
        return __vp_orig_TabExpansion2 -ast $ast -tokens $tokens -positionOfCursor $positionOfCursor -options $options
    }

    $first = $cmdAst.CommandElements[0]
    $cmdName = if ($first -is [System.Management.Automation.Language.StringConstantExpressionAst]) { $first.Value } else { $first.Extent.Text }
    if ([IO.Path]::GetFileName($cmdName).ToLowerInvariant() -notin @('vp', 'vp.exe', 'vp.cmd', 'vp.bat', 'vp.ps1', 'vpr', 'vpr.exe', 'vpr.cmd', 'vpr.bat', 'vpr.ps1', 'vpx', 'vpx.exe', 'vpx.cmd', 'vpx.bat', 'vpx.ps1')) {
        return __vp_orig_TabExpansion2 -ast $ast -tokens $tokens -positionOfCursor $positionOfCursor -options $options
    }

    $comp = __vp_complete_ast $cmdAst $cursorOffset
    if ($comp.Results.Count -eq 0) {
        if (($comp.Directive -band 4) -ne 0) {
            return [System.Management.Automation.CommandCompletion]::new($comp.Results, -1, $comp.RepIdx, $comp.RepLen)
        }
        return __vp_orig_TabExpansion2 -ast $ast -tokens $tokens -positionOfCursor $positionOfCursor -options $options
    }
    return [System.Management.Automation.CommandCompletion]::new($comp.Results, 0, $comp.RepIdx, $comp.RepLen)
}

Register-ArgumentCompleter -Native -CommandName 'vp', 'vp.exe', 'vp.cmd', 'vp.bat', 'vp.ps1', 'vpr', 'vpr.exe', 'vpr.cmd', 'vpr.bat', 'vpr.ps1', 'vpx', 'vpx.exe', 'vpx.cmd', 'vpx.bat', 'vpx.ps1' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    (__vp_complete_ast $commandAst $cursorPosition).Results
}
