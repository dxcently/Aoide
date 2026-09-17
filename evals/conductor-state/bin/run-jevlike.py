"""Score a JSONL of {id, context, options, label} through a jevlike checkpoint,
one row per line: {id, pred, prob, margin, ms}. jevlike-predict starts python
per case; this loads once."""
import json, os, sys, time
sys.path.insert(0, os.environ.get("JEVLIKE", os.path.expanduser("~/jevlike")))
import torch
from jevlike.data import ChoiceExample
from jevlike.model import load_checkpoint, select_device
from jevlike.train import move

ckpt, path = sys.argv[1], sys.argv[2]
device = select_device("cpu")
model, collator, _ = load_checkpoint(ckpt, device)
model.eval()
for raw in open(path):
    row = json.loads(raw)
    t0 = time.perf_counter()
    batch = move(collator([ChoiceExample(row["context"], tuple(row["options"]), 0)]), device)
    with torch.no_grad():
        probs = model(batch).softmax(-1)[0, :len(row["options"])].cpu().tolist()
    ms = (time.perf_counter() - t0) * 1000
    ranked = sorted(range(len(probs)), key=lambda i: -probs[i])
    print(json.dumps({"id": row["id"], "pred": row["options"][ranked[0]], "prob": probs[ranked[0]],
                      "margin": probs[ranked[0]] - probs[ranked[1]], "ms": round(ms, 2)}))
