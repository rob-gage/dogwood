#!/bin/sh
set -eu

command -v mdbook >/dev/null || { echo 'mdbook is required: https://rust-lang.github.io/mdBook/'; exit 1; }
mdbook build documentation
mdbook build documentation/internals

python3 - <<'PY'
from pathlib import Path
import re
for root in (Path('documentation/src'), Path('documentation/internals/src')):
    for page in root.rglob('*.md'):
        for link in re.findall(r'\]\(([^)#]+\.md)\)', page.read_text()):
            if not (page.parent / link).exists():
                raise SystemExit(f'{page}: missing link target {link}')
PY

for name in Game GameApplication Scene ActorRegistry MaterialRegistryBuilder UserInterfaceContext; do
    rg -q "\b$name\b" engine/src engine_physics/src engine_user_interface/src documentation/src || {
        echo "documented identifier missing: $name" >&2; exit 1;
    }
done
echo 'documentation checks passed'
