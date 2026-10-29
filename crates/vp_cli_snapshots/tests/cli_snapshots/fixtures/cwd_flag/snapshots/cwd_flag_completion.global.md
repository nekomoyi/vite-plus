# cwd_flag_completion

Dynamic shell completion for -C suggests directories when completing the flag
value and resolves cwd-dependent completions (such as `vp run` tasks) from the
target directory once the cursor moves past -C.

## `vpt write-file node_modules/vite-plus/package.json '{"name":"vite-plus","version":"0.0.0"}
'`


## `vpt write-file node_modules/vite-plus/dist/bin.js 'const fs = require('\''node:fs'\''); const pkg = JSON.parse(fs.readFileSync('\''package.json'\'', '\''utf8'\'')); for (const [k, v] of Object.entries(pkg.scripts || {})) console.log(`${k}: ${v}`);
'`


## `VP_COMPLETE=fish vp -- vp -C pack`

completing -C suggests directories from the current working directory

```
packages/
```

## `VP_COMPLETE=fish vp -- vp run w`

without -C, run completion resolves tasks from the workspace root

```
where:hello
```

## `VP_COMPLETE=fish vp -- vp -C packages/hello run w`

-C <dir> switches directory before resolving run task completions

```
where
```

## `VP_COMPLETE=fish vp -- vp -C=packages/hello run w`

-C=<dir> inline form also switches directory for completion

```
where
```

## `VP_COMPLETE=fish vp -- vp -Cpackages/hello run w`

-C<dir> attached form also switches directory for completion

```
where
```

## `cd packages/hello && VP_COMPLETE=fish vp -- vp run w`

equivalent cd form for run completion

```
where
```
