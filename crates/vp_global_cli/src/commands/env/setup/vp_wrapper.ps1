# Vite+ PowerShell wrapper (https://viteplus.dev)
$__vp_bin = '__VP_BIN_WIN__'
# PowerShell consumes the first bare -- when calling a script. Recover its
# position from the caller's AST, without evaluating argument expressions again.
# The call stack also works on Windows PowerShell 5.1, which has no Statement.
$__vp_stack = @(Get-PSCallStack)
$__vp_command = $null
if ($__vp_stack.Count -gt 1) {
    $__vp_ast = $__vp_stack[1].InvocationInfo.MyCommand.ScriptBlock.Ast
    $__vp_line = $MyInvocation.ScriptLineNumber
    $__vp_column = $MyInvocation.OffsetInLine
    if ($__vp_ast) {
        $__vp_command = $__vp_ast.Find({ param($node)
            $node -is [System.Management.Automation.Language.CommandAst] -and
            $node.Extent.StartLineNumber -eq $__vp_line -and $node.Extent.StartColumnNumber -eq $__vp_column
        }, $true)
    }
}
if ($__vp_command) {
    $__vp_index = 0
    $__vp_elements = $__vp_command.CommandElements
    for ($__vp_i = 1; $__vp_i -lt $__vp_elements.Count; $__vp_i++) {
        $__vp_element = $__vp_elements[$__vp_i]
        # A preceding splat has an unknown length. Leave $args unchanged rather
        # than reading caller variables, which may have changed or belong to another scope.
        if ($__vp_element -is [System.Management.Automation.Language.VariableExpressionAst] -and $__vp_element.Splatted) {
            break
        }
        # Only a parameter AST can be consumed. After --%, the separator is literal.
        if ($__vp_element -is [System.Management.Automation.Language.CommandParameterAst] -and $__vp_element.Extent.Text -ceq '--') {
            if ($__vp_index -le $args.Count) {
                $__vp_arguments = [System.Collections.Generic.List[object]]::new()
                $__vp_arguments.AddRange($args)
                $__vp_arguments.Insert($__vp_index, '--')
                $args = $__vp_arguments.ToArray()
            }
            break
        }
        $__vp_index++
        if ($__vp_element -is [System.Management.Automation.Language.CommandParameterAst] -and $__vp_element.Argument) {
            $__vp_index++
        }
    }
}
$__vp_command_index = 0
if ($args.Count -ge 1) {
    if ($args[0] -ceq "-C") {
        $__vp_command_index = 2
    } elseif ("$($args[0])" -clike "-C?*") {
        $__vp_command_index = 1
    }
}
# Intercept `vp env use` to set/unset VP_NODE_VERSION in the current session.
if ($args.Count -ge ($__vp_command_index + 2) -and $args[$__vp_command_index] -ceq "env" -and $args[$__vp_command_index + 1] -ceq "use") {
    if ($args -ccontains "-h" -or $args -ccontains "--help") {
        & (Join-Path $__vp_bin "vp") @args
        exit $LASTEXITCODE
    }
    $previousEvalEnable = $env:VP_ENV_USE_EVAL_ENABLE
    $previousShell = $env:VP_SHELL
    $previousErrorActionPreference = $ErrorActionPreference
    try {
        $env:VP_ENV_USE_EVAL_ENABLE = "1"
        $env:VP_SHELL = "pwsh"
        # Windows PowerShell 5.1 treats native stderr as an error when redirected.
        $ErrorActionPreference = "Continue"
        $output = & (Join-Path $__vp_bin "vp") @args 2>&1 | ForEach-Object {
            if ($_ -is [System.Management.Automation.ErrorRecord]) {
                Write-Host $_.Exception.Message
            } else {
                $_
            }
        }
    } finally {
        $ErrorActionPreference = $previousErrorActionPreference
        $env:VP_ENV_USE_EVAL_ENABLE = $previousEvalEnable
        $env:VP_SHELL = $previousShell
    }
    if ($LASTEXITCODE -eq 0 -and $output) {
        Invoke-Expression ($output -join "`n")
    }
} else {
    & (Join-Path $__vp_bin "vp") @args
}
# Exits only this script, preserving the caller's session and native exit code.
exit $LASTEXITCODE
