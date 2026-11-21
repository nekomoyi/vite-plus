#!/bin/sh
# Vite+ environment setup (https://viteplus.dev)
__ENV_EXPORTS____vp_bin="__VP_BIN__"
__vp_fallback="__VP_FALLBACK_BIN__"
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
    eval "$(VP_COMPLETE=bash command vp)"
elif [ -n "${ZSH_VERSION-}" ] && type compdef >/dev/null 2>&1; then
    eval "$(VP_COMPLETE=zsh command vp)"
    eval '
    _vpr_complete() {
        local -a orig=("${words[@]}")
        if [[ "${orig[2]-}" == "-C" ]]; then
            if (( ${#orig[@]} >= 4 )); then
                words=("vp" "-C" "${orig[3]}" "run" "${orig[@]:3}")
                if (( CURRENT >= 4 )); then
                    CURRENT=$((CURRENT + 1))
                fi
            else
                words=("vp" "${orig[@]:1}")
            fi
        elif [[ "${orig[2]-}" == -C?* ]]; then
            if (( ${#orig[@]} >= 3 )); then
                words=("vp" "${orig[2]}" "run" "${orig[@]:2}")
                if (( CURRENT >= 3 )); then
                    CURRENT=$((CURRENT + 1))
                fi
            else
                words=("vp" "${orig[@]:1}")
            fi
        else
            words=("vp" "run" "${orig[@]:1}")
            if (( CURRENT >= 2 )); then
                CURRENT=$((CURRENT + 1))
            fi
        fi
        ${=_comps[vp]}
    }
    compdef _vpr_complete vpr
    '
fi
