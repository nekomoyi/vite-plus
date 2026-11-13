let lines = [
  "vp un",
  "vp -C packages/subpkg run ",
  "vpr build:",
  "vp remove ",
  "vp run --filter ",
  "vp check --",
  "vpx -C custom ./scripts/",
]
let env_nu = ($env.VP_HOME | path join "env.nu")
let script = $"source '($env_nu)'
($lines | to nuon) | each {|line| $line | commandline complete } | to json --raw"
let result = ^$nu.current-exe -n -c $script | complete
if $result.exit_code != 0 {
  error make { msg: $"Nushell completion failed: ($result.stderr)" }
}
if ($result.stderr | str contains "deprecated") {
  error make { msg: $"Nushell completion emitted deprecation warning: ($result.stderr)" }
}

let completions = ($result.stdout | from json)
let expected_pairs = [
  [0 "uninstall"]
  [1 "sub:task"]
  [2 "build:watch"]
  [3 "react"]
  [4 "@demo/subpkg"]
  [5 "--fix-dangerously"]
  [6 "./scripts/seed.ts"]
]
for pair in $expected_pairs {
  let values = ($completions | get $pair.0 | each {|v| $v | str trim })
  if $pair.1 not-in $values {
    error make { msg: $"`($lines | get $pair.0)` missing `($pair.1)`, got: ($values)" }
  }
}

print "Nushell completion assertions passed"
