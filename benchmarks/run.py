#!/usr/bin/env python3
"""Run jevr over a BEIR query set and record the top-10 ranking per query.

Responsibility: the retrieval half of the benchmark — one `jevr <query>
<corpus_dir> --threshold 0 --top-k 10 --json` subprocess per test query
(those present in the qrels), mapping returned file paths back to BEIR doc
ids by filename stem. Scoring lives in score.py.

Exit-code contract honored from jevr: 0 = matches, 1 = no matches (recorded
as an empty ranking), 2 = error (aborts the run).

Runs against the BEIR response cache, cache/beir/; harness.py describes the
--offline replay and the --spotcheck mode.

Usage: python3 run.py [--offline] <queries.jsonl> <qrels.tsv> <corpus_dir> <out.jsonl>
       python3 run.py --spotcheck <seed> <n> <queries.jsonl> <qrels.tsv> <corpus_dir>
"""

import json
import sys
from functools import partial
from pathlib import Path

import harness


def qrels_query_ids(qrels_tsv: Path) -> set:
    """Query ids that have relevance judgments (skips the header row)."""
    with qrels_tsv.open(encoding='utf-8') as fh:
        next(fh)
        return {line.split('\t')[0] for line in fh if line.strip()}


def main() -> None:
    offline, spot, args = harness.mode(sys.argv[1:])
    if len(args) != (3 if spot else 4):
        sys.exit(__doc__.split('Usage: ')[1])
    queries_path, qrels_path, corpus_dir = args[:3]

    judged = qrels_query_ids(Path(qrels_path))
    with Path(queries_path).open(encoding='utf-8') as fh:
        queries = [json.loads(line) for line in fh if line.strip()]
    queries = [q for q in queries if q['_id'] in judged]
    jobs = [
        partial(
            harness.jevr,
            [q['text'], corpus_dir, '--threshold', '0', '--top-k', '10', '--json'],
        )
        for q in queries
    ]
    if spot:
        harness.spotcheck('beir', jobs, *spot)
        return

    rankings = harness.run('beir', jobs, offline, workers=4)
    with Path(args[3]).open('w', encoding='utf-8') as out:
        for q, results in zip(queries, rankings):
            ranking = [Path(r['file']).stem for r in results]
            out.write(json.dumps({'query_id': q['_id'], 'ranking': ranking}) + '\n')


if __name__ == '__main__':
    main()
