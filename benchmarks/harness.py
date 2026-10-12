"""Shared jevr runner for the benchmarks in this directory.

Responsibility: run a benchmark's jobs — each one jevr invocation — against
that benchmark's own response cache, cache/<name>/ (git-ignored), and count
what the run bills. Every benchmark script takes the same modes:

- live (default): a request missing from the cache is billed and cached, so
  each response is paid for once and reused by every later run;
- --offline: a missing request fails without reaching the network, so the
  replay is free and its count of missing requests is what a live run would
  send. jevr handles a miss as it handles any failed request, which is how
  the live run handled a request the API rejected (never cached), so a
  replay of a complete cache reproduces that run's results byte for byte;
- --spotcheck <seed> <n>: re-sends a random sample of n jobs, drawn from a
  seed the verifier picks after publication, to the live API with no cache
  (billed to the caller's TYPESAFE_API_KEY), and compares every answer with
  the cached answer to the byte-identical request. Two passes, so both stages
  are compared on identical requests: the first re-sends every verification
  request; the second serves the cached verification answers, so each rerank
  request carries the cached kept set and order and is re-sent as cached.

A job is a callable taking jevr's environment and returning jevr's JSON
results and stderr.
"""

import json
import os
import random
import shutil
import statistics
import subprocess
import sys
import tempfile
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

CACHE = Path(__file__).resolve().parent / 'cache'
REQUEST = 'jevr: debug: jev request'  # JEVR_DEBUG: one line per request not served from the cache


def download(url: str, path: Path) -> Path:
    """path, downloaded from url unless already there (an interrupted download leaves only a .part file)."""
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(url, path.with_suffix('.part'))
        path.with_suffix('.part').rename(path)
    return path


def mode(argv: list) -> tuple:
    """(offline, spotcheck (seed, n) or None, remaining args) from a script's argv."""
    if argv[:1] == ['--offline']:
        return True, None, argv[1:]
    if argv[:1] == ['--spotcheck'] and len(argv) >= 3:
        return False, (argv[1], int(argv[2])), argv[3:]
    return False, None, argv


def environment(cache_home: Path, offline: bool) -> dict:
    """jevr's environment: the given cache, request accounting, optional offline replay."""
    env = {**os.environ, 'XDG_CACHE_HOME': str(cache_home), 'JEVR_DEBUG': '1'}
    if offline:
        env['JEVR_OFFLINE'] = '1'
        # jevr requires a key; with a placeholder, even a build that predates
        # JEVR_OFFLINE cannot bill a miss.
        env['TYPESAFE_API_KEY'] = 'offline'
    return env


def jevr(args: list, env: dict) -> tuple:
    """jevr's JSON results and stderr; exit 1 (nothing kept) still prints fallback guesses."""
    proc = subprocess.run(
        ['jevr', *args], capture_output=True, text=True, env=env, check=False
    )
    if proc.returncode not in (0, 1):
        sys.exit(f'jevr failed (exit {proc.returncode}): {proc.stderr.strip()}')
    return (json.loads(proc.stdout) if proc.stdout.strip() else []), proc.stderr


def run(name: str, jobs: list, offline: bool, workers: int = 1) -> list:
    """Every job's results in job order, after printing the run's request and warning counts."""
    env = environment(CACHE / name, offline)
    outcomes = []
    with ThreadPoolExecutor(workers) as pool:
        for i, outcome in enumerate(pool.map(lambda job: job(env), jobs), 1):
            outcomes.append(outcome)
            if i % 100 == 0 or i == len(jobs):
                print(f'{i}/{len(jobs)} jobs done')
    requests = sum(stderr.count(REQUEST) for _, stderr in outcomes)
    warnings = sum(stderr.count('jevr: warning:') for _, stderr in outcomes)
    state = 'missing from the cache' if offline else 'billed'
    print(f'{requests} requests {state}, {warnings} warnings')
    return [results for results, _ in outcomes]


def spotcheck(name: str, jobs: list, seed: str, n: int) -> None:
    """Re-send a seeded sample of jobs live and report how far the answers moved."""
    sample = random.Random(seed).sample(jobs, n)
    cached = CACHE / name / 'jevr'
    with tempfile.TemporaryDirectory() as tmp:
        live, seeded = Path(tmp, 'live'), Path(tmp, 'seeded')
        billed = sum(job(environment(live, False))[1].count(REQUEST) for job in sample)
        verify = responses(live, rerank=False)
        (seeded / 'jevr').mkdir(parents=True)
        for key in verify:
            if (cached / key).exists():
                shutil.copyfile(cached / key, seeded / 'jevr' / key)
        billed += sum(
            job(environment(seeded, False))[1].count(REQUEST) for job in sample
        )
        rerank = responses(seeded, rerank=True)

        models = sorted({r['model'] for r in (*verify.values(), *rerank.values())})
        print(
            f'{n} jobs (seed {seed!r}), {billed} requests billed, models: {", ".join(models)}'
        )
        report('verification', verify, cached)
        report('rerank', rerank, cached)


def responses(cache_home: Path, rerank: bool) -> dict:
    """{key: response} of one stage's entries under cache_home."""
    entries = (
        (e.name, json.loads(e.read_text(encoding='utf-8')))
        for e in (cache_home / 'jevr').glob('*.json')
    )
    return {key: r for key, r in entries if ('rank' in r['answers']) == rerank}


def probabilities(response: dict) -> dict:
    """{answer: probability}; a choice answer contributes one per option."""
    flat = {}
    for key, answer in response['answers'].items():
        if answer['type'] == 'noul':
            flat[key] = answer['noul']
        else:
            flat |= {f'{key}/{opt}': p for opt, p in answer['probabilities'].items()}
    return flat


def report(stage: str, fresh: dict, cached: Path) -> None:
    """Print how far the fresh responses moved from their cached twins."""
    deltas, missing = [], 0
    for key, response in fresh.items():
        if not (cached / key).exists():
            missing += 1
            continue
        twin = json.loads((cached / key).read_text(encoding='utf-8'))
        live, kept = probabilities(response), probabilities(twin)
        deltas += [abs(live[k] - kept[k]) for k in live.keys() & kept.keys()]
    moved = (
        f'|Δp| mean {statistics.fmean(deltas):.4f}, max {max(deltas):.3f} over {len(deltas)} answers'
        if deltas
        else 'no answers compared'
    )
    print(
        f'{stage}: {len(fresh) - missing} requests paired, {missing} missing from the cache; {moved}'
    )
