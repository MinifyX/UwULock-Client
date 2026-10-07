#!/usr/bin/env bash
# Waits until a job of this same workflow run has uploaded the artifact <name>, so a long job can
# start right away and pick up what a short one (the extension, extension.yml) builds beside it,
# instead of waiting for it before it begins. Fails when the artifact doesn't come, or the job
# that makes it failed. Needs GH_TOKEN with `actions: read`.
#
# Usage: scripts/wait-artifact.sh <name> <job> [minutes, default 30]
#   <job>: a part of the name of the job that uploads it, to stop waiting when that one failed.
set -euo pipefail

name="$1"
job="$2"
minutes="${3:-30}"
: "${GITHUB_REPOSITORY:?}" "${GITHUB_RUN_ID:?}" "${GH_TOKEN:?}"

deadline=$(($(date +%s) + minutes * 60))
while [ "$(date +%s)" -lt "$deadline" ]; do
  found=$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID/artifacts?name=$name" \
    --jq '.total_count' 2>/dev/null || echo 0)
  if [ "$found" -gt 0 ]; then
    echo "Artifact $name is there."
    exit 0
  fi
  # The job that makes it failed or was cancelled: it won't come any more.
  failed=$(gh api "repos/$GITHUB_REPOSITORY/actions/runs/$GITHUB_RUN_ID/jobs?per_page=100" \
    --jq "[.jobs[] | select((.name | contains(\"$job\")) and (.conclusion == \"failure\" or .conclusion == \"cancelled\")) | .name] | join(\", \")" \
    2>/dev/null || true)
  if [ -n "$failed" ]; then
    echo "::error::Waiting for $name, but $failed failed"
    exit 1
  fi
  sleep 15
done
echo "::error::Artifact $name didn't arrive within $minutes minutes"
exit 1
