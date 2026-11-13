function __vp_complete
    set -l tokens (builtin commandline -opc)
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
