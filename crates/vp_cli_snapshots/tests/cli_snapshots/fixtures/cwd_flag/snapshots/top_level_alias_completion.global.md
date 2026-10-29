# top_level_alias_completion

Dynamic shell completion includes top-level commands rewritten by normalize_args
(list/ls, rebuild, node, help), their flags/subcommands, and file path completion
for `vp -C <dir> node <TAB>`.

## `VP_COMPLETE=fish vp -- vp ls`

top-level ls alias is completed

```
ls      List installed packages
```

## `VP_COMPLETE=fish vp -- vp list --dep`

top-level list completes pm list flags

```
--depth Maximum depth of dependency tree
```

## `VP_COMPLETE=fish vp -- vp ls --dep`

top-level ls alias completes pm list flags

```
--depth Maximum depth of dependency tree
```

## `VP_COMPLETE=fish vp -- vp reb`

top-level rebuild command is completed

```
rebuild Rebuild native modules
```

## `VP_COMPLETE=fish vp -- vp no`

top-level node command is completed

```
node    Run plain Node.js (`env exec node`); `vpx <file>` runs TypeScript
```

## `VP_COMPLETE=fish vp -- vp help env u`

top-level help completes nested subcommands and visible aliases

```
unpin   Remove Node.js pins locally and package-manager pins at the workspace root
uni     Uninstall explicit Node.js or package-manager versions
use     Activate Node.js and package-manager versions for this shell session
```

## `VP_COMPLETE=fish vp -- vp -C packages/hello node `

vp -C <dir> node <TAB> completes paths from the target directory

```
.
package.json
src/
```
