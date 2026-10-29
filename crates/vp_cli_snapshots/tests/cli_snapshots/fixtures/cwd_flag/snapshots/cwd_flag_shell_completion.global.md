# cwd_flag_shell_completion

Shell completion adapters for Bash, Zsh, and Fish expand shell-local variables
in -C <dir>, support `vpr -C` in Bash, and preserve single-quoted literal paths
in Fish.

## `vpt write-file node_modules/vite-plus/package.json '{"name":"vite-plus","version":"0.0.0"}
'`


## `vpt write-file node_modules/vite-plus/dist/bin.js 'const fs = require('\''node:fs'\''); const pkg = JSON.parse(fs.readFileSync('\''package.json'\'', '\''utf8'\'')); for (const [k, v] of Object.entries(pkg.scripts || {})) console.log(`${k}: ${v}`);
'`


## `vpt write-file ~/literal/package.json '{"name":"literal-tilde","scripts":{"where:tilde":"echo tilde"}}
'`


## `bash --noprofile --norc -c 'eval "$(VP_COMPLETE=bash vp)"; PROJECT=packages/hello; COMP_WORDS=(vpr -C '\''"$PROJECT"'\'' w); COMP_CWORD=3; COMP_LINE='\''vpr -C "$PROJECT" w'\''; COMP_POINT=${#COMP_LINE}; _clap_complete_vpr; printf '\''bash vpr -C $PROJECT: %s\n'\'' "${COMPREPLY[*]}"; COMP_WORDS=(vpr -C pack); COMP_CWORD=2; COMP_LINE='\''vpr -C pack'\''; COMP_POINT=${#COMP_LINE}; _clap_complete_vpr; printf '\''bash vpr -C <dir>: %s\n'\'' "${COMPREPLY[*]}"'`

Bash expands unexported local $PROJECT and completes both vpr -C <dir> and tasks in <dir>

```
bash vpr -C $PROJECT: where
bash vpr -C <dir>: packages/
```

## `zsh -f -c 'compdef() { : }; eval "$(VP_COMPLETE=zsh vp)"; _describe() { local -a arr; eval "arr=(\"\${${3}[@]}\")"; print -r -- "zsh vp -C \$PROJECT: ${arr[*]}"; }; PROJECT=packages/hello; words=(vp -C '\''"$PROJECT"'\'' run w); CURRENT=5; _clap_dynamic_completer_vp'`

Zsh expands unexported local $PROJECT before invoking vp dynamic completion

```
zsh vp -C $PROJECT: where
```

## `fish --no-config -c 'VP_COMPLETE=fish vp | source; set -l PROJECT packages/hello; printf '\''fish vp -C $PROJECT: %s\n'\'' (complete -C '\''vp -C "$PROJECT" run w'\''); printf '\''fish vp -C literal tilde: %s\n'\'' (complete -C "vp -C '\''~/literal'\'' run w")'`

Fish expands local $PROJECT and preserves single-quoted '~/literal' without home expansion

```
fish vp -C $PROJECT: where
fish vp -C literal tilde: where:tilde
```
