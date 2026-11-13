# shell_integration_cwd_templates

## `VP_HOME=${workspace}/home VP_NU_VERSION=0.112.1 vp env setup --refresh`

VP_NU_VERSION keeps env.nu independent of the installed Nushell


## `vpt print-file home/env`

POSIX wrapper and zsh vpr completion keep global -C before env use/run

```
#!/bin/sh
# Vite+ environment setup (https://viteplus.dev)
export VP_HOME="<workspace>/home"
__vp_bin="<workspace>/home/bin"
__vp_fallback="<workspace>/home/fallback-bin"
# Split PATH on ':' once; repeated `${PATH#*pattern}` stripping is super-linear on long PATHs.
# A function scopes zsh's option changes; other shells restore noglob and IFS explicitly.
__vp_dedupe_path() {
    __vp_ifs_set=${IFS+1}
    __vp_ifs=${IFS-}
    IFS=:
    __vp_glob=
    if [ -n "${ZSH_VERSION-}" ]; then
        setopt localoptions shwordsplit noglob
    else
        case $- in *f*) ;; *) set -f; __vp_glob=1 ;; esac
    fi
    __vp_new=
    # A non-empty sentinel last field keeps trailing empty entries consistent across shells.
    __vp_path="${PATH}:."
    for __vp_dir in $__vp_path; do
        case "$__vp_dir" in
            "$__vp_bin"|"$__vp_fallback") ;;
            *) __vp_new="${__vp_new}:${__vp_dir}" ;;
        esac
    done
    if [ -n "$__vp_ifs_set" ]; then IFS=$__vp_ifs; else unset IFS; fi
    [ -z "$__vp_glob" ] || set +f
    __vp_new="${__vp_new%:*}"
    PATH="${__vp_new#:}"
}
# Assigning a read-only IFS aborts dash and zsh, so probe it in a subshell first and fall back
# to stripping each copy with parameter expansion, which leaves IFS and shell options alone.
if (IFS=:) 2>/dev/null; then
    __vp_dedupe_path
else
    for __vp_dir in "$__vp_bin" "$__vp_fallback"; do
        while case ":${PATH}:" in *":${__vp_dir}:"*) true ;; *) false ;; esac; do
            __vp_tmp=":${PATH}:"
            __vp_before="${__vp_tmp%%":${__vp_dir}:"*}"
            __vp_before="${__vp_before#:}"
            __vp_after="${__vp_tmp#*":${__vp_dir}:"}"
            __vp_after="${__vp_after%:}"
            PATH="${__vp_before}${__vp_before:+${__vp_after:+:}}${__vp_after}"
        done
    done
fi
unset -f __vp_dedupe_path
export PATH="${__vp_bin}${PATH:+:${PATH}}:${__vp_fallback}"
unset __vp_bin __vp_fallback __vp_dir __vp_path __vp_new __vp_ifs __vp_ifs_set __vp_glob \
    __vp_tmp __vp_before __vp_after
hash -r 2>/dev/null || true

# Shell function wrapper: intercepts `vp env use` to eval its stdout,
# which sets/unsets VP_NODE_VERSION in the current shell session.
vp() {
    __vp_env_use=
    if [ "${1-}" = "env" ] && [ "${2-}" = "use" ]; then
        __vp_env_use=1
    elif [ "${1-}" = "-C" ] && [ "${3-}" = "env" ] && [ "${4-}" = "use" ]; then
        __vp_env_use=1
    else
        case "${1-}" in
            -C?*)
                if [ "${2-}" = "env" ] && [ "${3-}" = "use" ]; then
                    __vp_env_use=1
                fi
                ;;
        esac
    fi

    if [ -n "$__vp_env_use" ]; then
        unset __vp_env_use
        case " $* " in *" -h "*|*" --help "*) command vp "$@"; return; esac
        __vp_out="$(VP_ENV_USE_EVAL_ENABLE=1 VP_SHELL=sh command vp "$@")" || return $?
        eval "$__vp_out"
    else
        unset __vp_env_use
        command vp "$@" || return $?
        # Mode changes move executables between directories; discard cached command paths.
        hash -r 2>/dev/null || true
    fi
}

# Dynamic shell completion for bash/zsh
if [ -n "${BASH_VERSION-}" ] && type complete >/dev/null 2>&1; then
    eval "$(VP_COMPLETE=bash command vp 2>/dev/null)"
elif [ -n "${ZSH_VERSION-}" ]; then
    case $- in
        *i*) eval "$(VP_COMPLETE=zsh command vp 2>/dev/null)" ;;
        *)
            if type compdef >/dev/null 2>&1; then
                eval "$(VP_COMPLETE=zsh command vp 2>/dev/null)"
            fi
            ;;
    esac
fi
```

## `vpt print-file home/env.fish`

fish wrapper and vpr completion keep global -C before env use/run

```
# Vite+ environment setup (https://viteplus.dev)
set -gx VP_HOME "<workspace>/home"
while set -l __vp_idx (builtin contains -i -- "<workspace>/home/bin" $PATH)
    set -e PATH[$__vp_idx]
end
while set -l __vp_idx (builtin contains -i -- "<workspace>/home/fallback-bin" $PATH)
    set -e PATH[$__vp_idx]
end
set -gx PATH "<workspace>/home/bin" $PATH "<workspace>/home/fallback-bin"

