# User functions that shadow the built-ins env.fish uses (#3128). fish runs a function before
# the built-in with the same name, so env.fish must call the built-ins with `builtin`.
# This script calls the shadowed names only with `builtin`.
function count
    echo 0
end
function math
    echo 99
end
# Always false: an always-true `contains` would make the PATH cleanup loop in env.fish endless.
function contains
    return 1
end
function complete
end
function commandline
end
function source
end

builtin source env.fish
or begin
    echo "failed to source env.fish"
    exit 1
end

set -gx VP_NODE_VERSION 18.20.0
vp env use --help >/dev/null
or begin
    echo "vp env use --help failed through the Fish wrapper"
    exit 1
end
if test "$VP_NODE_VERSION" != "18.20.0"
    echo "vp env use --help changed VP_NODE_VERSION"
    exit 1
end

vp env use 20.20.2 --no-install
if test "$VP_NODE_VERSION" != "20.20.2"
    echo "vp env use did not set VP_NODE_VERSION, got '$VP_NODE_VERSION'"
    exit 1
end

vp -C . env use 22.23.4 --no-install
if test "$VP_NODE_VERSION" != "22.23.4"
    echo "vp -C . env use did not set VP_NODE_VERSION, got '$VP_NODE_VERSION'"
    exit 1
end

# Shell completion is hardened: vp_complete registers handlers with `builtin complete`
# and calls `builtin commandline`, so shadowed user functions do not affect it.
set -l vp_candidates (builtin complete -C "vp in" | string split -f1 \t)
if not builtin contains -- install $vp_candidates
    echo "vp completion did not suggest install, got: $vp_candidates"
    exit 1
end

set -l vpr_candidates (builtin complete -C "vpr b" | string split -f1 \t)
if not builtin contains -- build $vpr_candidates
    echo "vpr completion did not suggest build, got: $vpr_candidates"
    exit 1
end

if test (count a b c) != 0
    echo "env.fish replaced the user's count function"
    exit 1
end

echo "env.fish loads and works with shadowed built-ins"
