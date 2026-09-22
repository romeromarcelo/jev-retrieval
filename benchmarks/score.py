#!/usr/bin/env python3
"""Score a benchmark run: macro-averaged nDCG@10 against BEIR qrels.

Standard formulation — DCG@10 = sum(gain_i / log2(i + 1)) over ranks
i = 1..10 with graded gains from the qrels; the ideal DCG uses the query's
qrels gains sorted descending. Queries with judgments but an empty ranking
score 0, so a no-match query honestly drags the average down.

Usage: python3 score.py <results.jsonl> <qrels.tsv>
"""

import json
import math
import sys
from pathlib import Path


def load_qrels(qrels_tsv: Path) -> dict:
    """{query_id: {doc_id: gain}} from a BEIR qrels TSV (header skipped)."""
    qrels: dict = {}
    with qrels_tsv.open(encoding='utf-8') as fh:
        next(fh)
        for line in fh:
            if not line.strip():
                continue
            qid, did, score = line.rstrip('\n').split('\t')
            qrels.setdefault(qid, {})[did] = int(score)
    return qrels


def ndcg_at_10(ranking: list, gains: dict) -> float:
    """nDCG@10 for one query; 0.0 when the query has no positive gains."""
    dcg = sum(
        gains.get(doc, 0) / math.log2(i + 1)
        for i, doc in enumerate(ranking[:10], start=1)
    )
    ideal = sum(
        g / math.log2(i + 1)
        for i, g in enumerate(sorted(gains.values(), reverse=True)[:10], start=1)
    )
    return dcg / ideal if ideal > 0 else 0.0


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(f'usage: {sys.argv[0]} <results.jsonl> <qrels.tsv>')
    qrels = load_qrels(Path(sys.argv[2]))

    scores = []
    with Path(sys.argv[1]).open(encoding='utf-8') as fh:
        for line in fh:
            if not line.strip():
                continue
            r = json.loads(line)
            if r['query_id'] in qrels:
                scores.append(ndcg_at_10(r['ranking'], qrels[r['query_id']]))

    if not scores:
        sys.exit('no scored queries — do results and qrels use the same query ids?')
    print(f'nDCG@10 = {sum(scores) / len(scores):.3f} over {len(scores)} queries')


if __name__ == '__main__':
    main()
