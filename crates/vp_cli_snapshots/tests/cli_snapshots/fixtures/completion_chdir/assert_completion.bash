set -eu
vp_bin=$(command -v vp)
VP_COMPLETE=bash "$vp_bin" > completion.bash
source completion.bash
COMP_WORDBREAKS=' :='
COMP_TYPE=9
vp() {
    local IFS=" "
    [[ $_CLAP_COMPLETE_INDEX == "$expected_index" && "$*" == "$expected_args" ]] || {
        printf 'unexpected completion argv: index=%s %s\n' "$_CLAP_COMPLETE_INDEX" "$*" >&2
        return 1
    }
    printf 'build:prod'
}
for invocation in vp vpr /path/vpr; do
    for style in separate attached equals; do
        case $style in
            separate) COMP_WORDS=("$invocation" -C 'apps\ web'); expected_index=4 ;;
            attached) COMP_WORDS=("$invocation" '-Capps\ web'); expected_index=3 ;;
            equals) COMP_WORDS=("$invocation" -C = 'apps\ web'); expected_index=3 ;;
        esac
        expected_args="-- $invocation"
        case $style in
            separate) expected_args+=' -C apps\ web' ;;
            attached) expected_args+=' -Capps\ web' ;;
            equals) expected_args+=' -C=apps\ web' ;;
        esac
        [[ $invocation != vp ]] || COMP_WORDS+=(run)
        expected_args+=' run build:pr'
        COMP_WORDS+=(build : pr)
        COMP_CWORD=$((${#COMP_WORDS[@]} - 1))
        COMP_LINE="${COMP_WORDS[*]}"
        COMP_LINE=${COMP_LINE//build : pr/build:pr}
        [[ $style != equals ]] || COMP_LINE=${COMP_LINE//-C = /-C=}
        _clap_complete_vp || exit 1
        [[ ${COMPREPLY[*]} == prod ]]
    done
done
mkdir -p 'apps web/unique-child'
for dir in 'apps\ web' "'apps web'" '"apps web"'; do
    result=$(VP_COMPLETE=bash _CLAP_COMPLETE_INDEX=4 "$vp_bin" -- vp -C "$dir" -C unique)
    [[ $result == unique-child* ]]
done
printf 'completion argv, colon trimming, and escaped cwd: ok\n'
