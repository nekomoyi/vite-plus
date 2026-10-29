_clap_reassemble_words() {
    if [[ "$COMP_WORDBREAKS" != *:* ]]; then
        return
    fi
    local i j=0 line=$COMP_LINE
    words=()
    _CLAP_COMPLETE_INDEX=0
    for ((i = 0; i < ${#COMP_WORDS[@]}; i++)); do
        if ((i > 0 && j > 0)) && [[ "${COMP_WORDS[i]}" == :* || "${words[j-1]}" == *: ]] && [[ "$line" != [[:blank:]]* ]]; then
            words[j-1]="${words[j-1]}${COMP_WORDS[i]}"
        else
            words[j]="${COMP_WORDS[i]}"
            ((j++))
        fi
        if ((i == COMP_CWORD)); then
            _CLAP_COMPLETE_INDEX=$((j - 1))
        fi
        line=${line#*"${COMP_WORDS[i]}"}
    done
}

_clap_trim_completions() {
    local cur="${words[_CLAP_COMPLETE_INDEX]}"
    if [[ "$cur" != *:* || "$COMP_WORDBREAKS" != *:* ]]; then
        return
    fi
    local colon_word=${cur%"${cur##*:}"}
    local i=${#COMPREPLY[*]}
    while [[ $((--i)) -ge 0 ]]; do
        COMPREPLY[$i]=${COMPREPLY[$i]#"$colon_word"}
    done
}

_clap_normalize_split_chdir_equals() {
    if [[ "${words[1]-}" == "-C" && "${words[2]-}" == "=" && "${COMP_LINE-}" =~ ^[[:space:]]*[^[:space:]]+[[:space:]]+-C= ]]; then
        if (( ${#words[@]} == 3 )); then
            words=("${words[0]}" "-C" "")
            _CLAP_COMPLETE_INDEX=2
        else
            words=("${words[0]}" "-C" "${words[@]:3}")
            if (( _CLAP_COMPLETE_INDEX > 2 )); then
                _CLAP_COMPLETE_INDEX=$((_CLAP_COMPLETE_INDEX - 1))
            fi
        fi
    fi
}

_clap_translate_vpr_words() {
    local -a orig=("${words[@]}")
    if [[ "${orig[1]-}" == "-C" || "${orig[1]-}" == "\"-C\"" || "${orig[1]-}" == "'-C'" ]]; then
        if (( ${#orig[@]} >= 4 )); then
            words=("vp" "-C" "${orig[2]}" "run" "${orig[@]:3}")
            if (( _CLAP_COMPLETE_INDEX >= 3 )); then
                _CLAP_COMPLETE_INDEX=$((_CLAP_COMPLETE_INDEX + 1))
            fi
        else
            words=("vp" "${orig[@]:1}")
        fi
    elif [[ "${orig[1]-}" == -C?* || "${orig[1]-}" == [\"\']-C?* ]]; then
        if (( ${#orig[@]} >= 3 )); then
            words=("vp" "${orig[1]}" "run" "${orig[@]:2}")
            if (( _CLAP_COMPLETE_INDEX >= 2 )); then
                _CLAP_COMPLETE_INDEX=$((_CLAP_COMPLETE_INDEX + 1))
            fi
        else
            words=("vp" "${orig[@]:1}")
        fi
    else
        words=("vp" "run" "${orig[@]:1}")
        if (( _CLAP_COMPLETE_INDEX >= 1 )); then
            _CLAP_COMPLETE_INDEX=$((_CLAP_COMPLETE_INDEX + 1))
        fi
    fi
}

_clap_expand_chdir_word() {
    local raw=$1 expanded=$1
    if [[ "$raw" != *[\`\;\&\|\<\>\(\)$'\n']* ]]; then
        eval "expanded=$raw" 2>/dev/null || expanded=$1
    fi
    printf '%s' "$expanded"
}

_clap_expand_leading_chdir() {
    if [[ "${words[1]-}" == "-C" || "${words[1]-}" == "\"-C\"" || "${words[1]-}" == "'-C'" ]]; then
        if (( _CLAP_COMPLETE_INDEX > 2 )) && [[ -n "${words[2]+x}" ]]; then
            words[1]="-C"
            words[2]=$(_clap_expand_chdir_word "${words[2]}")
        fi
    elif [[ "${words[1]-}" == -C?* ]]; then
        if (( _CLAP_COMPLETE_INDEX > 1 )); then
            local rest=${words[1]#-C}
            rest=${rest#=}
            words[1]="-C=$(_clap_expand_chdir_word "$rest")"
        fi
    elif [[ "${words[1]-}" == [\"\']-C?* ]]; then
        if (( _CLAP_COMPLETE_INDEX > 1 )); then
            local expanded
            expanded=$(_clap_expand_chdir_word "${words[1]}")
            if [[ "$expanded" == "-C" ]]; then
                if (( _CLAP_COMPLETE_INDEX > 2 )) && [[ -n "${words[2]+x}" ]]; then
                    words[1]="-C"
                    words[2]=$(_clap_expand_chdir_word "${words[2]}")
                fi
            elif [[ "$expanded" == -C?* ]]; then
                words[1]=$expanded
            fi
        fi
    fi
}

_clap_complete_vp() {
    local _CLAP_COMPLETE_INDEX=${COMP_CWORD}
    local _CLAP_COMPLETE_COMP_TYPE=${COMP_TYPE}
    if compopt +o nospace 2> /dev/null; then
        local _CLAP_COMPLETE_SPACE=false
    else
        local _CLAP_COMPLETE_SPACE=true
    fi
    local words=("${COMP_WORDS[@]}")
    _clap_reassemble_words
    _clap_normalize_split_chdir_equals
    if [[ "${_CLAP_COMPLETE_IS_VPR-}" == 1 ]]; then
        _clap_translate_vpr_words
    fi
    _clap_expand_leading_chdir
    local IFS=$'\013'
    COMPREPLY=( $( \
        _CLAP_IFS="$IFS" \
        _CLAP_COMPLETE_INDEX="$_CLAP_COMPLETE_INDEX" \
        _CLAP_COMPLETE_COMP_TYPE="$_CLAP_COMPLETE_COMP_TYPE" \
        _CLAP_COMPLETE_SPACE="$_CLAP_COMPLETE_SPACE" \
        VP_COMPLETE="bash" \
        "vp" -- "${words[@]}" \
    ) )
    if [[ $? != 0 ]]; then
        unset COMPREPLY
    elif [[ $_CLAP_COMPLETE_SPACE == false ]] && [[ "${COMPREPLY-}" =~ [=/:]$ ]]; then
        compopt -o nospace
    fi
    _clap_trim_completions
}

_clap_complete_vpr() {
    local _CLAP_COMPLETE_IS_VPR=1
    _clap_complete_vp
}

if [[ "${BASH_VERSINFO[0]}" -eq 4 && "${BASH_VERSINFO[1]}" -ge 4 || "${BASH_VERSINFO[0]}" -gt 4 ]]; then
    complete -o nospace -o bashdefault -o nosort -F _clap_complete_vp vp
    complete -o nospace -o bashdefault -o nosort -F _clap_complete_vpr vpr
else
    complete -o nospace -o bashdefault -F _clap_complete_vp vp
    complete -o nospace -o bashdefault -F _clap_complete_vpr vpr
fi
