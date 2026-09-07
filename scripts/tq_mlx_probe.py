#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import os
import pathlib
import shlex
import time


ROOT = pathlib.Path(__file__).resolve().parents[1]
PROMPTS = [
    ("smoke", ROOT / "docs/turboquant/prompts/smoke.txt", 8, "exact_OK"),
    ("context_fill", ROOT / "docs/turboquant/prompts/context_fill.txt", 64, "non_empty_summary"),
    ("retrieval", ROOT / "docs/turboquant/prompts/retrieval.txt", 16, "exact_TURBO-314159"),
    ("instruct", ROOT / "docs/turboquant/prompts/instruct.txt", 48, "exact_JSON_contract"),
]


def parse_args() -> argparse.Namespace:
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)

    run = sub.add_parser("run")
    run.add_argument("--model", required=True)
    run.add_argument("--out", required=True)
    run.add_argument("--ctx", type=int, default=8192)
    run.add_argument(
        "--python",
        default=os.environ.get("CX_TQ_MLX_PYTHON", "/tmp/cx_mlx_env/bin/python"),
        help="Kept for artifact provenance; the script itself should already be running under the desired interpreter.",
    )
    run.add_argument(
        "--preferred-args",
        default="",
        help="Registry-backed MLX args applied before CX_MLX_ARGS override parsing.",
    )
    run.add_argument(
        "--mlx-args",
        default="",
        help="Explicit CX_MLX_ARGS override string for the benchmark harness.",
    )
    return ap.parse_args()


def parse_runtime_args(*raw_values: str) -> dict[str, object]:
    config: dict[str, object] = {
        "temperature": 0.0,
        "top_p": 1.0,
        "min_p": 0.0,
        "top_k": 0,
        "seed": None,
    }
    ignored: list[str] = []
    tokens: list[str] = []
    for raw in raw_values:
        if raw.strip():
            tokens.extend(shlex.split(raw))

    i = 0
    while i < len(tokens):
        token = tokens[i]
        if token in {"--temp", "--temperature"} and i + 1 < len(tokens):
            config["temperature"] = float(tokens[i + 1])
            i += 2
            continue
        if token == "--top-p" and i + 1 < len(tokens):
            config["top_p"] = float(tokens[i + 1])
            i += 2
            continue
        if token == "--min-p" and i + 1 < len(tokens):
            config["min_p"] = float(tokens[i + 1])
            i += 2
            continue
        if token == "--top-k" and i + 1 < len(tokens):
            config["top_k"] = int(tokens[i + 1])
            i += 2
            continue
        if token == "--seed" and i + 1 < len(tokens):
            config["seed"] = int(tokens[i + 1])
            i += 2
            continue
        ignored.append(token)
        if i + 1 < len(tokens) and not tokens[i + 1].startswith("--"):
            ignored.append(tokens[i + 1])
            i += 2
            continue
        i += 1

    config["ignored_args"] = ignored
    return config


def judge(prompt_name: str, response: str) -> tuple[bool, str]:
    if prompt_name == "smoke":
        return response == "OK", "exact_OK"
    if prompt_name == "retrieval":
        return response == "TURBO-314159", "exact_TURBO-314159"
    if prompt_name == "context_fill":
        return bool(response.strip()), "non_empty_summary"
    if prompt_name == "instruct":
        try:
            obj = json.loads(response)
        except Exception:
            return False, "exact_JSON_contract"
        return obj == {
            "status": "ready",
            "focus": "turboquant-baseline",
            "next_step": "phase2-v-cache",
        }, "exact_JSON_contract"
    return False, "unknown"


def main() -> int:
    ns = parse_args()
    if ns.cmd != "run":
        return 1

    import mlx.core as mx
    from mlx_lm import load, stream_generate
    from mlx_lm.models import cache
    from mlx_lm.sample_utils import make_sampler

    runtime_config = parse_runtime_args(ns.preferred_args, ns.mlx_args)
    if runtime_config["seed"] is not None:
        mx.random.seed(runtime_config["seed"])
    model, tokenizer = load(ns.model)
    sampler = make_sampler(
        runtime_config["temperature"],
        runtime_config["top_p"],
        runtime_config["min_p"],
        1,
        top_k=runtime_config["top_k"],
        xtc_probability=0.0,
        xtc_threshold=0.0,
        xtc_special_tokens=tokenizer.encode("\n") + list(tokenizer.eos_token_ids),
    )

    results = []
    for name, prompt_file, max_tokens, _rule in PROMPTS:
        prompt = prompt_file.read_text()
        prompt_text = tokenizer.apply_chat_template(
            [{"role": "user", "content": prompt}],
            tokenize=False,
            add_generation_prompt=True,
        )
        prompt_cache = cache.make_prompt_cache(model, max_kv_size=ns.ctx)
        mx.reset_peak_memory()
        start = time.monotonic()
        text = ""
        last = None
        for resp in stream_generate(
            model,
            tokenizer,
            prompt_text,
            max_tokens=max_tokens,
            sampler=sampler,
            max_kv_size=ns.ctx,
            prompt_cache=prompt_cache,
        ):
            text += resp.text
            last = resp
        mx.synchronize()
        wall_ms = round((time.monotonic() - start) * 1000)
        response_text = text.strip()
        passed, quality_rule = judge(name, response_text)
        cache_nbytes = sum(c.nbytes for c in prompt_cache)
        cache_tokens = max((getattr(c, "size", lambda: 0)() for c in prompt_cache), default=0)
        results.append(
            {
                "mode": "mlx",
                "prompt_name": name,
                "prompt_file": str(prompt_file.relative_to(ROOT)),
                "context_target": ns.ctx,
                "predict_n": max_tokens,
                "quality_rule": quality_rule,
                "passed": passed,
                "response_text": response_text,
                "prompt_tokens_per_sec": None if last is None else last.prompt_tps,
                "decode_tokens_per_sec": None if last is None else last.generation_tps,
                "peak_memory_gb": None if last is None else last.peak_memory,
                "cache_nbytes": cache_nbytes,
                "cache_tokens": cache_tokens,
                "wall_ms": wall_ms,
            }
        )

    payload = {
        "contract_version": "turboquant-mlx.v2",
        "backend": "mlx",
        "python": ns.python,
        "model": ns.model,
        "context_target": ns.ctx,
        "runtime_config": runtime_config,
        "runs": results,
        "passes": sum(1 for r in results if r["passed"]),
        "total": len(results),
        "metric_note": "MLX now records both peak_memory_gb and live cache_nbytes. cache_nbytes is the preferred cache-footprint metric on this backend path.",
    }
    pathlib.Path(ns.out).write_text(json.dumps(payload, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
