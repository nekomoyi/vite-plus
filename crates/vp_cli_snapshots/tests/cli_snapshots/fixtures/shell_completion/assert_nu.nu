let lines = [
  "vp un",
  "vp -C packages/subpkg run ",
  "vpr build:",
  "vp remove ",
  "vp run --filter ",
  "vp check --",
  "vpx -C custom ./scripts/",
  "vp fo",
  "vp i",
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
let alias_values = ($completions | first | each {|v| $v | str trim } | sort)
if $alias_values != ["un", "uninstall", "unlink"] {
  error make { msg: $"vp un must offer each matching spelling, got: ($alias_values)" }
}
let install_values = ($completions | last | each {|v| $v | str trim })
if "install" not-in $install_values or "i" in $install_values {
  error make { msg: $"vp i should prefer install, got: ($install_values)" }
}
let expected_pairs = [
  [1 "sub:task"]
  [2 "build:watch"]
  [3 "react"]
  [4 "@demo/subpkg"]
  [5 "--fix-dangerously"]
  [6 "./scripts/seed.ts"]
  [7 "format"]
]
for pair in $expected_pairs {
  let values = ($completions | get $pair.0 | each {|v| $v | str trim })
  if $pair.1 not-in $values {
    error make { msg: $"`($lines | get $pair.0)` missing `($pair.1)`, got: ($values)" }
  }
}

print "Nushell completion assertions passed"
