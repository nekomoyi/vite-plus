#!/bin/bash
set -e

# Source the generated environment setup
source home/env

test_complete() {
    local cmd="$1"
    COMP_LINE="$cmd"
    COMP_POINT=${#COMP_LINE}
    eval "COMP_WORDS=($cmd)"
    if [[ "$cmd" == *" " ]]; then
        COMP_WORDS+=("")
    fi
    COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
    COMPREPLY=()
    _vp_complete
    echo "${COMPREPLY[*]:-}"
}

# 1. vp un
res_un=$(test_complete "vp un")
echo "vp un -> $res_un"
echo "$res_un" | grep -q "uninstall"
echo "$res_un" | grep -q "unlink"
echo "$res_un" | grep -q "un"

# 2. vp -C packages/subpkg run
res_sub=$(test_complete "vp -C packages/subpkg run ")
echo "vp -C packages/subpkg run  -> $res_sub"
echo "$res_sub" | grep -q "sub:task"

# 3. vpr build:
res_build=$(test_complete "vpr build:")
echo "vpr build: -> $res_build"
echo "$res_build" | grep -q "watch"

# 4. vp remove
res_remove=$(test_complete "vp remove ")
echo "vp remove  -> $res_remove"
echo "$res_remove" | grep -q "react"

# 5. vp run --filter
res_filter=$(test_complete "vp run --filter ")
echo "vp run --filter  -> $res_filter"
echo "$res_filter" | grep -q "@demo/subpkg"

# 6. vp check --
res_check=$(test_complete "vp check --")
echo "vp check -- -> $res_check"
echo "$res_check" | grep -q -- "--fix-dangerously"

# 7. vpx -C custom ./scripts/
res_vpx=$(test_complete "vpx -C custom ./scripts/")
echo "vpx -C custom ./scripts/ -> $res_vpx"
echo "$res_vpx" | grep -q "./scripts/seed.ts"

echo "Bash completion assertions passed"
