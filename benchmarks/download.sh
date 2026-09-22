#!/usr/bin/env bash
# Download and verify the BEIR SciFact and NFCorpus datasets into data/.
# Idempotent: a zip that already exists and passes its md5 check is not
# re-downloaded; extraction is skipped when the dataset directory exists.
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p data

md5_of() {
  if command -v md5 >/dev/null 2>&1; then
    md5 -q "$1"
  else
    md5sum "$1" | cut -d' ' -f1
  fi
}

fetch() {
  local name="$1" sum="$2"
  local zip="data/${name}.zip"
  local url="https://public.ukp.informatik.tu-darmstadt.de/thakur/BEIR/datasets/${name}.zip"

  if [[ -f "$zip" ]] && [[ "$(md5_of "$zip")" == "$sum" ]]; then
    echo "$zip already downloaded and verified"
  else
    echo "downloading $url"
    curl -fL --retry 3 -o "$zip" "$url"
    local got
    got="$(md5_of "$zip")"
    if [[ "$got" != "$sum" ]]; then
      echo "md5 mismatch for $zip: expected $sum, got $got" >&2
      exit 1
    fi
  fi

  if [[ -d "data/${name}" ]]; then
    echo "data/${name} already extracted"
  else
    unzip -q "$zip" -d data
  fi
}

fetch scifact 5f7d1de60b170fc8027bb7898e2efca1
fetch nfcorpus a89dba18a62ef92f7d323ec890a0d38d
echo "done"
