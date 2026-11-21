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
