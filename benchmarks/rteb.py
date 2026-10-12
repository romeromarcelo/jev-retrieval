#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["pyarrow"]
# ///
"""HAKARI-Bench NanoRTEB: nDCG@10 on 14 specialized retrieval tasks.

Responsibility: the three runs recorded in RTEB.md, one jevr per judged
query (2,390 over 14 tasks), in one of three modes:

- retrieval: `jevr <query> <corpus> --threshold 0 --top-k 10 --json`, the
  leaderboard's full-corpus mode;
- cap100: the same with --config holding `candidate_cap: 100`;
- reranking: the same with --candidates listing the query's
  reranking_hybrid candidates, the leaderboard's reranking mode.

The dataset is read from its Parquet files at a pinned revision. Each task's
corpus is written as corpora/rteb/<task>/d00000.txt, ... in row order, each
file holding exactly the document's text and a trailing newline: request
bodies embed these bytes, so they key the response cache. Scored with
score.py's nDCG@10 (graded qrels gains), per task and as the 14-task mean.

Runs against cache/rteb/; harness.py describes the --offline replay and the
--spotcheck mode. Needs pyarrow, which `uv run rteb.py` installs.

Usage: python3 rteb.py [--offline] <retrieval|cap100|reranking> <out.jsonl>
       python3 rteb.py --spotcheck <seed> <n> <retrieval|cap100|reranking>
"""

import json
import statistics
import sys
from functools import partial
from pathlib import Path

import pyarrow.parquet

import harness
from score import ndcg_at_10

HERE = Path(__file__).resolve().parent
CORPORA = HERE / 'corpora' / 'rteb'
CAP100 = CORPORA / 'cap100.yaml'
PARQUET = (
    'https://huggingface.co/datasets/hakari-bench/NanoRTEB/resolve/'
    '3a002b35576f30f114f94bb417026139e70fb9db/{}/{}-00000-of-00001.parquet'
)
TASKS = (
    'NanoAILACasedocs',
    'NanoAILAStatutes',
    'NanoApps',
    'NanoCUREv1',
    'NanoChatDoctor',
    'NanoDS1000',
    'NanoFinQA',
    'NanoFinanceBench',
    'NanoFreshStack',
    'NanoHC3Finance',
    'NanoHumanEval',
    'NanoLegalSummarization',
    'NanoMBPP',
    'NanoWikiSQL',
)
MODES = ('retrieval', 'cap100', 'reranking')
FLAGS = ['--threshold', '0', '--top-k', '10', '--json']


def rows(config: str, task: str) -> list:
    """A task's rows in one dataset config, downloaded once into data/rteb/."""
    path = harness.download(
        PARQUET.format(config, task),
        HERE / 'data' / 'rteb' / config / f'{task}.parquet',
    )
    return pyarrow.parquet.read_table(path).to_pylist()


def write(path: Path, data: bytes) -> None:
    """Write data unless path already holds it, so a run never rewrites a file another run is reading."""
    if not path.exists() or path.read_bytes() != data:
        path.write_bytes(data)


def prepare(task: str) -> list:
    """The task's judged queries, after writing its corpus and candidate files.

    Each query carries its qrels gains, its candidates file and the task's
    stem -> corpus-id map, which turns jevr's result paths back into ids.
    """
    corpus, candidates = CORPORA / task, CORPORA / f'{task}.candidates'
    corpus.mkdir(parents=True, exist_ok=True)
    candidates.mkdir(exist_ok=True)
    stems = {}
    for i, doc in enumerate(rows('corpus', task)):
        stems[doc['_id']] = f'd{i:05d}'
        write(corpus / f'd{i:05d}.txt', (doc['text'] + '\n').encode('utf-8'))
    ids = {stem: corpus_id for corpus_id, stem in stems.items()}
    gains = {}
    for r in rows('qrels', task):
        gains.setdefault(r['query-id'], {})[r['corpus-id']] = int(r['score'])
    hybrid = {r['query-id']: r['corpus-ids'] for r in rows('reranking_hybrid', task)}

    queries = []
    for j, q in enumerate(q for q in rows('queries', task) if q['_id'] in gains):
        path = candidates / f'q{j:04d}.txt'
        write(path, ''.join(f'{stems[c]}.txt\n' for c in hybrid[q['_id']]).encode())
        queries.append(
            {
                'task': task,
                'id': q['_id'],
                'text': q['text'],
                'gains': gains[q['_id']],
                'candidates': path,
                'ids': ids,
            }
        )
    return queries


def mode_flags(mode: str, query: dict) -> list:
    """What a mode adds to FLAGS."""
    if mode == 'cap100':
        return ['--config', str(CAP100)]
    if mode == 'reranking':
        return ['--candidates', str(query['candidates'])]
    return []


def main() -> None:
    offline, spot, args = harness.mode(sys.argv[1:])
    if len(args) != (1 if spot else 2) or args[0] not in MODES:
        sys.exit(__doc__.split('Usage: ')[1])
    queries = [q for task in TASKS for q in prepare(task)]
    write(CAP100, b'candidate_cap: 100\n')
    jobs = [
        partial(
            harness.jevr,
            [q['text'], str(CORPORA / q['task']), *FLAGS, *mode_flags(args[0], q)],
        )
        for q in queries
    ]
    if spot:
        harness.spotcheck('rteb', jobs, *spot)
        return

    rankings = [
        [q['ids'][Path(r['file']).stem] for r in results]
        for q, results in zip(queries, harness.run('rteb', jobs, offline, workers=4))
    ]
    with Path(args[1]).open('w', encoding='utf-8') as out:
        out.writelines(
            json.dumps({'task': q['task'], 'query_id': q['id'], 'ranking': ranking})
            + '\n'
            for q, ranking in zip(queries, rankings)
        )
    by_task = {}
    for q, ranking in zip(queries, rankings):
        by_task.setdefault(q['task'], []).append(ndcg_at_10(ranking, q['gains']))
    means = {task: statistics.fmean(scores) for task, scores in by_task.items()}
    for task, mean in means.items():
        print(f'{task:24} {100 * mean:5.1f}')
    print(
        f'{"Mean":24} {100 * statistics.fmean(means.values()):5.1f}  (nDCG@10 x100, {len(queries)} queries)'
    )


if __name__ == '__main__':
    main()
