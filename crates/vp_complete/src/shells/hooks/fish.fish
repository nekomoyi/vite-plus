function __vp_complete
    set -l tokens
    if not set tokens (builtin commandline -xpc 2>/dev/null)
        set tokens (builtin commandline -opc)
        if builtin string match -qr '^~(/|$)' -- $tokens
            # Older Fish removes quotes with -o. Its tokenizer decodes a hex escape
            # only outside quotes, so the probe identifies bare tildes without eval.
            set -l line (builtin commandline -pc | builtin string collect)
            set -l probe (builtin string replace -a '~' '\x7e' -- "$line")
            set -l marked (builtin commandline -opc --input "$probe")
            set -l i 1
            for word in $tokens
                if builtin string match -qr '^~(/|$)' -- "$word"; and builtin string match -qr '^~(/|$)' -- "$marked[$i]"
                    set tokens[$i] "$HOME"(builtin string sub -s 2 -- "$word")
                end
                set i (builtin math $i + 1)
            end
        end
    end
    set -l cur (builtin commandline -ct)
    set -l bin ""
    if not builtin test -f "$bin" -a -x "$bin"
        set bin ""
        for dir in $PATH
            builtin string match -qr '^(/|[A-Za-z]:[\\\\/])' -- "$dir"; or continue
            builtin string match -qir '(^|[\\\\/])node_modules[\\\\/]\.bin([\\\\/]|$)' -- "$dir"; and continue
            for exe in "$dir/vp" "$dir/vp.exe"
                if builtin test -f "$exe" -a -x "$exe"
                    set bin "$exe"
                    break
                end
            end
            builtin test -n "$bin"; and break
        end
    end
    builtin test -n "$bin"; and VP_COMPLETE=fish _VP_COMPLETE_INDEX=(builtin count $tokens) command "$bin" -- $tokens $cur
end

builtin complete -c vp --keep-order -f -a "(__vp_complete)"
builtin complete -c vpr --keep-order -f -a "(__vp_complete)"
builtin complete -c vpx --keep-order -f -a "(__vp_complete)"
