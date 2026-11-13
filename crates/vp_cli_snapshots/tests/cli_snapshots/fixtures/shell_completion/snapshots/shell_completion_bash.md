# shell_completion_bash

## `VP_HOME=${workspace}/home vp env setup --refresh`


## `bash --noprofile --norc assert_bash.sh`

verifies Bash completion across aliases, -C tasks, colon tasks, deps, workspace filters, flags, and vpx scripts

```
vp un -> un uninstall unlink
vp -C packages/subpkg run  -> sub:task
vpr build: -> watch
vp remove  -> @demo/subpkg fixture-shell-completion react
vp run --filter  -> @demo/subpkg fixture-shell-completion
vp check -- -> --continue-on-error --fix --fix-dangerously --quiet --deny-warnings --no-fmt --no-lint --no-error-on-unmatched-pattern --help
vpx -C custom ./scripts/ -> ./scripts/seed.ts
Bash completion assertions passed
```
