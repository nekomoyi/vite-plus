#!/bin/zsh
set -e
zmodload zsh/zpty

ZDOTDIR="$PWD/zsh_home"
mkdir -p "$ZDOTDIR"
cat > "$ZDOTDIR/.zshrc" <<'EOF'
PROMPT='READY> '
zstyle ':completion:*' menu no
autoload -Uz compinit
compinit -u
source "$VP_HOME/env"

capture_tab() {
  zle expand-or-complete
  print -r ""
  print -r "CAPTURED: $BUFFER"
  BUFFER=""
  zle reset-prompt
}
zle -N capture_tab
bindkey "^I" capture_tab
EOF

export ZDOTDIR
zpty pty zsh -f -o rcs -i
zpty -r pty out '*READY> *'

run_tab_case() {
  local typed="$1" expected="$2"
  zpty -w -n pty $'\x15'"${typed}"$'\t'
  local chunk=""
  zpty -r pty chunk '*READY> *' || { zpty -d pty; exit 1; }
  if [[ "$chunk" != *"$expected"* ]]; then
    print -u2 "Zsh completion mismatch for '$typed': expected '$expected', got:"
    print -u2 "$chunk"
    zpty -d pty
    exit 1
  fi
  echo "$typed -> $expected"
}

run_tab_case "vp un" "CAPTURED: vp un"
run_tab_case "vp -C packages/subpkg run sub:" "CAPTURED: vp -C packages/subpkg run sub:task"
run_tab_case "vpr build:" "CAPTURED: vpr build:watch"
run_tab_case "vp check --fix-d" "CAPTURED: vp check --fix-dangerously"
run_tab_case "vpx ./scripts/se" "CAPTURED: vpx ./scripts/seed.ts"

zpty -w -n pty $'exit 0\r'
zpty -d pty || true
echo "Zsh completion assertions passed"
