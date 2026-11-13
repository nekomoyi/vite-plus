# Vite+ uses an in-tree completion engine (`vp_complete`), so completion does not
# depend on Fish being installed.
let lines = ["vp "]
let path = $env.PATH | where { not ($in | path join fish | path exists) }
let script = $"source env.nu
($lines | to nuon) | each {|line| $line | commandline complete } | to json --raw"
let result = with-env { PATH: $path } { ^$nu.current-exe -n -c $script | complete }
if $result.exit_code != 0 {
  error make { msg: $"completion failed: ($result.stderr)" }
}

let completions = $result.stdout | from json
let values = $completions | get 0 | each {|value| $value | str trim }
if "env" not-in $values {
  error make { msg: $"`vp ` did not complete `env` without fish, got: ($values)" }
}
if "install" not-in $values {
  error make { msg: $"`vp ` did not complete `install` without fish, got: ($values)" }
}

print "Nushell completer checks without fish passed"
