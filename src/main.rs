//! CLI entry point: wires Stage 0–3 together.
//!
//! Pipeline: walk → index refresh → keyword variants → candidate union →
//! concurrent Jev verification → listwise rerank → render. Exit codes follow
//! the grep convention for Unix composability: 0 = matches, 1 = none,
//! 2 = usage/config error.

use anyhow::{Context, Result};
use clap::Parser;
use jevr::{
    bm25, candidates,
    config::Config,
    expansion,
    jev::client::JevClient,
    output::{self, Mode, RankedFile},
    rerank, verify, walk,
};
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::ExitCode;

const AFTER_HELP: &str = "\
Examples:
  jevr \"where is the websocket reconnect logic?\"     search current dir
  jevr \"exchange fee calculation\" src/arbitrage      search a subtree
  jevr \"how do I configure retry backoff?\" docs/     search documents
  jevr \"crash recovery\" --snippets                   show matching lines

Works on source code and plain-text documents alike (markdown, rst, txt,
html, org, tex, ...); PATH may be a directory or a single file. Code and
document results are gated and ranked separately — code keeps score >= 0.90,
documents >= 0.60 (prose scores run lower than code scores for equally
relevant content) — and document results print after code results.

Output lines are `path:start-end  score` — the line range of the best-matching
content and a 0..1 relevance score. Results are ordered by relevance; plain
natural-language questions rank best (keyword lists are unnecessary).

Exit codes follow grep: 0 = matches, 1 = none above threshold (the best few
below-threshold guesses may still print, flagged on stderr), 2 = error.
Requires TYPESAFE_API_KEY; results come from a network AI judge, so a cold
query takes a few seconds (repeats are cached and instant).";

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Semantic search: find the code or document files matching a concept, described in plain language",
    after_help = AFTER_HELP
)]
struct Args {
    /// What to look for, in natural language (a question or concept, not a regex).
    #[arg(value_name = "QUERY")]
    query: String,
    /// Directory or file to search (default: current directory).
    #[arg(value_name = "PATH", conflicts_with = "path")]
    path_pos: Option<PathBuf>,
    /// Directory or file to search (same as the positional PATH).
    #[arg(short, long, default_value = ".")]
    path: PathBuf,
    /// Extra comma-separated keyword variant for candidate retrieval
    /// (repeatable, ≤4). Rarely needed: bare queries usually rank better.
    #[arg(short, long = "keywords")]
    keywords: Vec<String>,
    /// File of repo-relative candidate paths (one per line); verifies only
    /// those files instead of discovering candidates (`-k`, `--no-grep-union`
    /// and `--hidden` are ignored).
    #[arg(long)]
    candidates: Option<PathBuf>,
    /// Minimum relevance score (0..1) to keep a file (defaults from config:
    /// 0.90 for code files, 0.60 for text documents; setting this applies one
    /// value to both).
    #[arg(long)]
    threshold: Option<f64>,
    /// Maximum files printed, applied separately to code results and document
    /// results (default from config: 10).
    #[arg(long)]
    top_k: Option<usize>,
    /// Jev model (default from config: pinned jev-1.13.0).
    #[arg(long)]
    model: Option<String>,
    /// Explicit YAML config file.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Disable the git-grep literal-vocabulary union.
    #[arg(long)]
    no_grep_union: bool,
    /// Disable the response cache (~/.cache/jevr); every request
    /// hits the network and is billed.
    #[arg(long)]
    no_cache: bool,
    /// Include hidden files and directories.
    #[arg(long)]
    hidden: bool,
    /// Concurrent verification requests (default from config: 32).
    #[arg(long)]
    concurrency: Option<usize>,
    /// Compact `path:start-end  score` lines (default).
    #[arg(long, group = "mode")]
    paths_only: bool,
    /// Also print each file's best-matching code, with line numbers
    /// (shows the exact symbol names to use in grep follow-ups).
    #[arg(long, group = "mode")]
    snippets: bool,
    /// Full verdict list as JSON.
    #[arg(long, group = "mode")]
    json: bool,
}

