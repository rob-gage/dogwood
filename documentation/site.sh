#!/usr/bin/env bash
set -euo pipefail

site_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
workspace_dir=$(CDPATH= cd -- "$site_dir/.." && pwd)
command=${1:-build}
if [[ $# -gt 1 || ! "$command" =~ ^(build|serve|check|publish)$ ]]; then
    echo "Usage: $0 [build|serve|check|publish]" >&2
    exit 2
fi
if ! command -v zola >/dev/null 2>&1; then
    echo 'Zola is required: https://www.getzola.org/documentation/getting-started/installation/' >&2
    exit 1
fi

# Keep the assembled site outside the checkout, including preview output. (Cargo's rustdoc
# build cache stays in the ignored target directory; see rustdoc() below.)
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

# Generates the public (game-facing library crates) and private (every crate, private items)
# Rust API references into the built site under rustdoc/. RUSTDOC=0 skips both.
rustdoc() {
    if [[ "${RUSTDOC:-1}" == 0 ]]; then
        echo 'Skipping Rust API references (RUSTDOC=0).' >&2
        return
    fi
    if ! command -v cargo >/dev/null 2>&1; then
        echo 'Cargo is required for the Rust API references; set RUSTDOC=0 to skip them.' >&2
        exit 1
    fi
    # Cargo's build cache stays in the ignored target directory (or RUSTDOC_TARGET_DIR) so that
    # repeated builds are incremental and large intermediates stay out of the temporary directory.
    local target_root=${RUSTDOC_TARGET_DIR:-${CARGO_TARGET_DIR:-$workspace_dir/target}/documentation}
    cargo doc --manifest-path "$workspace_dir/Cargo.toml" --workspace --exclude cli --no-deps \
        --target-dir "$target_root/public"
    cargo doc --manifest-path "$workspace_dir/Cargo.toml" --workspace --no-deps \
        --document-private-items --target-dir "$target_root/private"
    mkdir -p "$work_dir/site/rustdoc"
    cp -R "$target_root/public/doc" "$work_dir/site/rustdoc/public"
    cp -R "$target_root/private/doc" "$work_dir/site/rustdoc/private"
}

build() {
    if [[ -n "${BASE_URL:-}" ]]; then
        zola --root "$site_dir" --config config.toml build --output-dir "$work_dir/site" --base-url "$BASE_URL"
    else
        zola --root "$site_dir" --config config.toml build --output-dir "$work_dir/site"
    fi
    rustdoc
}

# Prints every `extra.sources` entry of every content file as "<content file> <source path>".
page_sources() {
    local file list entry
    while IFS= read -r -d '' file; do
        # the array may span lines; it ends at the first line containing `]`
        list=$(awk '/^\+\+\+/ { fences++; next } fences == 1 && /^sources *= *\[/ { open = 1 }
            open { print } open && /\]/ { exit }' "$file")
        [[ -z "$list" ]] && continue
        for entry in $(grep -oE '"[^"]+"' <<< "$list" | tr -d '"'); do
            printf '%s %s\n' "${file#"$site_dir/"}" "$entry"
        done
    done < <(find "$site_dir/content" -name '*.md' -print0)
}

# Verifies source references, source coverage, and Rust API reference links of a build.
check() {
    local problems=0 sources file entry path covered href target fragment
    sources=$(page_sources)
    # every documented source path must exist
    while read -r file entry; do
        if [[ ! -e "$workspace_dir/$entry" ]]; then
            echo "Missing source: $file lists $entry" >&2
            problems=$((problems + 1))
        fi
    done <<< "$sources"
    # every Rust, WGSL, and manifest file must be documented by some page or directory entry
    while IFS= read -r path; do
        covered=0
        while read -r file entry; do
            entry=${entry%/}
            if [[ "$path" == "$entry" || "$path" == "$entry"/* ]]; then covered=1; break; fi
        done <<< "$sources"
        if [[ $covered == 0 ]]; then
            echo "Undocumented source: $path" >&2
            problems=$((problems + 1))
        fi
    done < <(cd "$workspace_dir" && find . \( -path ./target -o -path ./documentation -o -path ./.git \) -prune \
        -o -type f \( -name '*.rs' -o -name '*.wgsl' -o -name Cargo.toml \) -print | sed 's|^\./||' | sort)
    # every Rust API reference link must resolve to a generated page and anchor
    if [[ -d "$work_dir/site/rustdoc" ]]; then
        while IFS= read -r href; do
            target=${href#*rustdoc/}
            fragment=''
            if [[ "$target" == *'#'* ]]; then fragment=${target#*#}; target=${target%%#*}; fi
            if [[ ! -f "$work_dir/site/rustdoc/$target" ]]; then
                echo "Broken Rust API link: $href" >&2
                problems=$((problems + 1))
            elif [[ -n "$fragment" ]] && ! grep -q "id=\"$fragment\"" "$work_dir/site/rustdoc/$target"; then
                echo "Broken Rust API anchor: $href" >&2
                problems=$((problems + 1))
            fi
        done < <(find "$work_dir/site" -path "$work_dir/site/rustdoc" -prune -o -name '*.html' -print0 \
            | xargs -0 grep -ohE 'href="[^"]*/rustdoc/(public|private)/[^"]*"' | sed -E 's/^href="|"$//g' | sort -u)
    else
        echo 'Rust API links were not checked (RUSTDOC=0).' >&2
    fi
    if [[ $problems -gt 0 ]]; then
        echo "Documentation check found $problems problem(s)." >&2
        exit 1
    fi
    echo 'Documentation check passed.'
}

case "$command" in
    build)
        build
        # Retain successful local builds for inspection; the printed directory is disposable.
        trap - EXIT
        printf 'Built site: %s\n' "$work_dir/site"
        ;;
    serve)
        # Live preview of the Markdown site; the Rust API references require `build`.
        zola --root "$site_dir" --config config.toml serve --output-dir "$work_dir/site"
        ;;
    check)
        build
        check
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
