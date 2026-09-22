#!/usr/bin/env python3
"""Run jevr over a BEIR query set and record the top-10 ranking per query.

Responsibility: the retrieval half of the benchmark — one `jevr <query>
<corpus_dir> --threshold 0 --top-k 10 --json` subprocess per test query
(those present in the qrels), mapping returned file paths back to BEIR doc
ids by filename stem. Scoring lives in score.py.

Exit-code contract honored from jevr: 0 = matches, 1 = no matches (recorded
as an empty ranking), 2 = error (aborts the run).

Usage: python3 run.py <queries.jsonl> <qrels.tsv> <corpus_dir> <out.jsonl>
"""

import json
import subprocess
import sys
from pathlib import Path


def qrels_query_ids(qrels_tsv: Path) -> set:
    """Query ids that have relevance judgments (skips the header row)."""
    with qrels_tsv.open(encoding='utf-8') as fh:
        next(fh)
        return {line.split('\t')[0] for line in fh if line.strip()}


def rank(query: str, corpus_dir: str) -> list:
    """Top-10 doc ids for one query, in jevr's final order."""
    proc = subprocess.run(
        ['jevr', query, corpus_dir, '--threshold', '0', '--top-k', '10', '--json'],
        capture_output=True,
        text=True,
        check=False,  # exit codes 1 (no matches) and 2 (error) handled below
    )
    if proc.returncode == 1:
        return []
    if proc.returncode != 0:
        sys.exit(f'jevr failed (exit {proc.returncode}): {proc.stderr.strip()}')
    return [Path(r['file']).stem for r in json.loads(proc.stdout)]


def main() -> None:
    if len(sys.argv) != 5:
        sys.exit(
            f'usage: {sys.argv[0]} <queries.jsonl> <qrels.tsv> <corpus_dir> <out.jsonl>'
        )
    queries_path, qrels_path, corpus_dir, out_path = sys.argv[1:5]

    judged = qrels_query_ids(Path(qrels_path))
    with Path(queries_path).open(encoding='utf-8') as fh:
        queries = [json.loads(line) for line in fh if line.strip()]
    queries = [q for q in queries if q['_id'] in judged]

    with Path(out_path).open('w', encoding='utf-8') as out:
        for i, q in enumerate(queries, 1):
            ranking = rank(q['text'], corpus_dir)
            out.write(json.dumps({'query_id': q['_id'], 'ranking': ranking}) + '\n')
            if i % 25 == 0 or i == len(queries):
                print(f'{i}/{len(queries)} queries done')


if __name__ == '__main__':
    main()
