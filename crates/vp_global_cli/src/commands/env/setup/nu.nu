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

# Shell completion for nushell (delegates to fish completions dynamically)
# Nushell 0.116+ binds `buffer` and `place` by name: `buffer` is the whole line up to the cursor,
# and `place.command` is the words of the current command up to the cursor. Older releases pass
# the current command in `buffer` and the cursor position in `place`. See "Unified custom
# completions" in https://www.nushell.sh/blog/2026-09-26-nushell_v0_116_0.html#unified-custom-completions
# `place` is `any` because it is an int or a record. `oneof<int, record>` needs Nushell 0.105+,
# and every Nushell release must parse the legacy env.nu.
def "nu-complete vp current-command" [buffer: string, place: any] {
    if (($place | %describe) starts-with "record") { $place.command | %str join ' ' } else { $buffer }
}
# Fish gets the command as an argument, so quotes in it cannot end a fish string.
# Without fish, return null so Nushell falls back to file completion. Nushell 0.116+ shows no
# suggestions when a parameter completer fails (#3272).
def "nu-complete vp fish" [command: string] {
    if (%which fish | %is-empty) { return null }
    let fish_script = 'VP_COMPLETE=fish command vp | source; complete --do-complete=$argv[1]'
    ^fish --command $fish_script $command | %from tsv --flexible --noheaders --no-infer | %rename value description | %update value {|row|
        let value = $row.value
        let need_quote = ['\' ',' '[' ']' '(' ')' ' ' '\t' "'" '"' "`"] | %any {$in in $value}
        if ($need_quote and ($value | %path exists)) {
            let expanded_path = if ($value starts-with ~) {$value | %path expand --no-symlink} else {$value}
            $'"($expanded_path | %str replace --all "\"" "\\\"")"'
        } else {$value}
    }
}
def "nu-complete vp" [buffer: string, place: any] {
    nu-complete vp fish (nu-complete vp current-command $buffer $place)
}
# Completion logic for vpr (translates context to 'vp run ...')
def "nu-complete vpr" [buffer: string, place: any] {
    let context = nu-complete vp current-command $buffer $place
    let modified_context = if ($context =~ '^vpr(?<cwd>\s+-C\s+(?:"[^"]*"|\x27[^\x27]*\x27|\S+))\s') {
        $context | %str replace -r '^vpr(?<cwd>\s+-C\s+(?:"[^"]*"|\x27[^\x27]*\x27|\S+))\s' 'vp$cwd run '
    } else if ($context =~ '^vpr(?<cwd>\s+-C=?(?:"[^"]*"|\x27[^\x27]*\x27|\S+))\s') {
        $context | %str replace -r '^vpr(?<cwd>\s+-C=?(?:"[^"]*"|\x27[^\x27]*\x27|\S+))\s' 'vp$cwd run '
    } else if ($context =~ '^vpr\s+-C') {
        $context | %str replace -r '^vpr' 'vp'
    } else {
        $context | %str replace -r '^vpr' 'vp run'
    }
    nu-complete vp fish $modified_context
}
export extern "vpr" [...args: string@"nu-complete vpr"]
