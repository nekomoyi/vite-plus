# Vite+ environment setup (https://viteplus.dev)
__ENV_EXPORTS__while set -l __vp_idx (builtin contains -i -- "__VP_BIN__" $PATH)
    set -e PATH[$__vp_idx]
end
while set -l __vp_idx (builtin contains -i -- "__VP_FALLBACK_BIN__" $PATH)
    set -e PATH[$__vp_idx]
end
set -gx PATH "__VP_BIN__" $PATH "__VP_FALLBACK_BIN__"

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
