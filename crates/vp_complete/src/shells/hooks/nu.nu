def "nu-complete vp" [buffer: string, place: any] {
    let preferred = ""
    let vp_bin = if ($preferred != "" and ($preferred | path exists) and ((($preferred | path expand) | path type) == "file")) {
        $preferred
    } else {
        let entries = ($env.PATH? | default ($env.Path? | default []))
        let exe = if $nu.os-info.name == "windows" { "vp.exe" } else { "vp" }
        let bins = ($entries | where { ($in =~ '^(/|[A-Za-z]:[\\/]|\\\\)') and ($in !~ '(?i)(^|[\\/])node_modules[\\/]\.bin([\\/]|$)') } | each {|d| $d | path join $exe } | where { ($in | path exists) and ((($in | path expand) | path type) == "file") })
        if ($bins | is-empty) { "" } else { $bins.0 }
    }
    if (($place | describe) starts-with "record") {
        if ($vp_bin | is-empty) { return { options: { sort: false, filter: false }, completions: [] } }
        let trailing_empty = if (($place.target? | describe) starts-with "record") { $place.target.start == $place.target.end } else { ($buffer | str ends-with ' ') or ($buffer | str ends-with "\t") }
        let spans = if $trailing_empty and (($place.command | is-empty) or (($place.command | last) != "")) { $place.command | append "" } else { $place.command }
        let items = (with-env { VP_COMPLETE: "nu", _VP_COMPLETE_INDEX: ((($spans | length) - 1) | into string) } { ^$vp_bin -- ...$spans | from json })
        { options: { sort: false, filter: false }, completions: $items }
    } else if ($vp_bin | is-empty) {
        []
    } else {
        with-env { VP_COMPLETE: "nu", COMP_LINE: $buffer, COMP_POINT: ($place | into string) } { ^$vp_bin | from json }
    }
}
def "nu-complete vpr" [buffer: string, place: any] { nu-complete vp $buffer $place }
export extern "vp" [...args: string@"nu-complete vp"]
export extern "vpr" [...args: string@"nu-complete vp"]
export extern "vpx" [...args: string@"nu-complete vp"]
