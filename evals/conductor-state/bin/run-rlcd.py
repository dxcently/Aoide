"""RLCD-style lane: an instruct Qwen, no training, one prefill, every field's
candidates scored in one batched pass over a shared KV cache — the shape of
harshatheg/Qwen-2.5-1B-RLCD (engine_torch: base prompt cached once, per-field
suffix `"<field>": "`, candidate logits sliced, softmax). One deviation, kept
on purpose: a candidate is scored by the summed log-probability of ALL its
tokens, not its first token alone — our labels share prefixes ("stalled",
"steer", "settled").

  run-rlcd.py <cases.jsonl> line|tail > pred rows
    line: the delexicalized state line (what verba-volantia sees)
    tail: the raw journal tail + last say (what only a text model can see)
"""
import json, os, sys, time
import torch
from transformers import AutoModelForCausalLM, AutoTokenizer

MODEL = os.environ.get("RLCD_MODEL", "Qwen/Qwen2.5-1.5B-Instruct")
CONDITIONS = {
    "progressing": "calls returning, work advancing", "wrapping-up": "told to finish, writing its report",
    "stalled": "turn open, nothing happening for a long time", "looping": "repeating the same call with the same result",
    "failing": "recent calls keep erroring", "blocked": "waiting on a question nobody answered",
    "dead": "process gone with the turn still open", "exhausted": "call budget spent without finishing",
    "misrouted": "a message reached a session it was not meant for", "settled": "turn ended normally",
    "cancelled": "turn was cancelled",
}
DECISIONS = {
    "wait": "leave it alone", "nudge": "tell it to wrap up", "steer": "send a correction",
    "answer": "answer its question", "resume": "restart the dead or spent turn", "cancel": "stop it",
    "collect": "read what it produced", "escalate": "hand it to the operator",
}
RISK = {
    "none": "nothing at stake", "confidentiality": "something could be disclosed (DAD: disclosure)",
    "integrity": "something could be altered (DAD: alteration)", "availability": "something could be stopped or destroyed (DAD: denial)",
}
FIELDS = {"condition": CONDITIONS, "decision": DECISIONS, "risk": RISK}

def schema_text():
    return json.dumps({f: {"type": "enum", "choices": [f"{k}: {v}" for k, v in c.items()]} for f, c in FIELDS.items()}, indent=1)

def context_of(row, mode):
    if mode == "tail" and row.get("tail"):
        return f"agent {row['node']} · journal tail:\n{row['tail']}\n\nlast said: {row.get('say') or '(nothing)'}\nprocess alive: {row['alive']}"
    return f"agent state line: {row['line']}\nbindings: {json.dumps(row['bind'])}"

def main():
    path, mode = sys.argv[1], sys.argv[2]
    tok = AutoTokenizer.from_pretrained(MODEL)
    model = AutoModelForCausalLM.from_pretrained(MODEL, torch_dtype=torch.float32).eval()
    system = "You are a calibrated decision engine. Select the single most accurate option based on evidence.\nClassify JSON attributes:\n" + schema_text()
    for raw in open(path):
        row = json.loads(raw)
        t0 = time.perf_counter()
        msgs = [{"role": "system", "content": system}, {"role": "user", "content": context_of(row, mode)}]
        base = tok.apply_chat_template(msgs, tokenize=False, add_generation_prompt=True) + "{\n"
        base_ids = tok(base, return_tensors="pt").input_ids
        with torch.no_grad():
            cache = model(base_ids, use_cache=True).past_key_values
        out = {"id": row["id"], "runner": f"rlcd-{mode}", "split": row.get("split", "seed")}
        for field, choices in FIELDS.items():
            suffixes = [f'  "{field}": "{c}' for c in choices]
            enc = [tok(s, add_special_tokens=False).input_ids for s in suffixes]
            head = len(tok(f'  "{field}": "', add_special_tokens=False).input_ids)
            L = max(len(e) for e in enc); K = len(enc)
            ids = torch.full((K, L), tok.pad_token_id, dtype=torch.long)
            for i, e in enumerate(enc): ids[i, :len(e)] = torch.tensor(e)
            mask = torch.cat([torch.ones(K, base_ids.shape[1], dtype=torch.long), (ids != tok.pad_token_id).long()], 1)
            import copy
            c = copy.deepcopy(cache); c.batch_repeat_interleave(K)
            with torch.no_grad():
                logits = model(ids, attention_mask=mask, past_key_values=c).logits.log_softmax(-1)
            # the first suffix token is predicted by the base prompt's last position; token j by suffix position j-1
            with torch.no_grad():
                base_last = model(base_ids).logits[0, -1].log_softmax(-1)
            scores = []
            for i, e in enumerate(enc):
                lp = base_last[e[0]].item() if head == 0 else 0.0
                for j in range(max(head, 1), len(e)):
                    lp += logits[i, j - 1, e[j]].item()
                scores.append(lp)
            probs = torch.tensor(scores).softmax(-1).tolist()
            ranked = sorted(range(K), key=lambda i: -probs[i])
            out[field] = list(choices)[ranked[0]]; out[f"{field}_prob"] = probs[ranked[0]]
            out[f"{field}_margin"] = probs[ranked[0]] - probs[ranked[1]]
        out["ms"] = round((time.perf_counter() - t0) * 1000, 1)
        print(json.dumps(out), flush=True)

if __name__ == "__main__":
    main()
