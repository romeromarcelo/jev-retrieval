#!/usr/bin/env python3
"""Convert a BEIR corpus.jsonl into one plain-text file per document.

Each document is written to <out_dir>/<doc_id>.txt as exactly:

    title, a blank line, text, and a trailing newline

This byte format is a contract, not a convenience: jevr's response cache is
keyed on the full request body, which embeds file content verbatim. Emitting
the same bytes as the reference runs makes cached responses replayable and
results comparable; any deviation silently produces a different (cold,
billed) run.

Usage: python3 convert.py data/scifact/corpus.jsonl corpora/scifact
"""

import json
import sys
from pathlib import Path


def convert(corpus_jsonl: Path, out_dir: Path) -> int:
    """Write every corpus document as <doc_id>.txt; return the count."""
    out_dir.mkdir(parents=True, exist_ok=True)
    count = 0
    with corpus_jsonl.open(encoding='utf-8') as fh:
        for line in fh:
            if not line.strip():
                continue
            doc = json.loads(line)
            body = f'{doc.get("title", "")}\n\n{doc.get("text", "")}\n'
            (out_dir / f'{doc["_id"]}.txt').write_text(body, encoding='utf-8')
            count += 1
    return count


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(f'usage: {sys.argv[0]} <corpus.jsonl> <out_dir>')
    n = convert(Path(sys.argv[1]), Path(sys.argv[2]))
    print(f'wrote {n} documents to {sys.argv[2]}')


if __name__ == '__main__':
    main()
