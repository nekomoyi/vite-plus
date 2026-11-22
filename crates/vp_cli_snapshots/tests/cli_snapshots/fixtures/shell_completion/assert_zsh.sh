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
record_tab() { zle menu-complete; print -r -- "CHOICE=<${BUFFER% }>"; }
zle -N record_tab
bindkey "^G" list-choices
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
run_tab_case "vp f" "CAPTURED: vp f"
run_tab_case "vp fo" "CAPTURED: vp format"
run_tab_case "vp -C packages/subpkg run sub:" "CAPTURED: vp -C packages/subpkg run sub:task"
run_tab_case "vpr build:" "CAPTURED: vpr build:watch"
run_tab_case "vp check --fix-d" "CAPTURED: vp check --fix-dangerously"
run_tab_case "vpx ./scripts/se" "CAPTURED: vpx ./scripts/seed.ts"

# List and cycle the same native menu as a user typing vp i.
zpty -w pty 'stty cols 150; zstyle -d ":completion:*" menu; bindkey "^I" record_tab'
zpty -r pty out '*READY> *'
zpty -w -n pty $'vp i\x07'
menu=""
for n in {1..150}; do
  while zpty -r -t pty chunk; do menu+="$chunk"; done
  [[ "$menu" == *'-- Install'* ]] && break
  sleep 0.02
done
[[ "$menu" =~ '(install[[:blank:]]+i|i[[:blank:]]+install)[[:blank:]]+-- Install' ]] || {
  print -u2 -r -- "Unexpected menu: $menu"; exit 1
}
choices=""
for n in {1..5}; do
  zpty -w -n pty $'\t'
  zpty -r pty chunk '*CHOICE=<*>*'
  choices+="$chunk"
done
[[ "$choices" == *'CHOICE=<vp install>'* && "$choices" == *'CHOICE=<vp i>'* ]] || {
  print -u2 -r -- "Unexpected selections: $choices"; exit 1
}
echo "vp i -> grouped menu and both Tab selections"

zpty -d pty || true
echo "Zsh completion assertions passed"
