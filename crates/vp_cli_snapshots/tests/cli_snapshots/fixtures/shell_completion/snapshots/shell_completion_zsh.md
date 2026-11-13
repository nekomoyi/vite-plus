# shell_completion_zsh

## `vp env setup --refresh`


## `zsh -f assert_zsh.sh`

verifies real interactive Zsh Tab completion via zpty across aliases, -C tasks, flags, and script paths

```
vp un -> CAPTURED: vp un
vp -C packages/subpkg run sub: -> CAPTURED: vp -C packages/subpkg run sub:task
vpr build: -> CAPTURED: vpr build:watch
vp check --fix-d -> CAPTURED: vp check --fix-dangerously
vpx ./scripts/se -> CAPTURED: vpx ./scripts/seed.ts
Zsh completion assertions passed
```
