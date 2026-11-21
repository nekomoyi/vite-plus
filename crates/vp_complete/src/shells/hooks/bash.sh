_vp_complete() {
    local IFS=$'\n' _vp_shopts="$-" _bin=""
    set -f
    if [ ! -f "$_bin" ] || [ ! -x "$_bin" ]; then
        _bin=""
        local _d _nc=0
        shopt -q nocasematch && _nc=1
        shopt -s nocasematch
        IFS=:
        for _d in $PATH; do
            case "$_d" in /*|[A-Za-z]:/*|[A-Za-z]:\\*) ;; *) continue ;; esac
            case "/${_d//\\//}/" in */node_modules/.bin/*) continue ;; esac
            if [ -f "$_d/vp" ] && [ -x "$_d/vp" ]; then _bin="$_d/vp"; break; fi
            if [ -f "$_d/vp.exe" ] && [ -x "$_d/vp.exe" ]; then _bin="$_d/vp.exe"; break; fi
        done
        (( _nc )) || shopt -u nocasematch
        IFS=$'\n'
    fi
    COMPREPLY=()
    if [ -n "$_bin" ]; then
        local _line _out
        _out="$(_VP_COMP_LINE="$COMP_LINE" _VP_COMP_POINT="$COMP_POINT" _VP_COMP_WORDBREAKS="$COMP_WORDBREAKS" _VP_BASH_V="${BASH_VERSINFO[0]-}" VP_COMPLETE=bash "$_bin" -- 2>/dev/null)"
        if [ -n "$_out" ]; then
            while IFS= read -r _line; do COMPREPLY+=("$_line"); done <<< "$_out"
        fi
    fi
    local _dir=0 _n=${#COMPREPLY[@]}
    if (( _n > 0 )) && [[ "${COMPREPLY[$((_n - 1))]}" == :* ]]; then
        _dir="${COMPREPLY[$((_n - 1))]#:}"
        unset "COMPREPLY[$((_n - 1))]"
    fi
    if type compopt >/dev/null 2>&1; then
        (( _dir & 2 )) && compopt -o nospace
        (( (_dir & 4) == 0 && ${#COMPREPLY[@]} == 0 )) && compopt -o default
    elif (( ${#COMPREPLY[@]} == 1 && (_dir & 66) == 0 )); then
        COMPREPLY=("${COMPREPLY[0]} ")
    fi
    [[ "$_vp_shopts" == *f* ]] || set +f
}

if type compopt >/dev/null 2>&1; then
    complete -F _vp_complete vp vpr vpx
else
    complete -o nospace -F _vp_complete vp vpr vpx
fi
