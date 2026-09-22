#!/usr/bin/env bash
# Install the jevr Claude Code plugin (skill: using-jevr).
#
# Usage:
#   ./install.sh            # register this clone as a plugin marketplace
#   ./install.sh --remote   # register the GitHub repo instead of the clone
#
# Falls back to copying the skill into ~/.claude/skills/ when the
# `claude` CLI is not available.
set -euo pipefail

repo_dir="$(cd "$(dirname "$0")" && pwd)"
remote_repo="romeromarcelo/jev-retrieval"

if command -v claude >/dev/null 2>&1; then
  if [ "${1:-}" = "--remote" ]; then
    claude plugin marketplace add "$remote_repo"
  else
    claude plugin marketplace add "$repo_dir"
  fi
  claude plugin install jevr@jevr
  echo "Plugin installed: skill 'using-jevr' is available in Claude Code."
else
  skill_dir="$HOME/.claude/skills/using-jevr"
  mkdir -p "$skill_dir"
  cp -R "$repo_dir/skills/using-jevr/." "$skill_dir/"
  echo "claude CLI not found - copied the skill to $skill_dir instead."
fi

cat <<'EOF'

Next steps:
  1. Build and install the jevr binary:  cargo install --path . --locked
  2. Export your API key:                export TYPESAFE_API_KEY=...
  3. Try it:                             jevr "where is the config loaded?" src/
EOF
