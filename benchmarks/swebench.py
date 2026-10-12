#!/usr/bin/env python3
# /// script
# requires-python = ">=3.9"
# dependencies = ["pyarrow"]
# ///
"""SWE-bench Lite file localization: does jevr rank the file the fix edits?

Responsibility: the code-retrieval gate of docs/BENCHMARKS.md §2. The sample
is every 6th of the 300 test instances sorted by id (50 instances, 11
repos), read from the dataset's Parquet file at a pinned revision. Each repo
is checked out at the instance's base_commit under repos/, the query is the
raw problem statement, and jevr runs at its defaults:
`jevr <problem statement> <checkout> --json`. Gold is the file the
instance's patch edits (every Lite patch edits exactly one existing file;
test edits live in test_patch). Scored over every result jevr prints,
fallback guesses included: Recall@1, any-gold@10, all-gold@10 and MRR
(1 / rank of the first gold file, 0 if absent).

Runs against cache/swebench/; harness.py describes the --offline replay and
the --spotcheck mode. Needs pyarrow, which `uv run swebench.py` installs.

Usage: python3 swebench.py [--offline] <out.jsonl>
       python3 swebench.py --spotcheck <seed> <n>
"""

import json
import re
import subprocess
import sys
from functools import partial
from pathlib import Path

import pyarrow.parquet

import harness

HERE = Path(__file__).resolve().parent
REPOS = HERE / 'repos'
PARQUET = (
    'https://huggingface.co/datasets/princeton-nlp/SWE-bench_Lite/resolve/'
    '6ec7bb89b9342f664a54a6e0a6ea6501d3437cc2/data/test-00000-of-00001.parquet'
)


def sample() -> list:
    """The 50 sampled instances: every 6th of the 300 test instances by id."""
    path = harness.download(PARQUET, HERE / 'data' / 'swebench' / 'test.parquet')
    rows = pyarrow.parquet.read_table(path).to_pylist()
    return sorted(rows, key=lambda r: r['instance_id'])[::6]


def checkout(repo: str, commit: str) -> Path:
    """The repo's clone under repos/, at commit, with no other file in the tree."""
    path = REPOS / repo.replace('/', '__')
    if not path.exists():
        git(
            'clone',
            '-q',
            '--filter=blob:none',
            '--no-checkout',
            f'https://github.com/{repo}.git',
            str(path),
        )
    git('-C', str(path), 'checkout', '-q', '-f', commit)
    git('-C', str(path), 'clean', '-q', '-fdx')
    return path


def git(*args: str) -> None:
    subprocess.run(['git', *args], check=True)


def localize(instance: dict, env: dict) -> tuple:
    """jevr's results and stderr for one instance, run on its base_commit checkout."""
    path = checkout(instance['repo'], instance['base_commit'])
    return harness.jevr([instance['problem_statement'], str(path), '--json'], env)


def gold(patch: str) -> set:
    """The files a patch edits."""
    return set(re.findall(r'^diff --git a/(\S+) b/', patch, re.MULTILINE))


def score(golds: list, rankings: list) -> str:
    """Recall@1, any-gold@10, all-gold@10 and MRR, as one line."""
    n = len(golds)
    recall1 = sum(bool(g & set(r[:1])) for g, r in zip(golds, rankings)) / n
    any10 = sum(bool(g & set(r[:10])) for g, r in zip(golds, rankings)) / n
    all10 = sum(g <= set(r[:10]) for g, r in zip(golds, rankings)) / n
    mrr = (
        sum(
            next((1 / i for i, f in enumerate(r, 1) if f in g), 0.0)
            for g, r in zip(golds, rankings)
        )
        / n
    )
    return f'Recall@1 {recall1:.3f}, any-gold@10 {any10:.3f}, all-gold@10 {all10:.3f}, MRR {mrr:.3f} over {n} instances'


def main() -> None:
    offline, spot, args = harness.mode(sys.argv[1:])
    if len(args) != (0 if spot else 1):
        sys.exit(__doc__.split('Usage: ')[1])
    instances = sample()
    jobs = [partial(localize, instance) for instance in instances]
    if spot:
        harness.spotcheck('swebench', jobs, *spot)
        return

    rankings = [
        [
            str(Path(r['file']).relative_to(REPOS / i['repo'].replace('/', '__')))
            for r in results
        ]
        for i, results in zip(instances, harness.run('swebench', jobs, offline))
    ]
    with Path(args[0]).open('w', encoding='utf-8') as out:
        out.writelines(
            json.dumps({'instance_id': instance['instance_id'], 'ranking': ranking})
            + '\n'
            for instance, ranking in zip(instances, rankings)
        )
    print(score([gold(i['patch']) for i in instances], rankings))


if __name__ == '__main__':
    main()
