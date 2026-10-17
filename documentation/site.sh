#!/usr/bin/env bash
set -euo pipefail

site_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
command=${1:-build}
if [[ $# -gt 1 || ! "$command" =~ ^(build|serve|publish)$ ]]; then
    echo "Usage: $0 [build|serve|publish]" >&2
    exit 2
fi
if ! command -v zola >/dev/null 2>&1; then
    echo 'Zola is required: https://www.getzola.org/documentation/getting-started/installation/' >&2
    exit 1
fi

# Keep all generated output outside the checkout, including preview output.
work_dir=$(mktemp -d /tmp/dogwood-docs.XXXXXXXX)
worktree=''
repo=''
cleanup() {
    if [[ -n "$worktree" ]]; then
        git -C "$repo" worktree remove --force "$worktree" || return
    fi
    rm -rf -- "$work_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

build() {
    if [[ -n "${BASE_URL:-}" ]]; then
        zola --root "$site_dir" --config config.toml build --output-dir "$work_dir/site" --base-url "$BASE_URL"
    else
        zola --root "$site_dir" --config config.toml build --output-dir "$work_dir/site"
    fi
}

case "$command" in
    build)
        build
        # Retain successful local builds for inspection; the printed directory is disposable.
        trap - EXIT
        printf 'Built site: %s\n' "$work_dir/site"
        ;;
    serve)
        zola --root "$site_dir" --config config.toml serve --output-dir "$work_dir/site"
        ;;
    publish)
        build
        repo=$(git -C "$site_dir" rev-parse --show-toplevel)
        remote=${PAGES_REMOTE:-origin}
        # An explicit remote ref distinguishes an absent branch from connection failures.
        if git -C "$repo" ls-remote --exit-code --heads "$remote" refs/heads/gh-pages > "$work_dir/remote-ref"; then
            git -C "$repo" fetch --no-tags "$remote" refs/heads/gh-pages
            base=FETCH_HEAD
        else
            status=$?
            if [[ $status -ne 2 ]]; then
                echo 'Could not inspect the publishing remote.' >&2
                exit "$status"
            fi
            if git -C "$repo" show-ref --verify --quiet refs/heads/gh-pages; then
                base=refs/heads/gh-pages
            else
                base=HEAD
            fi
        fi
        tree_path="$work_dir/worktree"
        git -C "$repo" worktree add --detach "$tree_path" "$base"
        worktree=$tree_path
        if [[ ! -s "$work_dir/remote-ref" && "$base" == HEAD ]]; then
            # Start independent history; the source branch and its files remain untouched.
            git -C "$worktree" switch --orphan gh-pages
        fi
        git -C "$worktree" rm -r --ignore-unmatch --quiet .
        cp -R "$work_dir/site/." "$worktree/"
        touch "$worktree/.nojekyll"
        git -C "$worktree" add --all
        if ! git -C "$worktree" diff --cached --quiet; then
            git -C "$worktree" -c core.hooksPath=/dev/null commit -m 'Publish documentation'
        fi
        # A concurrent remote update is rejected by this ordinary, non-forced push.
        git -C "$worktree" push "$remote" HEAD:refs/heads/gh-pages
        ;;
esac
