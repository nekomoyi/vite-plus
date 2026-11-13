# Vite+ environment setup (https://viteplus.dev)
__ENV_EXPORTS__$env.PATH = ($env.PATH | where { $in != "__VP_BIN__" and $in != "__VP_FALLBACK_BIN__" } | %prepend "__VP_BIN__" | %append "__VP_FALLBACK_BIN__")

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
    let preferred = ("__VP_BIN__" | %path expand --no-symlink | %path join $exe)
    let vp_bin = if ($preferred | %path exists) and ((($preferred | %path expand) | %path type) == "file") {
        $preferred
    } else {
        let entries = ($env.PATH? | %default ($env.Path? | %default []))
        let bins = ($entries | where { ($in =~ '^(/|[A-Za-z]:[\\/]|\\\\)') and ($in !~ '(?i)(^|[\\/])node_modules[\\/]\.bin([\\/]|$)') } | %each {|d| $d | %path join $exe } | where { ($in | %path exists) and ((($in | %path expand) | %path type) == "file") })
        if ($bins | %is-empty) { "" } else { $bins.0 }
    }
    if (($place | %describe) starts-with "record") {
        if ($vp_bin | %is-empty) { return { options: { sort: false, filter: false }, completions: [] } }
        let trailing_empty = if (($place.target? | %describe) starts-with "record") { $place.target.start == $place.target.end } else { ($buffer | %str ends-with ' ') or ($buffer | %str ends-with "\t") }
        let spans = if $trailing_empty and (($place.command | %is-empty) or (($place.command | %last) != "")) { $place.command | %append "" } else { $place.command }
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