# Shell function wrapper: intercepts `vp env use` to eval its stdout,
# which sets/unsets VP_NODE_VERSION in the current shell session.
function vp
    set -l __vp_command_index 1
    if test (builtin count $argv) -ge 1
        if test "$argv[1]" = "-C"
            set __vp_command_index 3
        else if string match -qr '^-C.+' -- "$argv[1]"
            set __vp_command_index 2
        end
    end
    set -l __vp_next_index (builtin math $__vp_command_index + 1)

    if test (builtin count $argv) -ge $__vp_next_index; and test "$argv[$__vp_command_index]" = "env"; and test "$argv[$__vp_next_index]" = "use"
        if builtin contains -- -h $argv; or builtin contains -- --help $argv
            command vp $argv; return
        end
        set -lx VP_ENV_USE_EVAL_ENABLE 1
        set -lx VP_SHELL fish
        set -l __vp_out (command vp $argv); or return $status
        for __vp_command in $__vp_out
            eval $__vp_command; or return $status
        end
        return 0
    else
        command vp $argv
    end
end

# Dynamic shell completion for fish
VP_COMPLETE=fish command vp 2>/dev/null | builtin source
```

## `vpt print-file home/env.nu`

Nushell wrapper and vpr completion keep global -C before env use/run

```
# Vite+ environment setup (https://viteplus.dev)
$env.VP_HOME = ("<workspace>/home" | %path expand --no-symlink)
$env.PATH = ($env.PATH | where { $in != "<workspace>/home/bin" and $in != "<workspace>/home/fallback-bin" } | %prepend "<workspace>/home/bin" | %append "<workspace>/home/fallback-bin")

# Shell function wrapper: intercepts `vp env use` to parse its stdout,
# which sets/unsets VP_NODE_VERSION in the current shell session.
def --env --wrapped vp [...args: string@"nu-complete vp"] {
    let command_args = if ($args | %length) >= 2 and $args.0 == "-C" {
        $args | %skip 2
    } else if ($args | %length) >= 1 and ($args.0 | %str starts-with "-C") and $args.0 != "-C" {
        $args | %skip 1
    } else {
        $args
    }
    if ($command_args | %length) >= 2 and $command_args.0 == "env" and $command_args.1 == "use" {
        if ("-h" in $args) or ("--help" in $args) {
            ^vp ...$args
            return
        }
        let out = (%with-env { VP_ENV_USE_EVAL_ENABLE: "1", VP_SHELL: "nu" } {
            ^vp ...$args
        })
        let lines = ($out | %lines)
        let exports = ($lines | where { $in =~ '^\$env\.' } | %parse '$env.{key} = "{value}"')
        let export_keys = ($exports | %get key? | %default [])
        # Exclude keys that also appear in exports: when vp emits `hide-env X` then
        # `$env.X = "v"` (e.g. `vp env use` with no args resolving from .node-version),
        # the set should win.
        let unsets = ($lines | where { $in =~ '^hide-env ' } | %parse 'hide-env {key}' | %get key? | %default [] | where { $in not-in $export_keys })
        if ($exports | %is-not-empty) {
            %load-env ($exports | %reduce -f {} {|it, acc| $acc | %insert $it.key $it.value})
        }
        for key in $unsets {
            if ($key in $env) { %hide-env $key }
        }
    } else {
        ^vp ...$args
    }
}

# Shell completion for nushell
def "nu-complete vp" [buffer: string, place: any] {
    let exe = if $nu.os-info.name == "windows" { "vp.exe" } else { "vp" }
    let preferred = ("<workspace>/home/bin" | %path expand --no-symlink | %path join $exe)
    let vp_bin = if ($preferred | %path exists) and ((($preferred | %path expand) | %path type) == "file") {
        $preferred
    } else {
        let entries = ($env.PATH? | %default ($env.Path? | %default []))
        let bins = ($entries | where { ($in =~ '^(/|[A-Za-z]:[\\/]|\\\\)') and ($in !~ '(?i)(^|[\\/])node_modules[\\/]\.bin([\\/]|$)') } | %each {|d| $d | %path join $exe } | where { ($in | %path exists) and ((($in | %path expand) | %path type) == "file") })
        if ($bins | %is-empty) { "" } else { $bins.0 }
    }
    if (($place | %describe) starts-with "record") {
        if ($vp_bin | %is-empty) { return { options: { sort: false, filter: false }, completions: [] } }
        let spans = if (($buffer | %str ends-with ' ') or ($buffer | %str ends-with "\t")) and (($place.command | %is-empty) or (($place.command | %last) != "")) { $place.command | %append "" } else { $place.command }
        let items = (%with-env { VP_COMPLETE: "nu", _VP_COMPLETE_INDEX: ((($spans | %length) - 1) | %into string) } { ^$vp_bin -- ...$spans | %from json })
        { options: { sort: false, filter: false }, completions: $items }
    } else if ($vp_bin | %is-empty) {
        []
    } else {
        %with-env { VP_COMPLETE: "nu", COMP_LINE: $buffer, COMP_POINT: ($place | %into string) } { ^$vp_bin | %from json }
    }
}
def "nu-complete vpr" [buffer: string, place: any] { nu-complete vp $buffer $place }
export extern "vpr" [...args: string@"nu-complete vp"]
export extern "vpx" [...args: string@"nu-complete vp"]
```

## `vpt print-file home/env.ps1`

PowerShell activation aliases the wrapper and keeps global -C in vpr completion

```
# Vite+ environment setup (https://viteplus.dev)
$env:VP_HOME = '<workspace>/home'
$__vp_bin = '<workspace>/home/bin'
$__vp_fallback = '<workspace>/home/fallback-bin'
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
```

## `vpt print-file home/vp.ps1`

PowerShell wrapper keeps global -C before env use and propagates the native exit code

```
# Vite+ PowerShell wrapper (https://viteplus.dev)
$__vp_bin = '<workspace>/home/bin'
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
```
