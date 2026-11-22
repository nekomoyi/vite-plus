# shell_completion_zsh

## `vp env setup --refresh`


## `zsh -f assert_zsh.sh`

verifies real interactive Zsh Tab completion via zpty across aliases, -C tasks, flags, and script paths

```
vp un -> CAPTURED: vp un
vp f -> CAPTURED: vp f
vp fo -> CAPTURED: vp format
vp -C packages/subpkg run sub: -> CAPTURED: vp -C packages/subpkg run sub:task
vpr build: -> CAPTURED: vpr build:watch
vp check --fix-d -> CAPTURED: vp check --fix-dangerously
vpx ./scripts/se -> CAPTURED: vpx ./scripts/seed.ts
vp i -> grouped menu and both Tab selections
Zsh completion assertions passed
```
