_vp_compadd() {
  local -a _v=() _expl
  (( $1 )) && _v=(-V completions)
  shift
  if (( $+functions[_wanted] )); then
    _wanted "${_v[@]}" completions _expl 'completions' compadd "$@"
  elif (( $+builtins[compadd] || $+functions[compadd] )); then
    compadd "${_v[@]}" "$@"
  fi
}

_vp_complete_files() {
  local _mode="$1" _pfx="$2" _base="$3" _exts="$4"
  [[ -n "$_pfx" ]] && (( $+builtins[compset] )) && compset -P "${(b)_pfx}"
  local _ip="${IPREFIX-}" _hesc="${HOME%/}"
  if [[ -n "$_pfx" && -n "${HOME-}" ]]; then
    case "${compstate[quote]-}" in
      \") _hesc="${${(qqq)_hesc}:1:-1}" ;;
      \') _hesc="${${(qq)_hesc}:1:-1}" ;;
      *)  _hesc="${(q)_hesc}" ;;
    esac
  fi
  if (( $+builtins[compset] && $+builtins[compadd] )) && [[ "${PREFIX-}" == \~ && -z "${SUFFIX-}" && -d "${HOME-}" ]]; then
    compset -P '\~'
    [[ -n "$_pfx" ]] && IPREFIX="${_ip}${_hesc}"
    compadd -Q -qS / -- ""
    return 0
  fi
  local -a _vp_w=() _wargs=()
  if [[ "${PREFIX-}" == \~/* ]] && (( $+builtins[compset] )) && compset -P '\~/'; then
    if [[ -n "${HOME-}" ]]; then
      [[ -n "$_pfx" ]] && IPREFIX="${_ip}${_hesc}/"
      _vp_w=("$HOME")
      _wargs=(-W _vp_w)
      _base="$HOME"
    fi
  elif [[ -n "$_base" && "${PREFIX-}" != /* ]]; then
    _vp_w=("$_base")
    _wargs=(-W _vp_w)
  fi
  local _ret=1
  case "$_mode" in
    dir) _files "${_wargs[@]}" -/ && _ret=0 ;;
    ext) _files "${_wargs[@]}" -g "*.(${_exts})(-.)" && _ret=0 ;;
    *)   _files "${_wargs[@]}" && _ret=0 ;;
  esac
  if (( _ret )) && (( $+builtins[compadd] )) && [[ "${PREFIX-}" == (.|..|*/.|*/..) ]]; then
    [[ -d "${_base:+$_base/}${PREFIX}" ]] && compadd -Q -qS / -- "$PREFIX" && _ret=0
  fi
  return $_ret
}

_vp_complete() {
  local _bin=""
  if [[ ! -f "$_bin" || ! -x "$_bin" ]]; then
    _bin=""
    local _d
    for _d in "${path[@]}"; do
      [[ "$_d" == /* || "$_d" == [A-Za-z]:[\\/]* ]] || continue
      [[ "/${${(L)_d}//\\//}/" == */node_modules/.bin/* ]] && continue
      if [[ -f "$_d/vp" && -x "$_d/vp" ]]; then _bin="$_d/vp"; break; fi
      if [[ -f "$_d/vp.exe" && -x "$_d/vp.exe" ]]; then _bin="$_d/vp.exe"; break; fi
    done
  fi
  [[ -n "$_bin" ]] || return 0

  local _q="" _eg="" _raw _directive=0
  local -a _vp_lines=()
  (( $+compstate )) && _q="${compstate[quote]-}"
  (( $+_comp_caller_options )) && _eg="${_comp_caller_options[extendedglob]-}"
  _raw="$(VP_COMPLETE=zsh _VP_COMPLETE_INDEX=$((CURRENT - 1)) _VP_COMP_QUOTE="$_q" _VP_EXTGLOB="$_eg" "$_bin" -- "${(@)words[1,$CURRENT]}" 2>/dev/null)"

  if [[ -n "$_raw" ]]; then
    _vp_lines=("${(@f)_raw}")
    if (( ${#_vp_lines} > 0 )) && [[ "${_vp_lines[-1]}" == :* ]]; then
      _directive="${_vp_lines[-1]#:}"
      _vp_lines[-1]=()
    fi
  fi

  if (( ${#_vp_lines} > 0 )) && [[ "${_vp_lines[1]}" == "__vp_files" ]]; then
    local _f_mode="any" _f_pfx="" _f_base="" _f_exts="" _line
    for _line in "${_vp_lines[@]:1}"; do
      case "$_line" in
        m:*) _f_mode="${_line#m:}" ;;
        p:*) _f_pfx="${_line#p:}" ;;
        d:*) _f_base="${_line#d:}" ;;
        g:*) _f_exts="${_line#g:}" ;;
      esac
    done
    _vp_complete_files "$_f_mode" "$_f_pfx" "$_f_base" "$_f_exts"
    return
  fi

  if (( ${#_vp_lines} > 0 )); then
    local -a _v_mats=() _v_disp=() _d_mats=() _d_disp=() _qopt=()
    local _has_v_desc=0 _has_d_desc=0 _keep=$(( (_directive & 32) != 0 )) _entry _kind _rest
    [[ -z "$_q" ]] && _qopt=(-Q)
    for _entry in "${_vp_lines[@]}"; do
      _kind="${_entry%%$'\t'*}"
      _rest="${_entry#*$'\t'}"
      case "$_kind" in
        d) _d_mats+=("${_rest%%$'\t'*}"); _d_disp+=("${_rest#*$'\t'}") ;;
        D) _d_mats+=("${_rest%%$'\t'*}"); _d_disp+=("${_rest#*$'\t'}"); _has_d_desc=1 ;;
        V) _v_mats+=("${_rest%%$'\t'*}"); _v_disp+=("${_rest#*$'\t'}"); _has_v_desc=1 ;;
        *) _v_mats+=("${_rest%%$'\t'*}"); _v_disp+=("${_rest#*$'\t'}") ;;
      esac
    done
    if (( ${#_v_mats} > 0 )); then
      local -a _v_opts=("${_qopt[@]}")
      (( ${#_d_mats} == 0 && (_directive & 2) )) && _v_opts+=(-S '')
      (( _has_v_desc )) && _v_opts+=(-l)
      _vp_compadd "$_keep" "${_v_opts[@]}" -d _v_disp -a _v_mats
    fi
    if (( ${#_d_mats} > 0 )); then
      local -a _d_opts=("${_qopt[@]}" -qS /)
      (( _has_d_desc )) && _d_opts+=(-l)
      _vp_compadd "$_keep" "${_d_opts[@]}" -d _d_disp -a _d_mats
    fi
  elif ! (( _directive & 4 )); then
    (( $+functions[_files] )) && _files
  fi
}

if (( $+functions[compdef] )); then
  compdef _vp_complete vp vpr vpx
else
  _vp_register_deferred() {
    if (( $+functions[compdef] )); then
      compdef _vp_complete vp vpr vpx
      add-zsh-hook -d precmd _vp_register_deferred 2>/dev/null || precmd_functions=(${precmd_functions:#_vp_register_deferred})
    fi
  }
  precmd_functions+=(_vp_register_deferred)
fi
