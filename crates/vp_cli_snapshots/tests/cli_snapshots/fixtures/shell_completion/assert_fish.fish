source home/env.fish

# 1. vp un
set -l res_un (complete -C "vp un" | string split -f1 \t)
echo "vp un -> $res_un"
builtin contains -- uninstall $res_un; or exit 1
builtin contains -- unlink $res_un; or exit 1
builtin contains -- un $res_un; or exit 1

# 2. vp -C packages/subpkg run
set -l res_sub (complete -C "vp -C packages/subpkg run " | string split -f1 \t)
echo "vp -C packages/subpkg run  -> $res_sub"
builtin contains -- sub:task $res_sub; or exit 1

# 3. vpr build:
set -l res_build (complete -C "vpr build:" | string split -f1 \t)
echo "vpr build: -> $res_build"
builtin contains -- build:watch $res_build; or exit 1

# 4. vp remove
set -l res_remove (complete -C "vp remove " | string split -f1 \t)
echo "vp remove  -> $res_remove"
builtin contains -- react $res_remove; or exit 1

# 5. vp run --filter
set -l res_filter (complete -C "vp run --filter " | string split -f1 \t)
echo "vp run --filter  -> $res_filter"
builtin contains -- @demo/subpkg $res_filter; or exit 1

# 6. vp check --
set -l res_check (complete -C "vp check --" | string split -f1 \t)
echo "vp check -- -> $res_check"
builtin contains -- --fix-dangerously $res_check; or exit 1

# 7. vpx -C custom ./scripts/
set -l res_vpx (complete -C "vpx -C custom ./scripts/" | string split -f1 \t)
echo "vpx -C custom ./scripts/ -> $res_vpx"
builtin contains -- ./scripts/seed.ts $res_vpx; or exit 1

echo "Fish completion assertions passed"