fn main() -> ExitCode {
    let args = Args::parse();
    match run(args) {
        Ok(found) => {
            if found {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(1)
            }
        }
        Err(e) => {
            eprintln!("jevr: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(args: Args) -> Result<bool> {
    if args.query.trim().is_empty() {
        anyhow::bail!("query is empty; describe what to look for in natural language");
    }
    let given = args.path_pos.clone().unwrap_or_else(|| args.path.clone());
    if !given.exists() {
        anyhow::bail!("{}: no such file or directory", given.display());
    }
    // grep accepts a single file as its PATH; so do we. The file becomes the
    // sole candidate and its parent directory the search root.
    let (root, single_file) = if given.is_file() {
        let file = given
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .context("path has no file name")?;
        let parent = match given.parent() {
            Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
            _ => PathBuf::from("."),
        };
        (parent, Some(file))
    } else {
        (given, None)
    };
    let mut config = Config::load(&root, args.config.as_deref())?;
    if let Some(threshold) = args.threshold {
        config.threshold = threshold;
        config.doc_threshold = threshold;
    }
    if let Some(top_k) = args.top_k {
        config.top_k = top_k;
    }
    if let Some(model) = args.model.clone() {
        config.model = model;
    }
    if let Some(concurrency) = args.concurrency {
        config.concurrency = concurrency;
    }
    if args.no_grep_union {
        config.git_grep_union = false;
    }
    if args.no_cache {
        config.cache = false;
    }
    if config.bm25_k1 <= 0.0 || !(0.0..=1.0).contains(&config.bm25_b) {
        anyhow::bail!("bm25_k1 must be > 0 and bm25_b between 0 and 1");
    }
    if !(0.0..=1.0).contains(&config.threshold) || !(0.0..=1.0).contains(&config.doc_threshold) {
        anyhow::bail!("threshold must be between 0 and 1");
    }
    if !config.doc_rank_weight.is_finite() || config.doc_rank_weight < 0.0 {
        anyhow::bail!("doc_rank_weight must be a finite value >= 0");
    }
    if config.concurrency == 0 {
        anyhow::bail!("concurrency must be at least 1");
    }
    if config.candidate_cap == 0 || config.top_k == 0 || config.snippet_lines == 0 {
        anyhow::bail!("candidate_cap, top_k and snippet_lines must be ≥ 1");
    }
    let key = std::env::var("TYPESAFE_API_KEY")
        .context("TYPESAFE_API_KEY is required (export it before running)")?;
    let mode = if args.snippets {
        Mode::Snippets
    } else if args.json {
        Mode::Json
    } else {
        Mode::PathsOnly
    };

    // Stage 1 (local, synchronous, stateless): walk, BM25-score, union.
    // An explicit --candidates file bypasses the whole stage (Unix seam for
    // external generators); otherwise the eager BM25 index is rebuilt per
    // invocation — at repo scale the cost is file I/O + tokenization, well
    // under the 1 s Stage-1 budget, so no index directory is persisted.
    let candidate_files: Vec<String> = if let Some(file) = single_file {
        vec![file]
    } else if let Some(list) = &args.candidates {
        std::fs::read_to_string(list)
            .with_context(|| format!("read candidates file {}", list.display()))?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect()
    } else {
        let walked = walk::searchable_files(&root, args.hidden)?;
        if walked.is_empty() {
            // Distinguish "nothing indexable here" from "nothing relevant":
            // silence would read as a legitimate empty search result.
            eprintln!(
                "jevr: no searchable files under {} (code or text documents)",
                root.display()
            );
            return Ok(false);
        }
        let variants = expansion::variants(&args.query, &args.keywords);
        // BM25 candidates are selected per kind: documents are wordier than
        // code and, sharing one index, crowd code out of the candidate_cap
        // slots — a shared index measurably lost correct code files on
        // doc-heavy repos (docs/BENCHMARKS.md). Separate indexes keep the code
        // lane's candidate selection independent of how many docs exist.
        let mut variant_lists: Vec<Vec<String>> = Vec::new();
        for kind in [walk::FileKind::Code, walk::FileKind::Doc] {
            let lane: Vec<PathBuf> = walked
                .iter()
                .filter(|p| walk::kind_of(p) == kind)
                .cloned()
                .collect();
            if lane.is_empty() {
                continue;
            }
            let index =
                bm25::Bm25Index::build_from_files(&root, &lane, config.bm25_k1, config.bm25_b);
            variant_lists.extend(
                variants
                    .iter()
                    .map(|variant| index.top(variant, config.candidate_cap)),
            );
        }
        let walked_rel: BTreeSet<String> = walked
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let grep_hits = if config.git_grep_union {
            let grep_keywords: Vec<String> = variants
                .iter()
                .flat_map(|v| v.split(','))
                .map(|k| k.trim().to_owned())
                .filter(|k| !k.is_empty())
                .collect();
            candidates::git_grep_hits(&root, &grep_keywords, &walked_rel)
        } else {
            Vec::new()
        };
        candidates::union_candidates(&variant_lists, &grep_hits)
    };
    if candidate_files.is_empty() {
        return Ok(false);
    }

    // Stages 2–3 (Jev, async): verify concurrently, then rerank the kept set.
    // Current-thread runtime: the workload is pure HTTP I/O, worker threads
    // would add nothing. Head snippets are captured from the same read that
    // feeds windowing, so no file is read twice.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    // User-level cache dir (XDG override honored): keeps searched repositories
    // free of tool artifacts while letting repeated runs skip billed requests.
    let cache_dir = if config.cache {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .map(|base| base.join("jevr"))
    } else {
        None
    };
    let client = JevClient::new(key, cache_dir);
    let mut batches = Vec::new();
    let mut head_by_file: std::collections::BTreeMap<String, String> = Default::default();
    for rel in &candidate_files {
        let abs = root.join(rel);
        let Ok(text) = std::fs::read_to_string(&abs) else {
            eprintln!("jevr: warning: skipping unreadable {rel}");
            continue;
        };
        head_by_file.insert(
            rel.clone(),
            text.lines().take(25).collect::<Vec<_>>().join("\n"),
        );
        batches.extend(verify::build_batches(
            rel,
            walk::kind_of(std::path::Path::new(rel)),
            &text,
        ));
    }
    let verdicts = runtime.block_on(verify::verify_files(
        &client,
        &config.model,
        &args.query,
        batches,
        config.concurrency,
    ));
    // Result lanes: code and documents keep separate thresholds, separate
    // empty-result fallbacks, and separate reranks. Code results are ranked
    // among themselves with the measured "implements" Choice and are never
    // displaced, reordered, or fallback-suppressed by documents — a shared
    // fallback measurably let kept documents silence the code lane's
    // below-threshold guesses and lose the right file (docs/BENCHMARKS.md);
    // documents rank among themselves with the coverage phrasing and print
    // after code. Each lane is capped at top_k.
    let (code_lane, doc_lane): (Vec<_>, Vec<_>) = verdicts
        .into_iter()
        .filter(|v| v.error.is_none())
        .partition(|v| walk::kind_of(std::path::Path::new(&v.file)) == walk::FileKind::Code);
    let mut ranked: Vec<RankedFile> = Vec::new();
    let mut any_real_match = false;
    let (mut code_shown, mut doc_shown) = (0usize, 0usize);
    for (lane, gate, any_doc, label) in [
        (code_lane, config.threshold, false, "code file"),
        (doc_lane, config.doc_threshold, true, "document"),
    ] {
        if lane.is_empty() {
            continue;
        }
        let (mut lane_kept, lane_rest): (Vec<_>, Vec<_>) =
            lane.into_iter().partition(|v| v.score >= gate);
        // Per-lane fallback: on out-of-envelope queries (e.g. bug reports)
        // nothing may clear the gate; the best few verdicts marked `fallback`
        // beat silence for a calling agent, which can judge by score.
        let lane_fallback = lane_kept.is_empty() && config.empty_fallback > 0;
        if lane_fallback {
            lane_kept = lane_rest;
            lane_kept.sort_by(|a, b| b.score.total_cmp(&a.score));
            lane_kept.truncate(config.empty_fallback);
            lane_kept.retain(|v| v.score > 0.0);
            if !lane_kept.is_empty() {
                eprintln!(
                    "jevr: no {label} scored >= {gate:.2}; showing the best {} below-threshold guesses",
                    lane_kept.len()
                );
            }
        }
        lane_kept.sort_by(|a, b| b.score.total_cmp(&a.score));
        if lane_kept.is_empty() {
            continue;
        }
        if !lane_fallback {
            any_real_match = true;
        }
        let lane = lane_kept;
        let fallback = lane_fallback;
        let heads: Vec<(String, String)> = lane
            .iter()
            .map(|v| {
                let head = head_by_file.get(&v.file).cloned().unwrap_or_default();
                (v.file.clone(), head)
            })
            .collect();
        let rank_probabilities = match runtime.block_on(rerank::rank_kept(
            &client,
            &config.model,
            &args.query,
            &heads,
            any_doc,
        )) {
            Ok(lane_ranked) => Some(
                lane_ranked
                    .into_iter()
                    .collect::<std::collections::BTreeMap<_, _>>(),
            ),
            Err(e) => {
                eprintln!("jevr: warning: rerank failed, ordering by score: {e}");
                None
            }
        };
        let mut lane_ranked: Vec<RankedFile> = lane
            .into_iter()
            .map(|verdict| RankedFile {
                rank_probability: rank_probabilities
                    .as_ref()
                    .and_then(|m| m.get(&verdict.file).copied()),
                fallback,
                verdict,
            })
            .collect();
        if any_doc {
            // Documents order by score + doc_rank_weight × rank_probability:
            // a single listwise Choice is winner-take-all, which ranks well
            // when one file holds the answer but under-resolves the rest when
            // many documents are relevant; fusing in the calibrated verify
            // score fixed that (measured, docs/BENCHMARKS.md). Code keeps pure
            // rerank order below — the all-code path is unaffected by fusion.
            let w = config.doc_rank_weight;
            let key = |r: &RankedFile| r.verdict.score + w * r.rank_probability.unwrap_or(0.0);
            lane_ranked.sort_by(|a, b| key(b).total_cmp(&key(a)));
        } else if rank_probabilities.is_some() {
            lane_ranked.sort_by(|a, b| {
                b.rank_probability
                    .unwrap_or(0.0)
                    .total_cmp(&a.rank_probability.unwrap_or(0.0))
            });
        }
        lane_ranked.truncate(config.top_k);
        if !fallback {
            if any_doc {
                doc_shown = lane_ranked.len();
            } else {
                code_shown = lane_ranked.len();
            }
        }
        ranked.extend(lane_ranked);
    }
    // One stderr line whenever documents print: the lanes keep different
    // gates, so sub-0.90 document scores after 0.9x code scores read as
    // broken filtering to an agent that only knows the code threshold.
    // Code-only runs stay stderr-silent.
    if doc_shown > 0 {
        let docs = format!(
            "{doc_shown} document result(s) kept at score >= {:.2}",
            config.doc_threshold
        );
        if code_shown > 0 {
            eprintln!(
                "jevr: {code_shown} code result(s) kept at score >= {:.2}, then {docs}",
                config.threshold
            );
        } else {
            eprintln!("jevr: {docs}");
        }
    }
    if ranked.is_empty() {
        return Ok(false);
    }
    // Print paths the way grep does — prefixed with the search root as given —
    // so a caller can open them directly from its own working directory.
    if root.as_os_str() != "." {
        for r in &mut ranked {
            r.verdict.file = root.join(&r.verdict.file).to_string_lossy().into_owned();
        }
    }
    let shown = ranked.len();
    output::render(mode, &ranked, shown, config.snippet_lines)?;
    // Fallback results are guesses, not matches: exit 1 unless some lane holds
    // a real match, keeping the grep convention honest while the printed hints
    // remain available.
    Ok(any_real_match)
}
